//! Canonical runtime-neutral AI views and capability ports.

mod models;
mod ports;
mod view;

pub use models::{
    AiActivity, AiAdvancedSettings, AiAuthEvent, AiAuthFlow, AiAuthPrompt, AiAuthPromptOption,
    AiClientEvent, AiCommand, AiConversation, AiConversationMatch, AiConversationSummary, AiEvent,
    AiExtension, AiExtensionAction, AiExtensionPage, AiExtensionRequest, AiExtensionWidget,
    AiFeatureSummary, AiGeneralSetting, AiGeneralSettingKind, AiImageAttachment, AiManagedFeature,
    AiMessage, AiMessageStatus, AiModel, AiModelCost, AiModelPreferences, AiPrompt,
    AiPromptDelivery, AiPromptTemplate, AiProviderAccount, AiProviderAuthKind,
    AiProviderAuthMethod, AiProviderSettings, AiResourceScope, AiRole, AiSkill, AiSkillCatalogView,
    AiSkillSearchPage, AiSkillSearchResult, AiThinkingLevel, AiUsage,
};
pub use ports::{
    AiClientEventStream, AiClientPort, AiConversationPort, AiEventStream, AiExtensionsPort,
    AiGeneralSettingsPort, AiManagedFeaturePort, AiModelPort, AiPorts, AiProviderAuthPort,
    AiResourcesPort, AiSettingsPort, AiWorktreePort,
};
pub use view::{AiSettingsView, AiView};
