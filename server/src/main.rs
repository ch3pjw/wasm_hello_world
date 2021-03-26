use {
    crypto::{
        digest::Digest,
        sha1::Sha1,
    },
    futures::{
        stream, StreamExt, SinkExt,
        FutureExt,
        future::{join_all, Either::{Left, Right}},
        channel::mpsc, channel::oneshot
    },
    hyper::{
        Body,
        body::Bytes,
        Request,
        Response,
        Server,
        StatusCode,
        header,
        http,
    },
    log::{info, error, warn, LevelFilter},
    simple_logger::SimpleLogger,
    std::{
        str,
        collections::BTreeMap,
        net::SocketAddr,
    },
    tokio::signal::unix::{signal, Signal, SignalKind},
    tokio_tungstenite::{
        tungstenite::protocol::{Role, Message},
        WebSocketStream,
    },
};

mod service;

use crate::service::ConnectionHandler;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    SimpleLogger::new()
        .with_module_level("mio", LevelFilter::Warn)
        .with_module_level("tokio_tungstenite", LevelFilter::Warn)
        .with_module_level("tungstenite", LevelFilter::Warn)
        .init()
        .unwrap();
    let sigint = signal(SignalKind::interrupt()).expect("failed to set up signal handler");
    let app = App::new(sigint);
    app.serve(&SocketAddr::from(([0, 0, 0, 0], 8080))).await?;
    Ok(())
}

struct App {
    sigint: Option<Signal>,
    shutting_down: bool,
    clients: BTreeMap<u16, Client>,
    next_client_id: u16,
}

impl App {
    pub fn new(sigint: Signal) -> Self {
        App {
            sigint: Some(sigint),
            shutting_down: false,
            clients: BTreeMap::new(),
            next_client_id: 0,
        }
    }

    pub async fn serve(mut self, addr: &SocketAddr) -> Result<(), hyper::Error> {
        // We have to be able to take the Signal out of self so that we can pass self.app_main to
        // spawn...
        let (graceful_rx, app_main_shutdown_rx) = Self::watch_sigint(self.sigint.take().unwrap());
        let (conn_handler, cmd_rx) = ConnectionHandler::new(&handle_request);
        let app_main_handle = tokio::task::spawn(self.app_main(cmd_rx, app_main_shutdown_rx));
        let server_builder = Server::bind(addr);
        info!("Visit http://{}/index.html to start", &addr);
        let server = server_builder
            .serve(conn_handler)
            .with_graceful_shutdown(graceful_rx.map(|r| r.expect("cancelled instead")));
        // If we just do this one next on the stream, there's no point in wrapping the
        // Signal::recv()...
        server.await?;
        Ok(app_main_handle.await.expect("join error"))
    }

    fn watch_sigint(mut sigint: Signal) -> (oneshot::Receiver<()>, mpsc::Receiver<AppShutdown>) {
        let (graceful_tx, graceful_rx) = oneshot::channel();
        let (mut app_main_tx, app_main_rx) = mpsc::channel(2);
        tokio::task::spawn(async move {
            sigint.recv().await;
            app_main_tx.send(AppShutdown::Soft).await.expect("bad send");
            graceful_tx.send(()).expect("bad graceful send");
            sigint.recv().await;
            app_main_tx.send(AppShutdown::Hard).await.expect("bad send");
        });
        (graceful_rx, app_main_rx)
    }

    async fn app_main(mut self, rx: mpsc::UnboundedReceiver<AppCmd>, shutdown_rx: mpsc::Receiver<AppShutdown>) {
        let mut both = stream::select(
            shutdown_rx.map(|x| Left(x)),
            rx.map(|x| Right(x))
        );
        loop {
            match both.next().await {
                Some(Left(shutdown)) => match shutdown {
                    AppShutdown::Soft => {
                        if self.clients.len() == 0 {
                            warn!("SIGINT - no clients, bye!");
                            break
                        } else {
                            warn!("SIGINT - waiting for clients to disconnect. Interrupt again to force-quit");
                            self.shutting_down = true;
                        }
                    },
                    AppShutdown::Hard => {
                        warn!("Hard app shutdown, closing remaining connections...");
                        self.send_all(Message::Close(None)).await;
                        break
                    }
                },
                Some(Right(cmd)) => match cmd {
                    AppCmd::NewClient(mut client_tx) => {
                        let id = self.next_client_id;
                        info!("new client ({}) connected!", id);
                        client_tx.send(ClientEvent::ClientId(id)).await;
                        let result = self.clients.insert(id, Client { tx: client_tx });
                        // FIXME: replace with Option::expect_none() when in stable:
                        if let Some(_client) = result { panic!("client ID already in map") }
                        self.next_client_id += 1;
                    },
                    AppCmd::ClientMsg(client_id, msg) => match msg {
                        Message::Binary(b) => todo!(),
                        Message::Text(s) => {
                            info!("Server received text {:?}", s);
                            self.send_all(Message::Text(s.clone())).await;
                        }
                        Message::Ping(b) => {
                            self.clients.get_mut(&client_id).expect("no client?").tx.send(
                                ClientEvent::AppMsg(Message::Pong(b))
                            ).await;
                        },
                        Message::Pong(b) => todo!(),
                        Message::Close(b) => {
                            warn!("client {} disconnected with {:?}", client_id, b);
                            self.clients.remove(&client_id).expect("no client in map");
                            if self.shutting_down && self.clients.len() == 0 {
                                info!("Last client left, bye!");
                                break
                            }
                        }
                    }
                }
                None => break
            }
        }
    }

    async fn send_all(&mut self, msg: Message) {
        join_all(self.clients.values_mut().map(
            |client| client.tx.send(ClientEvent::AppMsg(msg.clone()))
        )).await;
    }
}

enum AppCmd {
    NewClient(mpsc::UnboundedSender<ClientEvent>),
    ClientMsg(u16, Message),
}

enum AppShutdown { Soft, Hard }

struct Client {
    tx: mpsc::UnboundedSender<ClientEvent>
}

enum ClientEvent {
    ClientId(u16),
    AppMsg(Message),
}

fn handle_request(req: Request<Body>, tx: mpsc::UnboundedSender<AppCmd>) -> Result<Response<Body>, http::Error> {
    if req.method() != http::Method::GET {
        err_resp(StatusCode::METHOD_NOT_ALLOWED)
    } else if req.headers().contains_key(header::UPGRADE) {
        // TODO: The URI scheme doesn't seem to get supplied, so we can't use that to switch
        // handler :-(
        handle_ws(&tx, req)
    } else {
        handle_get(req)
    }
}

const CLIENT_HTML_TEMPLATE: &[u8] = include_bytes!("../templates/index.html.template");

fn generate_client_html(host: &[u8]) -> Bytes {
    // TODO: can we do this up front?
    let mut head = CLIENT_HTML_TEMPLATE;
    let mut tail = &CLIENT_HTML_TEMPLATE[0..0];
    for (i, pair) in CLIENT_HTML_TEMPLATE.windows(2).enumerate() {
        if pair == b"{}" {
            head = &CLIENT_HTML_TEMPLATE[..i];
            tail = &CLIENT_HTML_TEMPLATE[i+2..];
            break
        }
    }
    Bytes::from([head, host, tail].concat())
}

const CLIENT_JS: Bytes = Bytes::from_static(
    include_bytes!("../../client/static/wasm_hello_world.js")
);
const CLIENT_WASM: Bytes = Bytes::from_static(
    include_bytes!("../../client/static/wasm_hello_world_bg.wasm")
);

fn handle_get(req: Request<Body>) -> Result<Response<Body>, http::Error> {
    let b = Response::builder();
    let x = match req.uri().path() {
        "/" | "/index.html" => {
            if let Some(host) = req.headers().get("host") {
                Some(("text/html", generate_client_html(host.as_bytes())))
            } else {
                warn!("Request missing host header!");
                None
            }
        },
        "/wasm_hello_world.js" => Some(("application/javascript", CLIENT_JS)),
        "/wasm_hello_world_bg.wasm" => Some(("application/wasm", CLIENT_WASM)),
        path => {
            warn!("Requested missing path: {}", path);
            None
        }
    };
    match x {
        None => b.status(StatusCode::NOT_FOUND).body(Body::empty()),
        Some((content_type, bytes)) => b
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(bytes)),
    }
}


fn err_resp(code: StatusCode) -> Result<Response<Body>, http::Error> {
    Response::builder().status(code).body(Body::empty())
}

fn handle_ws(tx: &mpsc::UnboundedSender<AppCmd>, mut req: Request<Body>) -> Result<Response<Body>, http::Error> {
    if req.headers().get(header::UPGRADE) != Some(&hv("websocket")) ||
            req.headers().get("sec-websocket-version") != Some(&hv("13")) {
        return err_resp(StatusCode::BAD_REQUEST);
    }
    if req.uri().path() != "/" {
        error!("Bad websocket path: {}", req.uri().path());
        return err_resp(StatusCode::NOT_FOUND);
    }

    let sec_websocket_accept_header = match req.headers().get("Sec-WebSocket-Key") {
        None => { return err_resp(StatusCode::BAD_REQUEST); }
        Some(key) => mk_accept_header(key.as_bytes())
    };

    let tx = tx.clone();
    tokio::task::spawn(async move {
        match hyper::upgrade::on(&mut req).await {
            Ok(upgraded) => {
                if let Err(e) = websocket_dialogue(tx, upgraded).await {
                    error!("server websocket IO error: {}", e)
                }
            },
            Err(e) => error!("upgrade error: {}", e)
        }
    });

    let mut resp = Response::new(Body::empty());
    *resp.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    let headers = resp.headers_mut();
    headers.insert(header::UPGRADE, hv("websocket"));
    headers.insert(header::CONNECTION, hv("Upgrade"));
    headers.insert(
        "Sec-WebSocket-Accept",
        header::HeaderValue::from_str(&sec_websocket_accept_header).expect("this should never fail")
    );
    return Ok(resp);
}

fn hv(string: &'static str) -> header::HeaderValue {
    header::HeaderValue::from_static(string)
}


async fn websocket_dialogue(mut app_tx: mpsc::UnboundedSender<AppCmd>, upgraded: hyper::upgrade::Upgraded) -> Result<(), hyper::Error> {
    let (mut ws_tx, ws_rx) = WebSocketStream::from_raw_socket(upgraded, Role::Server, Default::default())
        .await.split();
    let (client_tx, client_rx) = mpsc::unbounded();
    app_tx.send(AppCmd::NewClient(client_tx)).await;
    let mut client_id = None;

    let mut both = stream::select(
        ws_rx.map(|x| Left(x)),
        client_rx.map(|x| Right(x))
    );
    loop {
        match both.next().await {
            Some(Left(ws_data)) => match ws_data {
                Ok(msg) => {
                    // if let Message::Close(ref x) = msg {
                        // // Make sure we tell the client we accept their close:
                        // ws_tx.send(msg.clone()).await.expect("le fail");
                    // }
                    app_tx.send(AppCmd::ClientMsg(client_id.unwrap(), msg)).await;
                },
                Err(e) => error!("Server errored! {:?}", e)
            },
            Some(Right(client_event)) => match client_event {
                ClientEvent::ClientId(id) => client_id = Some(id),
                ClientEvent::AppMsg(msg) => match msg {
                    Message::Close(x) => {
                        ws_tx.send(Message::Close(x)).await.expect("moar fail sending");
                        warn!("App told client it was closing, terminating dialogue");
                        break Ok(())
                    },
                    _ => ws_tx.send(msg).await.expect("how can sending fail?"),
                }
            },
            None => break Ok(())
        }
    }
}

fn mk_accept_header(key_header: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.input(key_header);
    // A magic UUID that makes it go (https://en.wikipedia.org/wiki/WebSocket)
    hasher.input(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let mut hashed = vec![0u8; hasher.output_bytes()];
    hasher.result(&mut hashed);
    base64::encode(hashed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accept_header() {
        // Example taken from https://en.wikipedia.org/wiki/WebSocket
        assert_eq!(mk_accept_header(b"x3JJHMbDL1EzLkh9GBhXDw=="), "HSmrc0sMlYUkAGmm5OPpG2HaGWk=")
    }
}
