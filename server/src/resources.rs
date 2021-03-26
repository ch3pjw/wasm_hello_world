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
};

pub fn handle_get(req: Request<Body>) -> Result<Response<Body>, http::Error> {
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
