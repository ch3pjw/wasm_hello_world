use {
    crypto::{
        digest::Digest,
        sha1::Sha1,
    },
    futures::{
        StreamExt, SinkExt,
        future::{join_all, select, ok, ready, Ready, Either},
        channel::mpsc,
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
        service::{Service},
    },
    log::{info, error, warn, LevelFilter},
    simple_logger::SimpleLogger,
    std::{
        str,
        convert::Infallible,
        net::SocketAddr,
        task::{Context, Poll},
    },
    tokio_tungstenite::{
        tungstenite::protocol::{Role, Message},
        WebSocketStream,
    },
};


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    SimpleLogger::new()
        .with_module_level("mio", LevelFilter::Warn)
        .with_module_level("tokio_tungstenite", LevelFilter::Warn)
        .with_module_level("tungstenite", LevelFilter::Warn)
        .init()
        .unwrap();
    let app = App::new();
    app.serve(&SocketAddr::from(([0, 0, 0, 0], 8080))).await?;
    Ok(())
}

struct App { }

impl App {
    pub fn new() -> Self {
        App { }
    }

    pub async fn serve(self, addr: &SocketAddr) -> Result<(), hyper::Error> {
        let (tx, rx) = mpsc::unbounded();
        tokio::task::spawn(Self::app_main(rx));
        let server = Server::bind(addr);
        info!("Visit http://{}/index.html to start", &addr);
        server.serve(ConnectionHandler { tx }).await
    }

    async fn app_main(mut rx: mpsc::UnboundedReceiver<AppCmd>) -> () {
        let mut client_txs = Vec::new();
        loop {
            match rx.next().await {
                Some(cmd) => match cmd {
                    AppCmd::NewClient(client_tx) => {
                        info!("new client connected!");
                        client_txs.push(client_tx);
                    },
                    AppCmd::ClientMsg(msg) => match msg {
                        Message::Text(s) => {
                            info!("Server received text {:?}", s);
                            join_all(client_txs.iter_mut().map(|tx| tx.send(s.clone()))).await;
                        }
                        // FIXME: One of the things we receive here is a close, and we should
                        // remove the client_tx when we get that!
                        x => warn!("Server received not text {:?}", x)
                    }
                }
                None => break
            }
        }
    }
}

enum AppCmd {
    NewClient(mpsc::UnboundedSender<String>),
    ClientMsg(Message),
}

struct ConnectionHandler {
    tx: mpsc::UnboundedSender<AppCmd>
}

impl<Conn> Service<Conn> for ConnectionHandler {
    type Response = RequestHandler;
    type Error = Infallible;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: Conn) -> Self::Future {
        ok(RequestHandler { tx: self.tx.clone() })
    }
}

struct RequestHandler { tx: mpsc::UnboundedSender<AppCmd> }

impl Service<Request<Body>> for RequestHandler {
    type Response = Response<Body>;
    type Error = http::Error;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let resp = if req.method() != http::Method::GET {
            err_resp(StatusCode::METHOD_NOT_ALLOWED)
        } else if req.headers().contains_key(header::UPGRADE) {
            // TODO: The URI scheme doesn't seem to get supplied, so we can't use that to switch
            // handler :-(
            handle_ws(&self.tx, req)
        } else {
            handle_get(req)
        };
        ready(resp)
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
    let (mut ws_tx, mut ws_rx) = WebSocketStream::from_raw_socket(upgraded, Role::Server, Default::default())
        .await.split();
    let (client_tx, mut client_rx) = mpsc::unbounded();
    app_tx.send(AppCmd::NewClient(client_tx)).await;
    let mut wss_fut = ws_rx.next();
    let mut app_fut = client_rx.next();
    loop {
        match select(wss_fut, app_fut).await {
            Either::Left((ws_data, pending_app_fut)) => {
                wss_fut = ws_rx.next();
                app_fut = pending_app_fut;
                match ws_data {
                    Some(x) => match x {
                        Ok(msg) => {
                            app_tx.send(AppCmd::ClientMsg(msg)).await;
                        },
                        Err(e) => error!("Server errored! {:?}", e)
                    },
                    None => {
                        warn!("End of stream?");
                        break Ok(())
                    }
                }
            },

            Either::Right((app_data, pending_wss_fut)) => {
                wss_fut = pending_wss_fut;
                app_fut = client_rx.next();
                match app_data {
                    Some(string) =>
                        ws_tx.send(Message::Text(string))
                            .await.expect("how can sending fail?"),
                    None => {
                        warn!("App stopped sending to client?!");
                        break Ok(())
                    }
                }
            }
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
