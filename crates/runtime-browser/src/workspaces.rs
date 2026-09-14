use async_trait::async_trait;
use syntaxis_app_contracts::AppError;
use syntaxis_app_shell::{RuntimeStatusPort, WorkspaceCatalogPort};
use syntaxis_workspace::{
    ExecutionLocation, RuntimeCapabilities, RuntimeCapability, RuntimeIdentity, RuntimeState,
    WorkspaceAvailability, WorkspaceIcon, WorkspaceIconSymbol, WorkspaceId, WorkspaceProfile,
    WorkspaceRecord, WorkspaceSection,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserWorkspaceCatalog;

#[async_trait(?Send)]
impl WorkspaceCatalogPort for BrowserWorkspaceCatalog {
    async fn list(&self) -> Result<Vec<WorkspaceRecord>, AppError> {
        Ok(Vec::new())
    }

    async fn resolve(&self, slug: &str) -> Result<WorkspaceRecord, AppError> {
        Ok(browser_workspace(slug))
    }

    async fn touch(&self, _workspace: &WorkspaceRecord) -> Result<(), AppError> {
        Ok(())
    }

    async fn remember_section(
        &self,
        _workspace: &WorkspaceRecord,
        _section: WorkspaceSection,
    ) -> Result<(), AppError> {
        Ok(())
    }
}

#[async_trait(?Send)]
impl RuntimeStatusPort for BrowserWorkspaceCatalog {
    async fn state(&self) -> Result<RuntimeState, AppError> {
        Ok(RuntimeState::Ready {
            identity: RuntimeIdentity {
                location: ExecutionLocation::Local,
                label: "Browser runtime".into(),
            },
            capabilities: RuntimeCapabilities {
                available: vec![
                    RuntimeCapability::Filesystem,
                    RuntimeCapability::Terminal,
                    RuntimeCapability::Git,
                    RuntimeCapability::Agent,
                    RuntimeCapability::Preview,
                ],
            },
        })
    }
}

fn browser_workspace(slug: &str) -> WorkspaceRecord {
    WorkspaceRecord {
        id: WorkspaceId::new("browser-opfs"),
        slug: slug.to_owned(),
        name: if slug == "browser" {
            "Browser workspace".into()
        } else {
            slug.replace('-', " ")
        },
        root: "opfs://syntaxis-browser".into(),
        icon: WorkspaceIcon::Symbol {
            name: WorkspaceIconSymbol::Folder,
        },
        profile: WorkspaceProfile::default(),
        registered_at_unix_ms: 0,
        last_opened_unix_ms: 0,
        last_section: WorkspaceSection::Files,
        availability: WorkspaceAvailability::Available,
    }
}
