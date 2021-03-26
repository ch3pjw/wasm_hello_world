use {
    hyper::body::Bytes
};

const CLIENT_HTML_TEMPLATE: &[u8] = include_bytes!("../templates/index.html.template");

pub fn generate_client_html(host: &[u8]) -> Bytes {
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

pub const CLIENT_JS: Bytes = Bytes::from_static(
    include_bytes!("../../client/static/wasm_hello_world.js")
);
pub const CLIENT_WASM: Bytes = Bytes::from_static(
    include_bytes!("../../client/static/wasm_hello_world_bg.wasm")
);
