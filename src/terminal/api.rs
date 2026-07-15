use bytes::Bytes;
use dioxus::fullstack::{CborEncoding, Encoding, WebSocketOptions, Websocket};
use dioxus::prelude::*;
use serde::{de::DeserializeOwned, Serialize};
use syntaxis_terminal::{ClientMessage, ServerMessage};

const MAX_TERMINAL_MESSAGE_BYTES: usize = 128 * 1024;

pub(crate) struct TerminalEncoding;

impl Encoding for TerminalEncoding {
    fn content_type() -> &'static str {
        CborEncoding::content_type()
    }

    fn stream_content_type() -> &'static str {
        CborEncoding::stream_content_type()
    }

    fn encode(data: impl Serialize, buffer: &mut Vec<u8>) -> Option<usize> {
        let original_len = buffer.len();
        let Some(encoded) = CborEncoding::encode(data, buffer) else {
            buffer.truncate(original_len);
            return None;
        };
        if encoded > MAX_TERMINAL_MESSAGE_BYTES {
            buffer.truncate(original_len);
            return None;
        }
        Some(encoded)
    }

    fn decode<Output: DeserializeOwned>(bytes: Bytes) -> Option<Output> {
        (bytes.len() <= MAX_TERMINAL_MESSAGE_BYTES)
            .then(|| CborEncoding::decode(bytes))
            .flatten()
    }
}
#[cfg(feature = "server")]
use syntaxis_workspace::WorkspaceId;
#[get("/api/terminal/{workspace_id}")]
pub async fn terminal_socket(
    workspace_id: String,
    options: WebSocketOptions,
) -> Result<Websocket<ClientMessage, ServerMessage, TerminalEncoding>, ServerFnError> {
    server::terminal_socket(WorkspaceId::new(workspace_id), options).await
}
#[cfg(feature = "server")]
pub(crate) mod server;

#[cfg(test)]
mod tests {
    use super::*;
    use syntaxis_terminal::{SessionId, TerminalSize};

    #[test]
    fn terminal_encoding_rejects_oversized_messages_before_deserialization() {
        let oversized = Bytes::from(vec![0; MAX_TERMINAL_MESSAGE_BYTES + 1]);
        assert!(TerminalEncoding::decode::<ClientMessage>(oversized).is_none());
    }

    #[test]
    fn terminal_encoding_keeps_binary_output_compact() {
        let message = ServerMessage::Output {
            session_id: SessionId::new("session"),
            sequence: 1,
            data: vec![42; 32 * 1024],
            replay: false,
        };
        let mut encoded = Vec::new();
        TerminalEncoding::encode(message, &mut encoded).unwrap();
        assert!(encoded.len() < 34 * 1024);

        let resize = ClientMessage::Resize {
            session_id: SessionId::new("session"),
            size: TerminalSize::DEFAULT,
        };
        let mut round_trip = Vec::new();
        TerminalEncoding::encode(&resize, &mut round_trip).unwrap();
        assert_eq!(
            TerminalEncoding::decode::<ClientMessage>(Bytes::from(round_trip)),
            Some(resize)
        );
    }
}
