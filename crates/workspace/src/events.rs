use serde::{Deserialize, Serialize};

use crate::{RelativePath, WorkspaceId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceChange {
    pub workspace_id: WorkspaceId,
    pub path: RelativePath,
    pub kind: ChangeKind,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventBatch {
    pub changes: Vec<WorkspaceChange>,
}
