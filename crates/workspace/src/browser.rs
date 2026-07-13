use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::WorkspaceResult;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowseRoot {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowseDirectory {
    pub name: String,
    pub path: String,
}

#[async_trait(?Send)]
pub trait WorkspaceBrowser: Send + Sync {
    async fn roots(&self) -> WorkspaceResult<Vec<BrowseRoot>>;
    async fn directories(&self, absolute_path: &str) -> WorkspaceResult<Vec<BrowseDirectory>>;
}
