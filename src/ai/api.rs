use bytes::Bytes;
use dioxus::fullstack::{CborEncoding, Encoding, WebSocketOptions, Websocket};
use dioxus::prelude::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use syntaxis_agent::{ClientMessage, ConversationSearchResult, ServerMessage};
use syntaxis_notifications::{NotificationClientMessage, NotificationServerMessage};

const MAX_AGENT_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct PiPackageSummary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub publisher: String,
    pub published_at: String,
    pub monthly_downloads: u64,
    pub kinds: Vec<String>,
    pub installed_scopes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct PiPackageSearch {
    pub packages: Vec<PiPackageSummary>,
    pub catalog_total: usize,
    pub start_offset: usize,
    pub next_offset: usize,
    pub has_more: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PiPackageAction {
    Install,
    Uninstall,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct PiOperationResult {
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct PiSettingsSnapshot {
    pub pi_version: String,
    pub schema_version: String,
    pub compatible: bool,
    pub compatibility_message: Option<String>,
    pub values: serde_json::Value,
}

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

#[post("/api/pi/packages/search")]
pub(crate) async fn pi_packages(
    workspace_id: String,
    query: String,
    offset: usize,
) -> Result<PiPackageSearch, ServerFnError> {
    server::pi_packages(WorkspaceId::new(workspace_id), query, offset).await
}

#[post("/api/pi/sessions/search")]
pub(crate) async fn search_conversations(
    workspace_id: String,
    query: String,
) -> Result<Vec<ConversationSearchResult>, ServerFnError> {
    server::search_conversations(WorkspaceId::new(workspace_id), query).await
}

#[post("/api/pi/packages/manage")]
pub(crate) async fn manage_pi_package(
    workspace_id: String,
    name: String,
    action: PiPackageAction,
) -> Result<PiOperationResult, ServerFnError> {
    server::manage_pi_package(WorkspaceId::new(workspace_id), name, action).await
}

#[post("/api/pi/settings")]
pub(crate) async fn pi_settings(workspace_id: String) -> Result<PiSettingsSnapshot, ServerFnError> {
    server::pi_settings(WorkspaceId::new(workspace_id)).await
}

#[post("/api/pi/settings/update")]
pub(crate) async fn update_pi_setting(
    workspace_id: String,
    path: String,
    value: serde_json::Value,
) -> Result<PiSettingsSnapshot, ServerFnError> {
    server::update_pi_setting(WorkspaceId::new(workspace_id), path, value).await
}

#[post("/api/pi/update")]
pub(crate) async fn update_pi(workspace_id: String) -> Result<PiOperationResult, ServerFnError> {
    server::update_pi(WorkspaceId::new(workspace_id)).await
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
