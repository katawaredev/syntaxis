use serde::{Deserialize, Serialize};

const SESSION_VERSION: u32 = 1;

/// Persisted UI state scoped to one registered workspace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceSession {
    #[serde(default = "session_version")]
    pub version: u32,
    #[serde(default)]
    pub files: FileSession,
}

impl Default for WorkspaceSession {
    fn default() -> Self {
        Self {
            version: SESSION_VERSION,
            files: FileSession::default(),
        }
    }
}

/// Restorable Files-module state. Buffer contents are intentionally excluded.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileSession {
    #[serde(default)]
    pub tabs: Vec<String>,
    #[serde(default)]
    pub active: Option<String>,
}

const fn session_version() -> u32 {
    SESSION_VERSION
}
