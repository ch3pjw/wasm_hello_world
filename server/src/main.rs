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
    std::{
        convert::Infallible,
        net::SocketAddr,
    }
};

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    let mk_svc = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(handle_request))
    });

    let server = Server::bind(&addr).serve(mk_svc);
    server.await;
}


const CLIENT_HTML: &[u8]  = include_bytes!("../../client/static/index.html");
const CLIENT_JS: &[u8]  = include_bytes!("../../client/static/wasm_hello_world.js");
const CLIENT_WASM: &[u8] = include_bytes!("../../client/static/wasm_hello_world_bg.wasm");

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, http::Error> {
    let b = Response::builder();
    let x = match req.uri().path() {
        "/" | "/index.html" => Some(("text/html", CLIENT_HTML)),
        "/wasm_hello_world.js" => Some(("application/javascript", CLIENT_JS)),
        "/wasm_hello_world_bg.wasm" => Some(("application/wasm", CLIENT_WASM)),
        _ => None
    };
    match x {
        None => b.status(StatusCode::NOT_FOUND).body(Body::empty()),
        Some((content_type, bytes)) => b
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(bytes)),
    }
}