use {
    crypto::{
        digest::Digest,
        sha1::Sha1,
    },
    hyper::{
        Body,
        Response,
        StatusCode,
        header,
        http,
    },
};

pub fn mk_accept_header(key_header: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.input(key_header);
    // A magic UUID that makes it go (https://en.wikipedia.org/wiki/WebSocket)
    hasher.input(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let mut hashed = vec![0u8; hasher.output_bytes()];
    hasher.result(&mut hashed);
    base64::encode(hashed)
}

pub fn hv(string: &'static str) -> header::HeaderValue {
    header::HeaderValue::from_static(string)
}

pub fn unhv(hv: &header::HeaderValue) -> String {
    match hv.to_str() {
        Err(e) => format!("<{}>", e),
        Ok(s) => s.to_string()
    }
}

pub fn server_header() -> header::HeaderValue {
    header::HeaderValue::from_str(&format!("Concert/{}", common::VERSION)).unwrap()
}

pub fn err_resp(code: StatusCode, message: String) -> Result<Response<Body>, http::Error> {
    Response::builder()
        .status(code)
        .header(header::SERVER, server_header())
        .body(Body::from(message))
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
