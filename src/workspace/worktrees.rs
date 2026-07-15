use dioxus::prelude::*;
use syntaxis_git::WorktreeInfo;
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ActiveWorkspace {
    pub base: Signal<Option<WorkspaceRecord>>,
    pub current: Signal<Option<WorkspaceRecord>>,
    worktrees: Signal<Vec<WorktreeInfo>>,
    refresh: Signal<u64>,
    pending_agent_session: Signal<Option<WorkspaceId>>,
}

pub(crate) fn use_active_workspace() -> ActiveWorkspace {
    ActiveWorkspace {
        base: use_signal(|| None),
        current: use_signal(|| None),
        worktrees: use_signal(Vec::new),
        refresh: use_signal(|| 0),
        pending_agent_session: use_signal(|| None),
    }
}

impl ActiveWorkspace {
    pub(crate) fn base(self) -> Option<WorkspaceRecord> {
        (self.base)()
    }

    pub(crate) fn current(self) -> Option<WorkspaceRecord> {
        (self.current)()
    }

    pub(crate) fn current_head(self) -> Option<String> {
        let current_id = self.current.peek().as_ref()?.id.clone();
        self.worktrees
            .peek()
            .iter()
            .find(|worktree| worktree.workspace.id == current_id)
            .map(|worktree| worktree.head.clone())
    }

    pub(crate) fn activate(mut self, worktree: WorktreeInfo) {
        let workspace = worktree.workspace.clone();
        let mut worktrees = self.worktrees.write();
        if let Some(existing) = worktrees
            .iter_mut()
            .find(|existing| existing.workspace.id == workspace.id)
        {
            *existing = worktree;
        } else {
            worktrees.push(worktree);
        }
        drop(worktrees);
        self.current.set(Some(workspace));
        *self.refresh.write() += 1;
    }

    pub(crate) fn request_new_agent_session(mut self, workspace_id: WorkspaceId) {
        self.pending_agent_session.set(Some(workspace_id));
    }

    pub(crate) fn should_create_agent_session(self, workspace_id: &WorkspaceId) -> bool {
        self.pending_agent_session.peek().as_ref() == Some(workspace_id)
    }

    pub(crate) fn complete_agent_session_request(mut self, workspace_id: &WorkspaceId) {
        if self.pending_agent_session.peek().as_ref() == Some(workspace_id) {
            self.pending_agent_session.set(None);
        }
    }

    pub(crate) fn worktrees(self) -> Vec<WorktreeInfo> {
        (self.worktrees)()
    }

    pub(crate) fn refresh(self) -> u64 {
        (self.refresh)()
    }

    pub(crate) fn reconcile(mut self, items: Vec<WorktreeInfo>) {
        let selected_id = self.current.peek().as_ref().map(|item| item.id.clone());
        let selected = selected_id
            .as_ref()
            .and_then(|id| items.iter().find(|item| item.workspace.id == *id))
            .or_else(|| items.iter().find(|item| item.is_primary()))
            .map(|item| item.workspace.clone());
        self.worktrees.set(items);
        if let Some(selected) = selected {
            self.current.set(Some(selected));
        }
    }

    pub(crate) fn set_base(mut self, workspace: WorkspaceRecord) {
        let unchanged = self
            .base
            .peek()
            .as_ref()
            .is_some_and(|base| base.id == workspace.id);
        if unchanged {
            return;
        }
        self.base.set(Some(workspace.clone()));
        self.current.set(Some(workspace));
        self.worktrees.set(Vec::new());
        self.pending_agent_session.set(None);
        *self.refresh.write() += 1;
    }

    pub(crate) fn select(mut self, workspace_id: &str) -> bool {
        let selected = self
            .worktrees
            .peek()
            .iter()
            .find(|worktree| worktree.workspace.id.0 == workspace_id)
            .map(|worktree| worktree.workspace.clone());
        if let Some(workspace) = selected {
            self.current.set(Some(workspace));
            true
        } else {
            false
        }
    }

    pub(crate) fn forget_worktree(mut self, workspace_id: &str) {
        self.worktrees
            .write()
            .retain(|worktree| worktree.workspace.id.0 != workspace_id);
        if self
            .current
            .peek()
            .as_ref()
            .is_some_and(|current| current.id.0 == workspace_id)
        {
            self.current.set(self.base.peek().clone());
        }
        *self.refresh.write() += 1;
    }
}
