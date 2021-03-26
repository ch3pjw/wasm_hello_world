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

pub fn err_resp(code: StatusCode) -> Result<Response<Body>, http::Error> {
    Response::builder().status(code).body(Body::empty())
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
