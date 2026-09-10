use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, PortHandle};
use syntaxis_git::{WorktreeCreateRequest, WorktreeInfo};
use syntaxis_workspace::WorkspaceRecord;

use crate::{
    AiAuthFlow, AiConversation, AiConversationSummary, AiEvent, AiExtensionAction, AiExtensionPage,
    AiFeatureSummary, AiManagedFeature, AiModel, AiPrompt, AiPromptTemplate, AiProviderAccount,
    AiProviderAuthKind, AiProviderSettings, AiSkill,
};

#[async_trait(?Send)]
pub trait AiEventStream {
    /// Returns the next normalized event, or `None` after the response has
    /// reached a terminal state.
    async fn receive(&mut self) -> Result<Option<AiEvent>, AppError>;
}

#[async_trait(?Send)]
pub trait AiConversationPort: Send + Sync {
    async fn list(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<AiConversationSummary>, AppError>;
    async fn create(&self, workspace: &WorkspaceRecord) -> Result<AiConversation, AppError>;
    async fn open(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<AiConversation, AppError>;
    async fn send(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        prompt: AiPrompt,
    ) -> Result<Box<dyn AiEventStream>, AppError>;
    async fn cancel(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait AiModelPort: Send + Sync {
    async fn list_models(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<Vec<AiModel>, AppError>;
    async fn select_model(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        model_id: &str,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait AiSettingsPort: Send + Sync {
    async fn load(&self, workspace: &WorkspaceRecord) -> Result<AiProviderSettings, AppError>;
    async fn save(
        &self,
        workspace: &WorkspaceRecord,
        settings: AiProviderSettings,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait AiManagedFeaturePort: Send + Sync {
    async fn summary(
        &self,
        workspace: &WorkspaceRecord,
        feature: AiManagedFeature,
    ) -> Result<AiFeatureSummary, AppError>;
}

#[async_trait(?Send)]
pub trait AiProviderAuthPort: Send + Sync {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<AiProviderAccount>, AppError>;
    async fn start(
        &self,
        workspace: &WorkspaceRecord,
        provider_id: &str,
        kind: AiProviderAuthKind,
    ) -> Result<AiAuthFlow, AppError>;
    async fn status(
        &self,
        workspace: &WorkspaceRecord,
        flow_id: &str,
    ) -> Result<AiAuthFlow, AppError>;
    async fn respond(
        &self,
        workspace: &WorkspaceRecord,
        flow_id: &str,
        prompt_id: u64,
        answer: &str,
    ) -> Result<(), AppError>;
    async fn cancel(&self, workspace: &WorkspaceRecord, flow_id: &str) -> Result<(), AppError>;
    async fn logout(&self, workspace: &WorkspaceRecord, provider_id: &str) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait AiResourcesPort: Send + Sync {
    async fn load_instructions(&self, workspace: &WorkspaceRecord) -> Result<String, AppError>;
    async fn save_instructions(
        &self,
        workspace: &WorkspaceRecord,
        content: &str,
    ) -> Result<(), AppError>;
    async fn prompt_templates(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<AiPromptTemplate>, AppError>;
    async fn save_prompt_template(
        &self,
        workspace: &WorkspaceRecord,
        original_name: Option<&str>,
        template: AiPromptTemplate,
    ) -> Result<(), AppError>;
    async fn delete_prompt_template(
        &self,
        workspace: &WorkspaceRecord,
        template: &AiPromptTemplate,
    ) -> Result<(), AppError>;
    async fn skills(&self, workspace: &WorkspaceRecord) -> Result<Vec<AiSkill>, AppError>;
    async fn save_skill(
        &self,
        workspace: &WorkspaceRecord,
        original_storage_name: Option<&str>,
        skill: AiSkill,
    ) -> Result<(), AppError>;
    async fn delete_skill(
        &self,
        workspace: &WorkspaceRecord,
        skill: &AiSkill,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait AiExtensionsPort: Send + Sync {
    async fn search(
        &self,
        workspace: &WorkspaceRecord,
        query: &str,
        offset: usize,
    ) -> Result<AiExtensionPage, AppError>;
    async fn manage(
        &self,
        workspace: &WorkspaceRecord,
        package_name: &str,
        action: AiExtensionAction,
    ) -> Result<String, AppError>;
}

#[async_trait(?Send)]
pub trait AiWorktreePort: Send + Sync {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<WorktreeInfo>, AppError>;
    async fn create(
        &self,
        workspace: &WorkspaceRecord,
        request: WorktreeCreateRequest,
    ) -> Result<WorktreeInfo, AppError>;
}

#[derive(Clone, Default)]
pub struct AiPorts {
    conversation: Option<PortHandle<dyn AiConversationPort>>,
    models: Option<PortHandle<dyn AiModelPort>>,
    settings: Option<PortHandle<dyn AiSettingsPort>>,
    usage: Option<PortHandle<dyn AiManagedFeaturePort>>,
    provider_auth: Option<PortHandle<dyn AiProviderAuthPort>>,
    resources: Option<PortHandle<dyn AiResourcesPort>>,
    extensions: Option<PortHandle<dyn AiExtensionsPort>>,
    worktrees: Option<PortHandle<dyn AiWorktreePort>>,
    notifications: Option<PortHandle<dyn AiManagedFeaturePort>>,
}

impl AiPorts {
    #[must_use]
    pub fn with_conversation(mut self, port: PortHandle<dyn AiConversationPort>) -> Self {
        self.conversation = Some(port);
        self
    }

    #[must_use]
    pub fn with_models(mut self, port: PortHandle<dyn AiModelPort>) -> Self {
        self.models = Some(port);
        self
    }

    #[must_use]
    pub fn with_settings(mut self, port: PortHandle<dyn AiSettingsPort>) -> Self {
        self.settings = Some(port);
        self
    }

    pub fn conversation(&self) -> Option<&PortHandle<dyn AiConversationPort>> {
        self.conversation.as_ref()
    }

    pub fn models(&self) -> Option<&PortHandle<dyn AiModelPort>> {
        self.models.as_ref()
    }

    pub fn settings(&self) -> Option<&PortHandle<dyn AiSettingsPort>> {
        self.settings.as_ref()
    }

    #[must_use]
    pub fn with_usage(mut self, port: PortHandle<dyn AiManagedFeaturePort>) -> Self {
        self.usage = Some(port);
        self
    }

    pub fn usage(&self) -> Option<&PortHandle<dyn AiManagedFeaturePort>> {
        self.usage.as_ref()
    }

    #[must_use]
    pub fn with_provider_auth(mut self, port: PortHandle<dyn AiProviderAuthPort>) -> Self {
        self.provider_auth = Some(port);
        self
    }

    pub fn provider_auth(&self) -> Option<&PortHandle<dyn AiProviderAuthPort>> {
        self.provider_auth.as_ref()
    }

    #[must_use]
    pub fn with_resources(mut self, port: PortHandle<dyn AiResourcesPort>) -> Self {
        self.resources = Some(port);
        self
    }

    pub fn resources(&self) -> Option<&PortHandle<dyn AiResourcesPort>> {
        self.resources.as_ref()
    }

    #[must_use]
    pub fn with_extensions(mut self, port: PortHandle<dyn AiExtensionsPort>) -> Self {
        self.extensions = Some(port);
        self
    }

    pub fn extensions(&self) -> Option<&PortHandle<dyn AiExtensionsPort>> {
        self.extensions.as_ref()
    }

    #[must_use]
    pub fn with_worktrees(mut self, port: PortHandle<dyn AiWorktreePort>) -> Self {
        self.worktrees = Some(port);
        self
    }

    pub fn worktrees(&self) -> Option<&PortHandle<dyn AiWorktreePort>> {
        self.worktrees.as_ref()
    }

    #[must_use]
    pub fn with_notifications(mut self, port: PortHandle<dyn AiManagedFeaturePort>) -> Self {
        self.notifications = Some(port);
        self
    }

    pub fn notifications(&self) -> Option<&PortHandle<dyn AiManagedFeaturePort>> {
        self.notifications.as_ref()
    }

    /// Verifies the required conversation/model surface and one settings path.
    ///
    /// # Errors
    ///
    /// Returns an error when required AI capabilities are missing.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.conversation.is_none() {
            return Err("AI requires a conversation port");
        }
        if self.models.is_none() {
            return Err("AI requires a model port");
        }
        if self.settings.is_none() && self.provider_auth.is_none() {
            return Err("AI requires provider settings or provider authentication");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AiPorts;

    #[test]
    fn managed_capabilities_are_absent_by_default() {
        let ports = AiPorts::default();
        assert!(ports.usage().is_none());
        assert!(ports.provider_auth().is_none());
        assert!(ports.resources().is_none());
        assert!(ports.extensions().is_none());
        assert!(ports.worktrees().is_none());
        assert!(ports.notifications().is_none());
        assert!(ports.validate().is_err());
    }
}
