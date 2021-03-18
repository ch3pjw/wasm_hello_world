use {
    crypto::{
        digest::Digest,
        sha1::Sha1,
    },
    hyper::{
        Body,
        Request,
        Response,
        Server,
        StatusCode,
        header,
        http,
        service::{make_service_fn, service_fn},
    },
    log::{info, error, warn},
    simple_logger::SimpleLogger,
    std::{
        str,
        convert::Infallible,
        net::SocketAddr,
    },
    tokio::io::{AsyncReadExt, AsyncWriteExt}
};

mod websocket;

#[tokio::main]
async fn main() {
    SimpleLogger::new().init().unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    let mk_svc = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(handle_request))
    });

    let server = Server::bind(&addr).serve(mk_svc);
    info!("Visit http://127.0.0.1:8080/index.html to start");
    server.await;
}


const CLIENT_HTML: &[u8] = include_bytes!("../../client/static/index.html");
const CLIENT_JS: &[u8] = include_bytes!("../../client/static/wasm_hello_world.js");
const CLIENT_WASM: &[u8] = include_bytes!("../../client/static/wasm_hello_world_bg.wasm");

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, http::Error> {
    if req.method() != http::Method::GET {
        return err_resp(StatusCode::METHOD_NOT_ALLOWED);
    }
    // TODO: The URI scheme doesn't seem to get supplied, so we can't use that to switch handler
    // :-(
    if req.headers().contains_key(header::UPGRADE) {
        handle_ws(req)
    } else {
        handle_get(req)
    }
}

fn handle_get(req: Request<Body>) -> Result<Response<Body>, http::Error> {
    let b = Response::builder();
    let x = match req.uri().path() {
        "/" | "/index.html" => Some(("text/html", CLIENT_HTML)),
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
    headers.insert("Sec-WebSocket-Protocol", hv("foo"));
    return Ok(resp);
}

fn hv(string: &'static str) -> header::HeaderValue {
    header::HeaderValue::from_static(string)
}


async fn websocket_dialogue(mut upgraded: hyper::upgrade::Upgraded) -> Result<(), hyper::Error> {
    let mut buf = [0u8; 1024];
    loop {
        let bytes_read = match upgraded.read(&mut buf).await {
            Ok(bytes_read) => {
                if bytes_read == 0 /* EOF */ { return Ok(()) };
                bytes_read
            },
            Err(e) => {
                error!("Websocket error {:?}", e);
                todo!("translate e into hyper::Error or change our error type everywhere")
            }
        };
        let (_, frame) = websocket::Frame::parse(&buf[0..bytes_read])
            .expect("failed to parse data frame");
        info!("Server received {:?}", str::from_utf8(&frame.payload));
        upgraded.write(&frame.serialise());
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
