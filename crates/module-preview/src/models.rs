use serde::{Deserialize, Serialize};
use syntaxis_workspace::RelativePath;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreviewConfig {
    #[serde(default)]
    pub target: Option<PreviewTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    pub start_command: String,
    #[serde(default)]
    pub stop_command: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PreviewTarget {
    Loopback { port: u16 },
    Url { url: String },
    StaticDocument { path: RelativePath },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewLifecycle {
    Persistent,
    #[default]
    Ephemeral,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreviewLease {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub lifecycle: PreviewLifecycle,
    #[serde(default)]
    pub dependencies: Vec<RelativePath>,
}

impl PreviewLease {
    #[must_use]
    pub fn ephemeral(id: String, url: String, dependencies: Vec<RelativePath>) -> Self {
        Self {
            id,
            url,
            lifecycle: PreviewLifecycle::Ephemeral,
            dependencies,
        }
    }

    #[must_use]
    pub fn persistent(id: String, url: String) -> Self {
        Self {
            id,
            url,
            lifecycle: PreviewLifecycle::Persistent,
            dependencies: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreviewShare {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreviewSession {
    pub lease: PreviewLease,
    pub share: Option<PreviewShare>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreviewCandidate {
    pub port: u16,
    pub process: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreviewProcessStatus {
    pub running: bool,
}
