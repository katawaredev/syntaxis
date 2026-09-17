use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, PortHandle};
use syntaxis_terminal::RunCommand;
use syntaxis_workspace::WorkspaceRecord;

use crate::{
    PreviewCandidate, PreviewConfig, PreviewLease, PreviewProcessStatus, PreviewSession,
    PreviewShare, PreviewTarget,
};

#[async_trait(?Send)]
pub trait PreviewPort: Send + Sync {
    async fn candidates(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<PreviewCandidate>, AppError>;
    async fn open(
        &self,
        workspace: &WorkspaceRecord,
        target: PreviewTarget,
    ) -> Result<PreviewLease, AppError>;
    async fn resume(&self, workspace: &WorkspaceRecord)
    -> Result<Option<PreviewSession>, AppError>;
    async fn refresh(
        &self,
        workspace: &WorkspaceRecord,
        lease: &PreviewLease,
    ) -> Result<PreviewLease, AppError>;
    async fn close(&self, workspace: &WorkspaceRecord, lease: PreviewLease)
    -> Result<(), AppError>;
}

/// Optional persistence for preview target and process settings.
#[async_trait(?Send)]
pub trait PreviewConfigPort: Send + Sync {
    async fn load(&self, workspace: &WorkspaceRecord) -> Result<PreviewConfig, AppError>;
    async fn save(
        &self,
        workspace: &WorkspaceRecord,
        config: PreviewConfig,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait PreviewProcessPort: Send + Sync {
    async fn commands(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError>;
    async fn status(&self, workspace: &WorkspaceRecord) -> Result<PreviewProcessStatus, AppError>;
    async fn start(
        &self,
        workspace: &WorkspaceRecord,
        start_command: &str,
        stop_command: &str,
    ) -> Result<PreviewProcessStatus, AppError>;
    async fn stop(
        &self,
        workspace: &WorkspaceRecord,
        stop_command: &str,
    ) -> Result<PreviewProcessStatus, AppError>;
}

#[async_trait(?Send)]
pub trait PreviewSharePort: Send + Sync {
    async fn create(
        &self,
        workspace: &WorkspaceRecord,
        lease: &PreviewLease,
    ) -> Result<PreviewShare, AppError>;
    async fn revoke(
        &self,
        workspace: &WorkspaceRecord,
        lease: &PreviewLease,
    ) -> Result<(), AppError>;
}

#[derive(Clone, Default)]
pub struct PreviewPorts {
    preview: Option<PortHandle<dyn PreviewPort>>,
    config: Option<PortHandle<dyn PreviewConfigPort>>,
    process: Option<PortHandle<dyn PreviewProcessPort>>,
    share: Option<PortHandle<dyn PreviewSharePort>>,
}

impl PreviewPorts {
    #[must_use]
    pub fn with_preview(mut self, preview: PortHandle<dyn PreviewPort>) -> Self {
        self.preview = Some(preview);
        self
    }

    #[must_use]
    pub fn with_process(mut self, process: PortHandle<dyn PreviewProcessPort>) -> Self {
        self.process = Some(process);
        self
    }

    #[must_use]
    pub fn with_config(mut self, config: PortHandle<dyn PreviewConfigPort>) -> Self {
        self.config = Some(config);
        self
    }

    #[must_use]
    pub fn with_share(mut self, share: PortHandle<dyn PreviewSharePort>) -> Self {
        self.share = Some(share);
        self
    }

    pub fn preview(&self) -> Option<&PortHandle<dyn PreviewPort>> {
        self.preview.as_ref()
    }

    pub fn config(&self) -> Option<&PortHandle<dyn PreviewConfigPort>> {
        self.config.as_ref()
    }

    pub fn process(&self) -> Option<&PortHandle<dyn PreviewProcessPort>> {
        self.process.as_ref()
    }

    pub fn share(&self) -> Option<&PortHandle<dyn PreviewSharePort>> {
        self.share.as_ref()
    }

    /// Verifies the core Preview lifecycle and dependent capabilities.
    ///
    /// # Errors
    ///
    /// Returns an error when dependent Preview capabilities are missing.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.preview.is_none() {
            return Err("Preview requires a lifecycle port");
        }
        if self.process.is_some() && self.config.is_none() {
            return Err("process Preview requires configuration persistence");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::PreviewPorts;

    #[test]
    fn optional_preview_capabilities_are_absent_by_default() {
        let ports = PreviewPorts::default();
        assert!(ports.preview().is_none());
        assert!(ports.config().is_none());
        assert!(ports.process().is_none());
        assert!(ports.share().is_none());
        assert!(ports.validate().is_err());
    }
}
