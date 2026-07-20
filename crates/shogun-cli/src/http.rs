//! A tiny **loopback-only** HTTP/1.1 client for talking to the local daemon (`127.0.0.1:<port>`).
//!
//! It is a raw `TcpStream` on purpose: the FR-TR-03 egress guard forbids any HTTP-client crate
//! outside shogun-core, and this is not an external egress — it's loopback IPC to the user's own
//! daemon. Hand-rolling a `Connection: close` request keeps the CLI dependency-free and the guard
//! green. Response parsing ([`parse_response`]) is pure and unit-tested.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// A parsed HTTP response: status code + body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Send one request to `127.0.0.1:<port>` and read the whole response (`Connection: close`).
pub fn request(
    port: u16,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&str>,
) -> std::io::Result<HttpResponse> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    let body = body.unwrap_or("");
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes())?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    Ok(parse_response(&String::from_utf8_lossy(&raw)))
}

/// Parse a raw HTTP/1.1 response into status + body. Pure.
pub fn parse_response(raw: &str) -> HttpResponse {
    let status = raw
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    // Body is everything after the header/body separator.
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    HttpResponse { status, body }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn parse_response_extracts_status_and_body() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}";
        let r = parse_response(raw);
        assert_eq!(r.status, 200);
        assert_eq!(r.body, "{\"ok\":true}");
    }

    #[test]
    fn parse_response_handles_401_and_empty_body() {
        let r = parse_response("HTTP/1.1 401 Unauthorized\r\n\r\n");
        assert_eq!(r.status, 401);
        assert_eq!(r.body, "");
        // a malformed line → status 0
        assert_eq!(parse_response("garbage").status, 0);
    }

    #[test]
    fn request_speaks_to_a_loopback_server_and_sends_headers() {
        // a one-shot mock server on an ephemeral port
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let n = sock.read(&mut buf).unwrap();
            let received = String::from_utf8_lossy(&buf[..n]).into_owned();
            sock.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\n\r\n{\"id\":9}")
                .unwrap();
            received
        });

        let resp = request(port, "POST", "/v1/memory/notes", Some("tok"), Some("hello")).unwrap();
        assert_eq!(resp.status, 202);
        assert_eq!(resp.body, "{\"id\":9}");

        let received = handle.join().unwrap();
        assert!(received.starts_with("POST /v1/memory/notes HTTP/1.1"));
        assert!(received.contains("Authorization: Bearer tok"));
        assert!(received.contains("Content-Length: 5"));
        assert!(received.ends_with("hello"));
    }
}
