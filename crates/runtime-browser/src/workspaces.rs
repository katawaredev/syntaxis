use crate::git::git_request;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use syntaxis_app_contracts::AppError;
use syntaxis_app_contracts::{AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_app_shell::{
    RuntimeStatusPort, WorkspaceCatalogPort, WorkspaceCloneEvent, WorkspaceClonePort,
    WorkspaceCloneStream,
};
use syntaxis_git::{ClonePhase, CloneProgress, CloneRequest};
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
        let names: Vec<String> = git_request("listProjects", serde_json::Value::Null).await?;
        Ok(names.iter().map(|name| cloned_workspace(name)).collect())
    }

    async fn resolve(&self, slug: &str) -> Result<WorkspaceRecord, AppError> {
        if let Some(name) = slug.strip_prefix("git-") {
            let names: Vec<String> = git_request("listProjects", serde_json::Value::Null).await?;
            if names.iter().any(|entry| entry == name) {
                syntaxis_workspace_browser::select_private_project(name)
                    .await
                    .map_err(AppError::from)?;
                return Ok(cloned_workspace(name));
            }
            return Err(clone_error("This browser Git project is unavailable."));
        }
        if slug == "browser" {
            syntaxis_workspace_browser::set_private_workspace();
        }
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

fn cloned_workspace(name: &str) -> WorkspaceRecord {
    let mut workspace = browser_workspace(&format!("git-{name}"));
    workspace.id = WorkspaceId::new(format!("browser-git-{name}"));
    workspace.name = name.to_owned();
    workspace.root = format!("opfs://syntaxis-browser/.syntaxis-repositories/{name}");
    workspace
}

fn clone_error(message: impl Into<String>) -> AppError {
    AppError::new(
        AppErrorCode::InvalidInput,
        message,
        RetryAdvice::AfterUserAction,
        ErrorSource::Git,
    )
}

#[derive(Deserialize)]
struct CloneStatus {
    done: bool,
    cancelled: bool,
    error: Option<String>,
    name: String,
    phase: ClonePhase,
    percent: Option<u8>,
}

struct BrowserCloneStream {
    id: String,
    finished: bool,
}

#[async_trait(?Send)]
impl WorkspaceClonePort for BrowserWorkspaceCatalog {
    fn supports_blobless(&self) -> bool {
        false
    }

    fn destination_description(&self) -> &'static str {
        "Clone an HTTPS repository into private browser storage. Use /project-name as the destination. Full and shallow clones are supported."
    }

    async fn start(
        &self,
        request: CloneRequest,
    ) -> Result<Box<dyn WorkspaceCloneStream>, AppError> {
        let id = git_request("startClone", json!(request)).await?;
        Ok(Box::new(BrowserCloneStream {
            id,
            finished: false,
        }))
    }
}

#[async_trait(?Send)]
impl WorkspaceCloneStream for BrowserCloneStream {
    async fn receive(&mut self) -> Result<Option<WorkspaceCloneEvent>, AppError> {
        if self.finished {
            return Ok(None);
        }
        let status: CloneStatus = git_request("cloneStatus", json!(self.id)).await?;
        if !status.done {
            return Ok(Some(WorkspaceCloneEvent::Progress(CloneProgress {
                phase: status.phase,
                percent: status.percent,
            })));
        }
        self.finished = true;
        git_request::<bool>("finishClone", json!(self.id)).await?;
        if status.cancelled {
            return Ok(Some(WorkspaceCloneEvent::Cancelled));
        }
        if let Some(error) = status.error {
            return Err(clone_error(error));
        }
        syntaxis_workspace_browser::select_private_project(&status.name)
            .await
            .map_err(AppError::from)?;
        Ok(Some(WorkspaceCloneEvent::Completed(Box::new(
            cloned_workspace(&status.name),
        ))))
    }

    async fn cancel(&self) -> Result<(), AppError> {
        git_request::<bool>("cancelClone", json!(self.id)).await?;
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
