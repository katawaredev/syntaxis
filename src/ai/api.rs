use bytes::Bytes;
use dioxus::fullstack::{CborEncoding, Encoding, WebSocketOptions, Websocket};
use dioxus::prelude::*;
use serde::{de::DeserializeOwned, Serialize};
use syntaxis_agent::{ClientMessage, ServerMessage};
use syntaxis_notifications::{NotificationClientMessage, NotificationServerMessage};

const MAX_AGENT_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) struct AgentEncoding;

impl Encoding for AgentEncoding {
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
        if encoded > MAX_AGENT_MESSAGE_BYTES {
            buffer.truncate(original_len);
            return None;
        }
        Some(encoded)
    }

    fn decode<Output: DeserializeOwned>(bytes: Bytes) -> Option<Output> {
        (bytes.len() <= MAX_AGENT_MESSAGE_BYTES)
            .then(|| CborEncoding::decode(bytes))
            .flatten()
    }
}

#[cfg(feature = "server")]
use syntaxis_workspace::WorkspaceId;

#[get("/api/agent/{workspace_id}")]
pub async fn agent_socket(
    workspace_id: String,
    options: WebSocketOptions,
) -> Result<Websocket<ClientMessage, ServerMessage, AgentEncoding>, ServerFnError> {
    server::agent_socket(WorkspaceId::new(workspace_id), options).await
}

#[get("/api/notifications")]
pub async fn notification_socket(
    options: WebSocketOptions,
) -> Result<
    Websocket<NotificationClientMessage, NotificationServerMessage, AgentEncoding>,
    ServerFnError,
> {
    server::notification_socket(options).await
}

#[cfg(feature = "server")]
pub(crate) mod server;

#[cfg(test)]
mod tests {
    use super::*;
    use syntaxis_agent::{AgentSnapshot, ServerMessage};

    #[test]
    fn agent_encoding_round_trips_snapshots() {
        let message = ServerMessage::Snapshot {
            snapshot: AgentSnapshot::default(),
        };
        let mut encoded = Vec::new();
        AgentEncoding::encode(&message, &mut encoded).unwrap();
        assert_eq!(
            AgentEncoding::decode::<ServerMessage>(Bytes::from(encoded)),
            Some(message)
        );
    }
}
