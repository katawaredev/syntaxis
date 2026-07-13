use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WorkspaceId(pub String);

impl WorkspaceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAvailability {
    Available,
    Missing,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceIconSymbol {
    Docker,
    Folder,
    Git,
    Go,
    Javascript,
    Nextjs,
    Node,
    Python,
    React,
    Rust,
    Storybook,
    Svelte,
    Typescript,
    Vercel,
    Vite,
    Vue,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceIcon {
    Image {
        relative_path: String,
        data_url: Option<String>,
    },
    Symbol {
        name: WorkspaceIconSymbol,
    },
}

impl Default for WorkspaceIcon {
    fn default() -> Self {
        Self::Symbol {
            name: WorkspaceIconSymbol::Folder,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceRecord {
    pub id: WorkspaceId,
    pub slug: String,
    pub name: String,
    /// Canonical absolute path as understood by the runtime.
    pub root: String,
    pub icon: WorkspaceIcon,
    pub registered_at_unix_ms: i64,
    pub last_opened_unix_ms: i64,
    pub availability: WorkspaceAvailability,
}
