use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiRole {
    #[default]
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiMessage {
    pub id: String,
    pub role: AiRole,
    pub content: String,
    #[serde(default)]
    pub entry_id: Option<String>,
    #[serde(default)]
    pub images: Vec<AiImageAttachment>,
    #[serde(default)]
    pub thinking: String,
    #[serde(default)]
    pub status: AiMessageStatus,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMessageStatus {
    Streaming,
    Running,
    #[default]
    Complete,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiConversationSummary {
    pub id: String,
    pub title: String,
    pub message_count: usize,
    #[serde(default)]
    pub updated_at_ms: u64,
    #[serde(default)]
    pub status_message: String,
    #[serde(default)]
    pub running: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiConversationMatch {
    pub session_id: String,
    pub title: String,
    pub updated_at_ms: u64,
    pub role: AiRole,
    pub snippet: String,
    pub match_count: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiConversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<AiMessage>,
    pub activity: Vec<AiActivity>,
    pub item_order: Vec<String>,
    pub selected_model_id: Option<String>,
    pub thinking_level: AiThinkingLevel,
    pub usage: Option<AiUsage>,
    pub running: bool,
    pub status_message: String,
    pub pending_messages: usize,
    pub steering_queue: Vec<String>,
    pub follow_up_queue: Vec<String>,
    pub commands: Vec<AiCommand>,
    pub supports_queued_prompts: bool,
    pub extension_request: Option<AiExtensionRequest>,
    pub extension_title: Option<String>,
    pub extension_statuses: Vec<(String, String)>,
    pub extension_widgets: Vec<AiExtensionWidget>,
    pub requested_composer_text: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiCommand {
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
    pub interactive: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiExtensionRequest {
    pub id: String,
    pub method: String,
    pub title: String,
    pub message: String,
    pub options: Vec<String>,
    pub placeholder: Option<String>,
    pub prefill: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiExtensionWidget {
    pub key: String,
    pub lines: Vec<String>,
    pub placement: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiClientEvent {
    pub kind: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub data: Option<String>,
    pub text: Option<String>,
    pub message: Option<String>,
    pub available: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiActivity {
    Tool {
        id: String,
        name: String,
        summary: String,
        output: String,
        args: Option<String>,
        details: Option<String>,
        #[serde(default)]
        args_truncated: bool,
        #[serde(default)]
        details_truncated: bool,
        status: AiMessageStatus,
    },
    Custom {
        id: String,
        label: String,
        text: String,
        details: Option<String>,
        #[serde(default)]
        details_truncated: bool,
    },
    Notice {
        id: String,
        text: String,
        status: AiMessageStatus,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiUsage {
    pub total_tokens: u64,
    pub context_tokens: Option<u64>,
    pub context_window: Option<u64>,
    pub context_percent: Option<u8>,
    pub cost_microusd: u64,
    pub total_messages: u64,
    pub tool_calls: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiPrompt {
    pub text: String,
    pub active_file_reference: Option<String>,
    #[serde(default)]
    pub images: Vec<AiImageAttachment>,
    #[serde(default)]
    pub delivery: AiPromptDelivery,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiPromptDelivery {
    #[default]
    Prompt,
    Steer,
    FollowUp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiModel {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub reasoning: bool,
    pub thinking_levels: Vec<AiThinkingLevel>,
    pub supports_images: bool,
    #[serde(default)]
    pub context_window: u64,
    #[serde(default)]
    pub max_tokens: u64,
    #[serde(default)]
    pub cost: AiModelCost,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiModelCost {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub has_paid_tier: bool,
}

impl AiModelCost {
    #[must_use]
    pub const fn is_free(&self) -> bool {
        self.input == 0
            && self.output == 0
            && self.cache_read == 0
            && self.cache_write == 0
            && !self.has_paid_tier
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiModelPreferences {
    pub favourites: Vec<String>,
    pub efforts: BTreeMap<String, AiThinkingLevel>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiThinkingLevel {
    Off,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
    Max,
}

impl AiThinkingLevel {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Minimal => "Minimal",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Xhigh => "Extra high",
            Self::Max => "Maximum",
        }
    }

    #[must_use]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiImageAttachment {
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub data: String,
}

impl AiImageAttachment {
    #[must_use]
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime_type, self.data)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiProviderSettings {
    pub endpoint: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub credential: String,
    pub credential_is_set: bool,
    pub volatile_credential: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiGeneralSettingKind {
    Toggle,
    Select,
    Number,
    Text,
    StringArray,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiGeneralSetting {
    pub section: String,
    pub path: String,
    pub label: String,
    pub description: String,
    pub kind: AiGeneralSettingKind,
    pub options: Vec<String>,
    pub value: String,
    pub available: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiAdvancedSettings {
    pub scope: AiResourceScope,
    pub path: String,
    pub content: String,
    pub revision: String,
    pub documentation: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiManagedFeature {
    Usage,
    ProviderAuth,
    Resources,
    Extensions,
    Worktrees,
    Notifications,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiFeatureSummary {
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderAuthKind {
    ApiKey,
    Oauth,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiProviderAuthMethod {
    pub kind: AiProviderAuthKind,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiProviderAccount {
    pub id: String,
    pub name: String,
    pub configured: bool,
    pub can_logout: bool,
    pub status: String,
    pub methods: Vec<AiProviderAuthMethod>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiAuthPromptOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiAuthPrompt {
    pub id: u64,
    pub kind: String,
    pub message: String,
    pub placeholder: String,
    pub options: Vec<AiAuthPromptOption>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiAuthEvent {
    pub kind: String,
    pub message: String,
    pub url: String,
    pub user_code: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiAuthFlow {
    pub id: String,
    pub provider_id: String,
    pub prompt: Option<AiAuthPrompt>,
    pub events: Vec<AiAuthEvent>,
    pub complete: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiResourceScope {
    Global,
    Project,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiPromptTemplate {
    pub name: String,
    pub description: String,
    pub argument_hint: String,
    pub content: String,
    pub scope: AiResourceScope,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiSkill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub scope: AiResourceScope,
    pub storage_name: String,
    pub single_file: bool,
    pub extra_frontmatter: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiSkillCatalogView {
    #[default]
    AllTime,
    Trending,
    Hot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiSkillSearchResult {
    pub name: String,
    pub slug: String,
    pub source: String,
    pub installs: u64,
    pub page_url: String,
    pub installable: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiSkillSearchPage {
    pub skills: Vec<AiSkillSearchResult>,
    pub start_offset: usize,
    pub next_offset: usize,
    pub has_more: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiExtensionAction {
    Install,
    Uninstall,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiExtension {
    pub name: String,
    pub version: String,
    pub description: String,
    pub publisher: String,
    #[serde(default)]
    pub published_at: String,
    pub monthly_downloads: u64,
    pub kinds: Vec<String>,
    #[serde(default)]
    pub installed_scopes: Vec<String>,
    pub installed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiExtensionPage {
    pub items: Vec<AiExtension>,
    pub total: usize,
    pub next_offset: usize,
    pub has_more: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiEvent {
    UserMessage(AiMessage),
    AssistantDelta {
        message_id: String,
        text: String,
    },
    AssistantThinkingDelta {
        message_id: String,
        text: String,
    },
    AssistantCompleted(AiMessage),
    ToolStarted {
        id: String,
        name: String,
    },
    ToolUpdated {
        id: String,
        output: String,
    },
    ToolCompleted {
        id: String,
        output: String,
    },
    ActivityUpdated(AiActivity),
    UsageUpdated {
        input_tokens: u64,
        output_tokens: u64,
    },
    StatusChanged {
        running: bool,
        message: String,
        pending_messages: usize,
    },
    QueueChanged {
        steering: Vec<String>,
        follow_up: Vec<String>,
    },
    ExtensionRequested(AiExtensionRequest),
    ExtensionSurfaces {
        title: Option<String>,
        statuses: Vec<(String, String)>,
        widgets: Vec<AiExtensionWidget>,
    },
    ComposerText(String),
    Failed {
        message: String,
    },
}
