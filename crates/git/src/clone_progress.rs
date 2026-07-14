use serde::{Deserialize, Serialize};
use syntaxis_workspace::WorkspaceRecord;

pub const CLONE_PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClonePhase {
    Preparing,
    Counting,
    Compressing,
    Receiving,
    Resolving,
    CheckingOut,
    Finalizing,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CloneProgress {
    pub phase: ClonePhase,
    pub percent: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CloneClientMessage {
    Start {
        version: u16,
        url: String,
        destination_parent: String,
    },
    Cancel,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CloneServerMessage {
    Started,
    Progress { progress: CloneProgress },
    Completed { workspace: WorkspaceRecord },
    Cancelled,
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::{CloneClientMessage, CLONE_PROTOCOL_VERSION};

    #[test]
    fn clone_start_carries_an_explicit_protocol_version() {
        let message = CloneClientMessage::Start {
            version: CLONE_PROTOCOL_VERSION,
            url: "https://example.invalid/repository.git".into(),
            destination_parent: "/srv/projects".into(),
        };
        assert!(matches!(
            message,
            CloneClientMessage::Start {
                version: CLONE_PROTOCOL_VERSION,
                ..
            }
        ));
    }
}
