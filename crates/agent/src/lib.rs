//! Shared, Pi-specific chat protocol used between the Syntaxis client and host.
use serde::{Deserialize, Serialize};
pub const PROTOCOL_VERSION: u16 = 8;
pub const MAX_PROMPT_BYTES: usize = 128 * 1024;
pub const MAX_SESSION_NAME_CHARS: usize = 80;
pub const MAX_PROMPT_IMAGES: usize = 5;
pub const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_TOTAL_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
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
pub enum ConversationMatchRole {
    User,
    Assistant,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConversationSearchResult {
    pub session_id: String,
    pub title: String,
    pub updated_at_ms: u64,
    pub role: ConversationMatchRole,
    pub snippet: String,
    pub match_count: usize,
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
        entry_id: Option<String>,
        text: String,
        images: Vec<ImageAttachment>,
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
        args: Option<serde_json::Value>,
        details: Option<serde_json::Value>,
        args_truncated: bool,
        details_truncated: bool,
        status: ItemStatus,
    },
    Custom {
        id: String,
        label: String,
        text: String,
        details: Option<serde_json::Value>,
        details_truncated: bool,
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
            | Self::Custom { id, .. }
            | Self::Notice { id, .. } => id,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelCost {
    /// Catalog rates in micro-USD per million tokens.
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub has_paid_tier: bool,
}
impl ModelCost {
    #[must_use]
    pub const fn is_free(&self) -> bool {
        self.input == 0
            && self.output == 0
            && self.cache_read == 0
            && self.cache_write == 0
            && !self.has_paid_tier
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelSummary {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub reasoning: bool,
    pub supports_images: bool,
    pub context_window: u64,
    pub max_tokens: u64,
    pub cost: ModelCost,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImageAttachment {
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub data: String,
}
impl ImageAttachment {
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime_type, self.data)
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PiCommand {
    pub name: String,
    pub description: String,
    pub source: String,
    pub location: Option<String>,
    pub argument_hint: Option<String>,
    pub invocation: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub total: u64,
}
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionStats {
    pub user_messages: u64,
    pub assistant_messages: u64,
    pub tool_calls: u64,
    pub total_messages: u64,
    pub tokens: TokenUsage,
    pub cost_microusd: u64,
    pub context_tokens: Option<u64>,
    pub context_window: Option<u64>,
    pub context_percent: Option<u8>,
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
    pub steering_queue: Vec<String>,
    pub follow_up_queue: Vec<String>,
    pub fork_points: Vec<(String, String)>,
    pub items: Vec<ChatItem>,
    pub models: Vec<ModelSummary>,
    pub commands: Vec<PiCommand>,
    pub session_stats: Option<SessionStats>,
    pub pending_extension_request: Option<ExtensionUiRequest>,
    pub extension_title: Option<String>,
    pub extension_statuses: Vec<(String, String)>,
    pub extension_widgets: Vec<ExtensionWidget>,
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
            steering_queue: Vec::new(),
            follow_up_queue: Vec::new(),
            fork_points: Vec::new(),
            items: Vec::new(),
            models: Vec::new(),
            commands: Vec::new(),
            session_stats: None,
            pending_extension_request: None,
            extension_title: None,
            extension_statuses: Vec::new(),
            extension_widgets: Vec::new(),
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
pub struct ExtensionWidget {
    pub key: String,
    pub lines: Vec<String>,
    pub placement: String,
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
    DeleteSession {
        session_id: String,
    },
    RenameSession {
        session_id: String,
        name: String,
    },
    SessionAction {
        session_id: String,
        action: Box<ClientMessage>,
    },
    Prompt {
        text: String,
        delivery: PromptDelivery,
        images: Vec<ImageAttachment>,
    },
    ForkMessage {
        entry_id: String,
    },
    Compact {
        custom_instructions: Option<String>,
    },
    CloneSession,
    ExportHtml,
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
            Self::Prompt { text, images, .. }
                if (text.trim().is_empty() && images.is_empty())
                    || text.len() > MAX_PROMPT_BYTES =>
            {
                Err(AgentError::new(
                    AgentErrorCode::InvalidRequest,
                    "Prompt must be between 1 byte and 128 KiB",
                ))
            }
            Self::Prompt { images, .. }
                if images.len() > MAX_PROMPT_IMAGES
                    || images.iter().any(|image| {
                        image.size > MAX_IMAGE_BYTES
                            || !image.mime_type.starts_with("image/")
                            || image.data.is_empty()
                    })
                    || images.iter().map(|image| image.size).sum::<u64>()
                        > MAX_TOTAL_IMAGE_BYTES =>
            {
                Err(AgentError::new(
                    AgentErrorCode::InvalidRequest,
                    "Attach up to 5 images, 8 MiB each and 16 MiB total",
                ))
            }
            Self::ForkMessage { entry_id } if entry_id.trim().is_empty() => Err(AgentError::new(
                AgentErrorCode::InvalidRequest,
                "A Pi message entry id is required",
            )),
            Self::SetModel { provider, model_id }
                if provider.trim().is_empty() || model_id.trim().is_empty() =>
            {
                Err(AgentError::new(
                    AgentErrorCode::InvalidRequest,
                    "A Pi provider and model are required",
                ))
            }
            Self::SelectSession { session_id }
            | Self::DeleteSession { session_id }
            | Self::RenameSession { session_id, .. }
            | Self::SessionAction { session_id, .. }
                if session_id.trim().is_empty() =>
            {
                Err(AgentError::new(
                    AgentErrorCode::InvalidRequest,
                    "A Pi session id is required",
                ))
            }
            Self::RenameSession { name, .. }
                if name.trim().is_empty()
                    || name.trim().chars().count() > MAX_SESSION_NAME_CHARS =>
            {
                Err(AgentError::new(
                    AgentErrorCode::InvalidRequest,
                    "A session name between 1 and 80 characters is required",
                ))
            }
            Self::SessionAction { action, .. }
                if !matches!(
                    action.as_ref(),
                    Self::Prompt { .. }
                        | Self::ForkMessage { .. }
                        | Self::Compact { .. }
                        | Self::CloneSession
                        | Self::ExportHtml
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
            | Self::DeleteSession { .. }
            | Self::RenameSession { .. }
            | Self::Prompt { .. }
            | Self::ForkMessage { .. }
            | Self::Compact { .. }
            | Self::CloneSession
            | Self::ExportHtml
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
            Self::CreateSession
            | Self::SelectSession { .. }
            | Self::DeleteSession { .. }
            | Self::RenameSession { .. }
            | Self::Prompt { .. }
            | Self::ForkMessage { .. }
            | Self::Compact { .. }
            | Self::CloneSession
            | Self::ExportHtml
            | Self::Abort
            | Self::SetModel { .. }
            | Self::SetThinkingLevel { .. }
            | Self::Refresh
            | Self::SessionAction { .. }
            | Self::ExtensionUiResponse { .. }
            | Self::Ping { .. } => Err(AgentError::new(
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
    QueueChanged {
        steering: Vec<String>,
        follow_up: Vec<String>,
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
    Commands {
        commands: Vec<PiCommand>,
    },
    SessionStats {
        stats: SessionStats,
    },
    ExtensionUiRequest {
        request: ExtensionUiRequest,
    },
    ExtensionSurfaces {
        title: Option<String>,
        statuses: Vec<(String, String)>,
        widgets: Vec<ExtensionWidget>,
    },
    ComposerText {
        text: String,
    },
    ExportReady {
        filename: String,
        data_base64: String,
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
            images: Vec::new(),
        };
        let value = serde_json::to_value(&message).expect("client message should serialize");
        assert_eq!(value["type"], "prompt");
        assert_eq!(value["delivery"], "prompt");
        assert_eq!(
            serde_json::from_value::<ClientMessage>(value)
                .expect("client message should deserialize"),
            message
        );
    }
    #[test]
    fn prompt_validation_rejects_empty_and_unbounded_input() {
        let empty = ClientMessage::Prompt {
            text: "  ".into(),
            delivery: PromptDelivery::Prompt,
            images: Vec::new(),
        };
        assert!(empty.validate().is_err());
        let oversized = ClientMessage::Prompt {
            text: "x".repeat(MAX_PROMPT_BYTES + 1),
            delivery: PromptDelivery::Prompt,
            images: Vec::new(),
        };
        assert!(oversized.validate().is_err());
    }
}
