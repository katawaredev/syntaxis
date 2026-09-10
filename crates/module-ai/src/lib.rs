//! Canonical runtime-neutral AI views and capability ports.

mod models;
mod ports;
mod view;

pub use models::{
    AiAuthEvent, AiAuthFlow, AiAuthPrompt, AiAuthPromptOption, AiConversation,
    AiConversationSummary, AiEvent, AiExtension, AiExtensionAction, AiExtensionPage,
    AiFeatureSummary, AiManagedFeature, AiMessage, AiModel, AiPrompt, AiPromptDelivery,
    AiPromptTemplate, AiProviderAccount, AiProviderAuthKind, AiProviderAuthMethod,
    AiProviderSettings, AiResourceScope, AiRole, AiSkill,
};
pub use ports::{
    AiConversationPort, AiEventStream, AiExtensionsPort, AiManagedFeaturePort, AiModelPort,
    AiPorts, AiProviderAuthPort, AiResourcesPort, AiSettingsPort, AiWorktreePort,
};
pub use view::{AiSettingsView, AiView};
