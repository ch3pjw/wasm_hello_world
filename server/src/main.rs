use {
    crypto::{
        digest::Digest,
        sha1::Sha1,
    },
    futures::{
        Future, StreamExt, SinkExt,
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
        pin::Pin,
        task::{Context, Poll},
    },
    tokio_tungstenite::{
        tungstenite::protocol::{Role, Message},
        WebSocketStream,
    },
};


struct App { tx: mpsc::UnboundedSender<()> }

impl<Conn> Service<Conn> for App {
    type Response = RequestHandler;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: Conn) -> Self::Future {
        let tx = self.tx.clone();
        Box::pin(async move { Ok( RequestHandler { tx } ) })
    }
}

struct RequestHandler { tx: mpsc::UnboundedSender<()> }

impl Service<Request<Body>> for RequestHandler {
    type Response = Response<Body>;
    type Error = http::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // Oh dear goodness why do I have to clone so much crap?!
        let tx = Box::new(self.tx.clone());
        Box::pin(handle_request(tx, req))
    }
}


#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_module_level("mio", LevelFilter::Warn)
        .with_module_level("tokio_tungstenite", LevelFilter::Warn)
        .with_module_level("tungstenite", LevelFilter::Warn)
        .init()
        .unwrap();
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    let (tx, mut rx) = mpsc::unbounded::<()>();

    tokio::task::spawn(async move {
        loop {
            match rx.next().await {
                Some(()) => warn!("I received something!"),
                None => break
            }
        }
    });
    let server = Server::bind(&addr).serve(App { tx });
    info!("Visit http://0.0.0.0:8080/index.html to start");
    server.await;
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

async fn handle_request(mut tx: Box<mpsc::UnboundedSender<()>>, req: Request<Body>) -> Result<Response<Body>, http::Error> {
    if req.method() != http::Method::GET {
        return err_resp(StatusCode::METHOD_NOT_ALLOWED);
    }
    // TODO: The URI scheme doesn't seem to get supplied, so we can't use that to switch handler
    // :-(
    tx.send(()).await;
    if req.headers().contains_key(header::UPGRADE) {
        handle_ws(req)
    } else {
        handle_get(req)
    }
}

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

fn handle_ws(mut req: Request<Body>) -> Result<Response<Body>, http::Error> {
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

    tokio::task::spawn(async move {
        match hyper::upgrade::on(&mut req).await {
            Ok(upgraded) => {
                if let Err(e) = websocket_dialogue(upgraded).await {
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


async fn websocket_dialogue(upgraded: hyper::upgrade::Upgraded) -> Result<(), hyper::Error> {
    let mut wss = WebSocketStream::from_raw_socket(upgraded, Role::Server, Default::default()).await;
    loop {
        match wss.next().await {
            Some(x) => match x {
                Ok(msg) => match msg {
                    Message::Text(s) => {
                        info!("Server received text {:?}", s);
                        wss.send(Message::Text(s)).await.expect("how can sending fail?");
                    }
                    x => warn!("Server received not text {:?}", x)
                },
                Err(e) => error!("Server errored! {:?}", e)
            },
            None => {
                warn!("End of stream?");
                break Ok(())
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
