//! Bounded `Content-Length` framing and JSON-RPC envelope helpers.

use std::collections::HashMap;
use std::io;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::lsp::LspError;

pub(crate) const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;

pub(crate) async fn read_message<R>(reader: &mut R) -> Result<Value, LspError>
where
    R: AsyncBufRead + Unpin,
{
    let mut headers = HashMap::new();
    let mut header_bytes = 0usize;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).await.map_err(io_error)?;
        if read == 0 {
            return Err(LspError::Protocol(
                "language server closed its output unexpectedly".to_owned(),
            ));
        }
        header_bytes = header_bytes.saturating_add(read);
        if header_bytes > MAX_HEADER_BYTES {
            return Err(LspError::Protocol(
                "language server response headers exceeded the safety limit".to_owned(),
            ));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let Some((name, value)) = trimmed.split_once(':') else {
            return Err(LspError::Protocol(
                "language server sent a malformed response header".to_owned(),
            ));
        };
        if headers
            .insert(name.trim().to_ascii_lowercase(), value.trim().to_owned())
            .is_some()
        {
            return Err(LspError::Protocol(
                "language server repeated a response header".to_owned(),
            ));
        }
    }
    let length = headers
        .get("content-length")
        .ok_or_else(|| LspError::Protocol("language server omitted Content-Length".to_owned()))?
        .parse::<usize>()
        .map_err(|_| {
            LspError::Protocol("language server sent an invalid Content-Length".to_owned())
        })?;
    if length > MAX_MESSAGE_BYTES {
        return Err(LspError::Protocol(format!(
            "language server message exceeded the {} MiB safety limit",
            MAX_MESSAGE_BYTES / (1024 * 1024)
        )));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await.map_err(|error| {
        LspError::Protocol(format!("language server response ended early: {error}"))
    })?;
    serde_json::from_slice(&body).map_err(|error| {
        LspError::Protocol(format!("language server sent malformed JSON: {error}"))
    })
}

pub(crate) async fn write_message<W>(writer: &mut W, value: &Value) -> Result<(), LspError>
where
    W: AsyncWrite + Unpin,
{
    let body = serde_json::to_vec(value)
        .map_err(|error| LspError::Protocol(format!("could not encode LSP request: {error}")))?;
    if body.len() > MAX_MESSAGE_BYTES {
        return Err(LspError::Protocol(
            "outbound language server message exceeded the safety limit".to_owned(),
        ));
    }
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await
        .map_err(io_error)?;
    writer.write_all(&body).await.map_err(io_error)?;
    writer.flush().await.map_err(io_error)
}

fn io_error(error: io::Error) -> LspError {
    LspError::Process(format!("language server transport failed: {error}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tokio::io::{AsyncWriteExt, BufReader, duplex};

    use super::{MAX_MESSAGE_BYTES, read_message, write_message};

    #[tokio::test]
    async fn framing_handles_partial_transport_reads() {
        let (mut peer, client) = duplex(256);
        tokio::spawn(async move {
            for part in [
                b"Content-Length: 7\r\n".as_slice(),
                b"\r\n{\"x\":1}".as_slice(),
            ] {
                peer.write_all(part)
                    .await
                    .unwrap_or_else(|error| panic!("write failed: {error}"));
            }
        });
        let value = read_message(&mut BufReader::new(client))
            .await
            .unwrap_or_else(|error| panic!("read failed: {error}"));
        assert_eq!(value, json!({"x": 1}));
    }

    #[tokio::test]
    async fn writer_emits_content_length_frame() {
        let (client, mut peer) = duplex(256);
        let write = tokio::spawn(async move {
            let mut client = client;
            write_message(&mut client, &json!({"jsonrpc":"2.0"})).await
        });
        let mut framed = BufReader::new(&mut peer);
        assert_eq!(
            read_message(&mut framed)
                .await
                .unwrap_or_else(|error| panic!("read failed: {error}")),
            json!({"jsonrpc":"2.0"})
        );
        write
            .await
            .unwrap_or_else(|error| panic!("join failed: {error}"))
            .unwrap_or_else(|error| panic!("write failed: {error}"));
    }

    #[tokio::test]
    async fn rejects_malformed_oversized_and_truncated_frames() {
        for source in [
            b"Broken\r\n\r\n{}".to_vec(),
            format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1).into_bytes(),
            b"Content-Length: 8\r\n\r\n{}".to_vec(),
            b"Content-Length: 1\r\n\r\n{".to_vec(),
        ] {
            let mut reader = BufReader::new(source.as_slice());
            assert!(read_message(&mut reader).await.is_err());
        }
    }
}
