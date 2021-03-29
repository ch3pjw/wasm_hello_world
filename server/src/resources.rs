use {
    hyper::{
        Body,
        body::Bytes,
        Request,
        Response,
        StatusCode,
        header,
        http,
    },
    log::warn,
    std::str,
};

use {
    crate::hyper_helpers::server_header,
    macros::template,
};

pub fn handle_get(req: Request<Body>) -> Result<Response<Body>, http::Error> {
    let b = Response::builder().header(header::SERVER, server_header());
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

fn generate_client_html(host: &[u8]) -> Bytes {
    Bytes::from(template!(
        "../templates/index.html.template",
        str::from_utf8(host).unwrap()
    ))
}

const CLIENT_JS: Bytes = Bytes::from_static(
    include_bytes!("../../client/static/wasm_hello_world.js")
);
const CLIENT_WASM: Bytes = Bytes::from_static(
    include_bytes!("../../client/static/wasm_hello_world_bg.wasm")
);
