use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionLocation {
    Local,
    Remote,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeIdentity {
    pub location: ExecutionLocation,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapability {
    Filesystem,
    FileEvents,
    Terminal,
    Git,
    Worktrees,
    Agent,
    UnrestrictedWorkspaceRoots,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeCapabilities {
    pub available: Vec<RuntimeCapability>,
}

impl RuntimeCapabilities {
    pub fn supports(&self, capability: RuntimeCapability) -> bool {
        self.available.contains(&capability)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Connecting,
    Ready {
        identity: RuntimeIdentity,
        capabilities: RuntimeCapabilities,
    },
    Unavailable {
        message: String,
    },
}
