use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_workspace::{FileSession, WorkspaceId, WorkspaceRecord};

use crate::{FilesSessionPort, SearchRequest, SearchResults, WorkspaceSearchPort};

/// In-memory session adapter for controller tests and non-persistent previews.
#[derive(Default)]
pub struct MemoryFilesSession {
    sessions: Mutex<HashMap<WorkspaceId, FileSession>>,
}

#[async_trait(?Send)]
impl FilesSessionPort for MemoryFilesSession {
    async fn load(&self, workspace_id: &WorkspaceId) -> Result<FileSession, AppError> {
        Ok(self
            .sessions
            .lock()
            .map_err(|_error| memory_error())?
            .get(workspace_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn save(&self, workspace_id: &WorkspaceId, session: FileSession) -> Result<(), AppError> {
        self.sessions
            .lock()
            .map_err(|_error| memory_error())?
            .insert(workspace_id.clone(), session);
        Ok(())
    }
}

/// Deterministic search adapter for controller tests.
#[derive(Clone, Debug, Default)]
pub struct FixedWorkspaceSearch {
    results: SearchResults,
}

impl FixedWorkspaceSearch {
    pub fn new(results: SearchResults) -> Self {
        Self { results }
    }
}

#[async_trait(?Send)]
impl WorkspaceSearchPort for FixedWorkspaceSearch {
    async fn search(
        &self,
        _workspace: &WorkspaceRecord,
        _request: SearchRequest,
    ) -> Result<SearchResults, AppError> {
        Ok(self.results.clone())
    }
}

fn memory_error() -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        "The in-memory Files adapter is unavailable.",
        RetryAdvice::Immediate,
        ErrorSource::Files,
    )
}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;

    use super::*;

    #[test]
    fn sessions_are_isolated_by_workspace() {
        let sessions = MemoryFilesSession::default();
        let first = WorkspaceId::new("first");
        let second = WorkspaceId::new("second");
        block_on(sessions.save(
            &first,
            FileSession {
                tabs: vec!["src/main.rs".into()],
                active: Some("src/main.rs".into()),
            },
        ))
        .unwrap();
        assert_eq!(block_on(sessions.load(&first)).unwrap().tabs.len(), 1);
        assert_eq!(
            block_on(sessions.load(&second)).unwrap(),
            FileSession::default()
        );
    }
}
