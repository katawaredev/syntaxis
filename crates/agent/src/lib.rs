//! Shared, Pi-specific chat protocol used between the Syntaxis client and host.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 2;
pub const MAX_PROMPT_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptDelivery {
    #[default]
    Prompt,
    Steer,
    FollowUp,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
    Max,
}

impl ThinkingLevel {
    pub const ALL: [Self; 7] = [
        Self::Off,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Xhigh,
        Self::Max,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Starting,
    Ready,
    Working,
    Compacting,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentSessionSummary {
    pub id: String,
    pub title: String,
    pub updated_at_ms: u64,
    pub status: AgentStatus,
    pub status_message: String,
    pub running: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Streaming,
    Running,
    Complete,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatItem {
    User {
        id: String,
        text: String,
    },
    Assistant {
        id: String,
        text: String,
        thinking: String,
        status: ItemStatus,
    },
    Tool {
        id: String,
        name: String,
        summary: String,
        output: String,
        status: ItemStatus,
    },
    Notice {
        id: String,
        text: String,
        status: ItemStatus,
    },
}

impl ChatItem {
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. }
            | Self::Assistant { id, .. }
            | Self::Tool { id, .. }
            | Self::Notice { id, .. } => id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelSummary {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub reasoning: bool,
}

impl ModelSummary {
    pub fn key(&self) -> String {
        format!("{}\u{1f}{}", self.provider, self.id)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentSnapshot {
    pub status: AgentStatus,
    pub status_message: String,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub model: Option<ModelSummary>,
    pub thinking_level: ThinkingLevel,
    pub pending_messages: usize,
    pub items: Vec<ChatItem>,
    pub models: Vec<ModelSummary>,
    pub pending_extension_request: Option<ExtensionUiRequest>,
}

impl Default for AgentSnapshot {
    fn default() -> Self {
        Self {
            status: AgentStatus::Starting,
            status_message: "Starting Pi…".into(),
            session_id: None,
            session_name: None,
            model: None,
            thinking_level: ThinkingLevel::Medium,
            pending_messages: 0,
            items: Vec::new(),
            models: Vec::new(),
            pending_extension_request: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExtensionUiRequest {
    pub id: String,
    pub method: String,
    pub title: String,
    pub message: String,
    pub options: Vec<String>,
    pub placeholder: Option<String>,
    pub prefill: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        version: u16,
    },
    CreateSession,
    SelectSession {
        session_id: String,
    },
    SessionAction {
        session_id: String,
        action: Box<ClientMessage>,
    },
    Prompt {
        text: String,
        delivery: PromptDelivery,
    },
    Abort,
    SetModel {
        provider: String,
        model_id: String,
    },
    SetThinkingLevel {
        level: ThinkingLevel,
    },
    Refresh,
    ExtensionUiResponse {
        request_id: String,
        value: Option<String>,
        confirmed: Option<bool>,
        cancelled: bool,
    },
    Ping {
        nonce: u64,
    },
}

impl ClientMessage {
    /// Validate allocation bounds and required fields on an incoming message.
    ///
    /// # Errors
    ///
    /// Returns a stable protocol error when the message is invalid.
    pub fn validate(&self) -> Result<(), AgentError> {
        match self {
            Self::Hello { version } if *version != PROTOCOL_VERSION => Err(AgentError::new(
                AgentErrorCode::InvalidProtocol,
                "Unsupported AI protocol version",
            )),
            Self::Prompt { text, .. }
                if text.trim().is_empty() || text.len() > MAX_PROMPT_BYTES =>
            {
                Err(AgentError::new(
                    AgentErrorCode::InvalidRequest,
                    "Prompt must be between 1 byte and 128 KiB",
                ))
            }
            Self::SetModel { provider, model_id }
                if provider.trim().is_empty() || model_id.trim().is_empty() =>
            {
                Err(AgentError::new(
                    AgentErrorCode::InvalidRequest,
                    "A Pi provider and model are required",
                ))
            }
            Self::SelectSession { session_id } | Self::SessionAction { session_id, .. }
                if session_id.trim().is_empty() =>
            {
                Err(AgentError::new(
                    AgentErrorCode::InvalidRequest,
                    "A Pi session id is required",
                ))
            }
            Self::SessionAction { action, .. }
                if !matches!(
                    action.as_ref(),
                    Self::Prompt { .. }
                        | Self::Abort
                        | Self::SetModel { .. }
                        | Self::SetThinkingLevel { .. }
                        | Self::Refresh
                        | Self::ExtensionUiResponse { .. }
                ) =>
            {
                Err(AgentError::new(
                    AgentErrorCode::InvalidRequest,
                    "Unsupported Pi session action",
                ))
            }
            Self::SessionAction { action, .. } => action.validate(),
            Self::Hello { .. }
            | Self::CreateSession
            | Self::SelectSession { .. }
            | Self::Prompt { .. }
            | Self::Abort
            | Self::SetModel { .. }
            | Self::SetThinkingLevel { .. }
            | Self::Refresh
            | Self::ExtensionUiResponse { .. }
            | Self::Ping { .. } => Ok(()),
        }
    }

    /// Validate that this is the first handshake message for the protocol.
    ///
    /// # Errors
    ///
    /// Returns a protocol error when the message is not a compatible hello.
    pub fn validate_handshake(&self) -> Result<(), AgentError> {
        match self {
            Self::Hello { .. } => self.validate(),
            _ => Err(AgentError::new(
                AgentErrorCode::InvalidProtocol,
                "AI protocol handshake required",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello {
        version: u16,
    },
    Sessions {
        sessions: Vec<AgentSessionSummary>,
    },
    SelectedSession {
        session_id: String,
        snapshot: AgentSnapshot,
    },
    SessionEvent {
        session_id: String,
        event: Box<ServerMessage>,
    },
    Snapshot {
        snapshot: AgentSnapshot,
    },
    ItemAdded {
        item: ChatItem,
    },
    ItemDelta {
        item_id: String,
        text: String,
        thinking: bool,
    },
    ItemUpdated {
        item: ChatItem,
    },
    Status {
        status: AgentStatus,
        message: String,
        pending_messages: usize,
    },
    SessionChanged {
        session_id: Option<String>,
        session_name: Option<String>,
    },
    ModelChanged {
        model: Option<ModelSummary>,
        thinking_level: ThinkingLevel,
    },
    Models {
        models: Vec<ModelSummary>,
    },
    ExtensionUiRequest {
        request: ExtensionUiRequest,
    },
    ComposerText {
        text: String,
    },
    Error {
        error: AgentError,
    },
    Pong {
        nonce: u64,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorCode {
    InvalidProtocol,
    InvalidRequest,
    Unavailable,
    ProcessExited,
    Internal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentError {
    pub code: AgentErrorCode,
    pub message: String,
}

impl AgentError {
    pub fn new(code: AgentErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_messages_are_stably_tagged() {
        let message = ClientMessage::Prompt {
            text: "Inspect this project".into(),
            delivery: PromptDelivery::Prompt,
        };
        let value = serde_json::to_value(&message).unwrap();
        assert_eq!(value["type"], "prompt");
        assert_eq!(value["delivery"], "prompt");
        assert_eq!(
            serde_json::from_value::<ClientMessage>(value).unwrap(),
            message
        );
    }

    #[test]
    fn prompt_validation_rejects_empty_and_unbounded_input() {
        let empty = ClientMessage::Prompt {
            text: "  ".into(),
            delivery: PromptDelivery::Prompt,
        };
        assert!(empty.validate().is_err());
        let oversized = ClientMessage::Prompt {
            text: "x".repeat(MAX_PROMPT_BYTES + 1),
            delivery: PromptDelivery::Prompt,
        };
        assert!(oversized.validate().is_err());
    }
}
