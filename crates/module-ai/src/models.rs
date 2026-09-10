use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiMessage {
    pub id: String,
    pub role: AiRole,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiConversationSummary {
    pub id: String,
    pub title: String,
    pub message_count: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiConversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<AiMessage>,
    pub selected_model_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AiPrompt {
    pub text: String,
    pub active_file_reference: Option<String>,
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
    pub monthly_downloads: u64,
    pub kinds: Vec<String>,
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
    UsageUpdated {
        input_tokens: u64,
        output_tokens: u64,
    },
    Failed {
        message: String,
    },
}
