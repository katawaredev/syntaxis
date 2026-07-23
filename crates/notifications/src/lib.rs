//! Shared application notification protocol.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    Completed,
    Attention,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationTarget {
    Agent { session_id: String },
    Terminal { session_id: String },
}

impl NotificationTarget {
    pub fn session_id(&self) -> &str {
        match self {
            Self::Agent { session_id } | Self::Terminal { session_id } => session_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppNotification {
    pub workspace_id: String,
    pub workspace_slug: String,
    pub workspace_name: String,
    pub target: NotificationTarget,
    pub title: String,
    pub kind: NotificationKind,
    pub message: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationClientMessage {
    Hello {
        version: u16,
    },
    Clear {
        workspace_id: String,
        target: NotificationTarget,
    },
    Ping {
        nonce: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationServerMessage {
    Hello {
        version: u16,
    },
    Snapshot {
        notifications: Vec<AppNotification>,
    },
    Upsert {
        notification: AppNotification,
    },
    Removed {
        workspace_id: String,
        target: NotificationTarget,
    },
    Error {
        message: String,
    },
    Pong {
        nonce: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_round_trips_with_its_kind() {
        let target = NotificationTarget::Terminal {
            session_id: "terminal-1".into(),
        };
        let json = serde_json::to_string(&target).expect("notification target should serialize");
        assert_eq!(
            serde_json::from_str::<NotificationTarget>(&json)
                .expect("serialized notification target should deserialize"),
            target
        );
    }
}
