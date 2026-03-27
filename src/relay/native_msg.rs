use anyhow::{Result, bail};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Read a native messaging frame: 4-byte little-endian length prefix followed by JSON.
pub async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<serde_json::Value> {
    let mut len_buf = [0u8; 4];
    let n = reader.read(&mut len_buf).await?;
    if n == 0 {
        bail!("EOF: no data to read");
    }
    // Ensure we read all 4 bytes
    if n < 4 {
        reader.read_exact(&mut len_buf[n..]).await?;
    }
    let len = u32::from_le_bytes(len_buf) as usize;

    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;

    let value = serde_json::from_slice(&body)?;
    Ok(value)
}

/// Write a native messaging frame: 4-byte little-endian length prefix followed by JSON.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &serde_json::Value,
) -> Result<()> {
    let body = serde_json::to_vec(msg)?;
    let len = body.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_round_trip() {
        let msg = serde_json::json!({"action": "open", "url": "https://example.com"});
        let mut buf = Vec::new();
        write_message(&mut buf, &msg).await.unwrap();

        let mut reader = Cursor::new(buf);
        let result = read_message(&mut reader).await.unwrap();
        assert_eq!(result, msg);
    }

    #[tokio::test]
    async fn test_empty_json_object() {
        let msg = serde_json::json!({});
        let mut buf = Vec::new();
        write_message(&mut buf, &msg).await.unwrap();

        let mut reader = Cursor::new(buf);
        let result = read_message(&mut reader).await.unwrap();
        assert_eq!(result, msg);
    }

    #[tokio::test]
    async fn test_nested_json() {
        let msg = serde_json::json!({
            "id": "abc-123",
            "data": {
                "tabs": [
                    {"id": 1, "title": "Tab One"},
                    {"id": 2, "title": "Tab Two"}
                ],
                "meta": {"count": 2, "active": true}
            }
        });
        let mut buf = Vec::new();
        write_message(&mut buf, &msg).await.unwrap();

        let mut reader = Cursor::new(buf);
        let result = read_message(&mut reader).await.unwrap();
        assert_eq!(result, msg);
    }

    #[tokio::test]
    async fn test_eof_error() {
        let mut reader = Cursor::new(Vec::<u8>::new());
        let result = read_message(&mut reader).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("EOF"),
            "error should mention EOF"
        );
    }

    #[tokio::test]
    async fn test_large_message() {
        let large_text = "x".repeat(100_000);
        let msg = serde_json::json!({"payload": large_text});
        let mut buf = Vec::new();
        write_message(&mut buf, &msg).await.unwrap();

        // Verify length prefix is correct
        let expected_body = serde_json::to_vec(&msg).unwrap();
        let stored_len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        assert_eq!(stored_len, expected_body.len());

        let mut reader = Cursor::new(buf);
        let result = read_message(&mut reader).await.unwrap();
        assert_eq!(result, msg);
    }
}
