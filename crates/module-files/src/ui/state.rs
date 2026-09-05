//! Transitional view-only Files state.

use dioxus::prelude::*;
use syntaxis_git::{DiffKind, UnifiedDiff};
use syntaxis_ui::prelude::Tone;

pub(super) use crate::{
    ActiveBufferMeta, ActiveDocumentView, CloseRequest, FileAction, FileActionDialog,
    FilesController as FilesSessionState, OpenDocument, OpenTab,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RevertAction {
    Unsaved,
    Unstaged,
    Original,
}

impl RevertAction {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Unsaved => "Revert Unsaved Changes",
            Self::Unstaged => "Revert Unstaged Changes",
            Self::Original => "Revert to Original",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct GitRevertRequest {
    pub path: String,
    pub action: RevertAction,
}

#[derive(Clone)]
pub(super) struct OpenDiffRequest {
    pub workspace: WorkspaceRecord,
    pub kind: DiffKind,
    pub diff: Signal<Option<UnifiedDiff>>,
    pub toast: Signal<Option<ToastState>>,
}

#[derive(Clone, PartialEq)]
pub(super) struct ToastState {
    pub message: String,
    pub tone: Tone,
}
