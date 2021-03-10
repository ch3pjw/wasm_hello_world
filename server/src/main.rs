use {
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
        convert::Infallible,
        net::SocketAddr,
        str,
    },
};

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
    if req.uri().path() != "/" {
        error!("Bad websocket path: {}", req.uri().path());
        return err_resp(StatusCode::NOT_FOUND);
    }
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
    Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, header::HeaderValue::from_static("blah"))
        .body(Body::empty())
}


async fn websocket_dialogue(mut upgraded: hyper::upgrade::Upgraded) -> Result<(), hyper::Error> {
    let mut buf = [0u8; 6];
    loop {
        upgraded.read_exact(&mut buf).await?;
        info!("Server received {:?}", str::from_utf8(&buf));
    }
}
