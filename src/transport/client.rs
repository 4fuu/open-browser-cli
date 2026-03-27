use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::protocol::messages::{Request, Response};

const RELAY_ADDR: &str = "127.0.0.1:12899";

/// Connect to the relay, send a request, and return the response.
pub async fn send_request(req: &Request) -> Result<Response> {
    send_request_to(req, RELAY_ADDR).await
}

async fn send_request_to(req: &Request, addr: &str) -> Result<Response> {
    let mut stream = TcpStream::connect(addr).await.with_context(|| {
        format!("Failed to connect to relay at {addr}. Is the browser extension running?")
    })?;

    let mut json = serde_json::to_string(req)?;
    json.push('\n');
    stream.write_all(json.as_bytes()).await?;

    let mut reader = BufReader::new(&mut stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: Response = serde_json::from_str(line.trim_end())?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn connection_error_when_no_relay() {
        let req = Request {
            id: "test-1".into(),
            action: "ping".into(),
            params: json!({}),
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let unused_addr = listener.local_addr().unwrap();
        drop(listener);

        let result = send_request_to(&req, &unused_addr.to_string()).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Failed to connect to relay"),
            "unexpected error: {err_msg}"
        );
    }

    #[test]
    fn request_response_serde_round_trip() {
        let req = Request {
            id: "req-42".into(),
            action: "list_tabs".into(),
            params: json!({"windowId": 1}),
        };

        let serialized = serde_json::to_string(&req).unwrap();
        let deserialized: Request = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.id, "req-42");
        assert_eq!(deserialized.action, "list_tabs");
        assert_eq!(deserialized.params, json!({"windowId": 1}));

        let resp = Response {
            id: "req-42".into(),
            ok: true,
            data: Some(json!({"tabs": []})),
            error: None,
        };

        let serialized = serde_json::to_string(&resp).unwrap();
        let deserialized: Response = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.id, "req-42");
        assert!(deserialized.ok);
        assert_eq!(deserialized.data, Some(json!({"tabs": []})));
        assert!(deserialized.error.is_none());

        // Round-trip a response with error and no data
        let err_resp = Response {
            id: "req-99".into(),
            ok: false,
            data: None,
            error: Some("not found".into()),
        };

        let serialized = serde_json::to_string(&err_resp).unwrap();
        let deserialized: Response = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.id, "req-99");
        assert!(!deserialized.ok);
        assert!(deserialized.data.is_none());
        assert_eq!(deserialized.error.as_deref(), Some("not found"));
    }
}
