use std::collections::HashMap;

use async_trait::async_trait;

use crate::{
    BrowseDirectory, BrowseRoot, ErrorCode, WorkspaceBrowser, WorkspaceError, WorkspaceResult,
};

#[derive(Clone, Debug, Default)]
pub struct MockWorkspaceBrowser {
    roots: Vec<BrowseRoot>,
    directories: HashMap<String, Vec<BrowseDirectory>>,
}

impl MockWorkspaceBrowser {
    pub fn new(roots: Vec<BrowseRoot>, directories: HashMap<String, Vec<BrowseDirectory>>) -> Self {
        Self { roots, directories }
    }
}

#[async_trait(?Send)]
impl WorkspaceBrowser for MockWorkspaceBrowser {
    async fn roots(&self) -> WorkspaceResult<Vec<BrowseRoot>> {
        Ok(self.roots.clone())
    }

    async fn directories(&self, absolute_path: &str) -> WorkspaceResult<Vec<BrowseDirectory>> {
        self.directories.get(absolute_path).cloned().ok_or_else(|| {
            WorkspaceError::new(
                ErrorCode::OutsideAllowedRoot,
                "The path is outside the mock browser roots.",
            )
        })
    }
}
