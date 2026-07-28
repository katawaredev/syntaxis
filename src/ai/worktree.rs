use dioxus::prelude::*;
use syntaxis_git::{WorktreeCreateRequest, WorktreeInfo};
use syntaxis_ui::prelude::{
    Button, ButtonKind, DialogActions, DialogForm, Field, Modal, TextInput, Tone,
};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct WorktreeFlow {
    open: Signal<bool>,
    branch: Signal<String>,
    creating: Signal<bool>,
    error: Signal<Option<String>>,
    worktrees: Resource<Result<Vec<WorktreeInfo>, String>>,
}

impl WorktreeFlow {
    pub(super) fn open_dialog(mut self) {
        self.branch.set(default_isolated_branch());
        self.error.set(None);
        self.open.set(true);
    }

    pub(super) fn new_disabled_reason(
        self,
        active_workspace: crate::workspace::ActiveWorkspace,
        files_dirty: bool,
    ) -> Option<String> {
        if (self.worktrees)().is_none() {
            Some("Checking repository state…".to_owned())
        } else if !repository_has_commits(active_workspace) {
            Some("Create the repository's first commit before adding a worktree".to_owned())
        } else if files_dirty {
            Some("Save or close modified files before adding a worktree".to_owned())
        } else {
            None
        }
    }

    fn create_disabled(
        self,
        active_workspace: crate::workspace::ActiveWorkspace,
        files_dirty: bool,
    ) -> bool {
        (self.creating)()
            || files_dirty
            || !repository_has_commits(active_workspace)
            || (self.branch)().trim().is_empty()
    }
}

pub(super) fn use_worktree_flow(
    active_workspace: crate::workspace::ActiveWorkspace,
    mut toast: Signal<Option<(String, Tone)>>,
) -> WorktreeFlow {
    let open = use_signal(|| false);
    let branch = use_signal(String::new);
    let creating = use_signal(|| false);
    let error = use_signal(|| None::<String>);
    let worktrees = use_resource(move || {
        let base = active_workspace.base();
        let _ = active_workspace.refresh();
        async move {
            match base {
                Some(base) => crate::workspace::client::worktrees(base).await,
                None => Ok(Vec::new()),
            }
        }
    });
    use_effect(move || {
        let Some(result) = worktrees() else { return };
        match result {
            Ok(items) => active_workspace.reconcile(items),
            Err(message) => toast.set(Some((message, Tone::Destructive))),
        }
    });
    WorktreeFlow {
        open,
        branch,
        creating,
        error,
        worktrees,
    }
}

#[component]
pub(super) fn IsolatedWorktreeDialog(
    flow: WorktreeFlow,
    files_dirty: bool,
    active_workspace: crate::workspace::ActiveWorkspace,
    files_session: crate::files::FilesSessionState,
    event_state: crate::workspace::WorkspaceEventState,
) -> Element {
    if !(flow.open)() {
        return rsx! {};
    }
    let mut open = flow.open;
    let mut branch = flow.branch;
    let mut creating = flow.creating;
    let mut error = flow.error;
    let create_disabled = flow.create_disabled(active_workspace, files_dirty);
    rsx! {
        Modal {
            title: "Create a worktree",
            description: "Create a branch and checkout for an independent chat. Files, Terminal, and Git will switch to it too.",
            on_close: move |()| {
                if !creating() {
                    open.set(false);
                }
            },
            DialogForm {
                Field {
                    control_id: "agent-worktree-branch",
                    label: "New branch",
                    required: true,
                    error: error(),
                    TextInput {
                        value: branch(),
                        placeholder: "agent/chat-1234",
                        disabled: creating(),
                        oninput: move |event: FormEvent| {
                            branch.set(event.value());
                            error.set(None);
                        },
                    }
                }
                if files_dirty {
                    p { class: "text-xs leading-relaxed text-warning",
                        "Save or close modified files before starting an isolated chat."
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: creating(),
                        onclick: move |_| open.set(false),
                    }
                    Button {
                        label: if creating() { "Creating worktree…" } else { "Create worktree" },
                        kind: ButtonKind::Primary,
                        disabled: create_disabled,
                        onclick: move |_| {
                            let Some(base) = active_workspace.base() else {
                                error.set(Some("The registered workspace is unavailable.".into()));
                                return;
                            };
                            let request = WorktreeCreateRequest {
                                branch: branch(),
                                start_point: active_workspace.current_head(),
                                create_branch: true,
                            };
                            creating.set(true);
                            error.set(None);
                            spawn(async move {
                                match crate::workspace::client::create_worktree(base, request).await {
                                    Ok(worktree) => {
                                        let target_id = worktree.workspace.id.clone();
                                        active_workspace.request_new_agent_session(target_id);
                                        active_workspace.activate(worktree);
                                        files_session.reset();
                                        event_state.reset();
                                    }
                                    Err(message) => {
                                        error.set(Some(message));
                                        creating.set(false);
                                    }
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}

fn repository_has_commits(active_workspace: crate::workspace::ActiveWorkspace) -> bool {
    active_workspace
        .worktrees()
        .iter()
        .any(|worktree| worktree.head.chars().any(|character| character != '0'))
}

fn default_isolated_branch() -> String {
    let milliseconds = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("agent/chat-{milliseconds}")
}
