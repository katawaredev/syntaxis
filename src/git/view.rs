use dioxus::prelude::*;
use dioxus_code_editor::{DiffLayout, UnifiedDiffView};
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use syntaxis_editor::language_slug_for_path;
use syntaxis_git::{
    BranchComparison, BranchInfo, ChangeKind, CommitDetail, CommitInfo, CommitOutcome,
    CommitRequest, ConflictChoice, ConflictFile, DiffHunk, DiffKind, FileChange, HunkAction,
    RemoteInfo, RemoteRequest, RepositoryState, RepositoryStatus, TagInfo, TagRequest, UnifiedDiff,
    parse_diff_hunks,
};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, Checkbox, ControlSize, DialogActions, DialogForm, Drawer, Field,
    FileIcon, GitChangeBadge, Icon, IconButton, MenuButtonTrigger, MenuContent, MenuTrigger, Modal,
    PanelHeader, PanelHeaderKind, TextArea, TextInput, TextInputType, Toast, Tone,
};

use crate::client_error::server_error_message;

#[path = "changes.rs"]
mod changes;
#[path = "view/controller.rs"]
mod controller;
#[path = "dialogs.rs"]
mod dialogs;
#[path = "history.rs"]
mod history;
#[path = "remotes.rs"]
mod remotes;
#[path = "view/support.rs"]
mod support;
#[path = "sync.rs"]
mod sync;
#[path = "worktrees.rs"]
mod worktrees;

use self::changes::{ChangeDetail, GitSidebar, RawPatch};
use self::controller::{
    RepositoryActionSignals, compare_handler, mutation_handler, repository_action_handler,
};
use self::dialogs::{
    AbortMergeDialog, BranchDialog, CommitDialog, CommitHistoryActionDialog, CompareMergeDialog,
    DiscardAllDialog, ForcePushDialog, RemoteDialog, RemoveRemoteDialog, SigningDialog, TagDialog,
};
use self::history::HistoryDetail;
use self::remotes::RemoteManager;
use self::support::{
    RepositoryWelcome, branch_request, copy_commit_hash, diff_line_class, display_remote_url,
    remote_request, short_oid,
};
use self::sync::{GitSyncAction, GitSyncButton};
use self::worktrees::{BranchWorktreeAction, BranchWorktreeMenu};
use super::api;
use super::operations::{Mutation, RepositoryAction};
use super::repository::{RepositoryResources, SelectedChange, use_repository_resources};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum GitDialog {
    #[default]
    None,
    Commit,
    SigningRetry,
    CreateBranch,
    RenameBranch,
    DeleteBranch,
    Tags,
    CheckoutCommit,
    RevertCommit,
    CompareMerge,
    AbortMerge,
    ForcePush,
    DiscardAll,
    AddRemote,
    EditRemote,
    RemoveRemote,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SidebarView {
    #[default]
    Changes,
    History,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HistoryAction {
    Checkout,
    CreateBranch,
    CreateTag,
    Revert,
    CopyHash,
}

#[component]
pub fn Git(slug: String) -> Element {
    let _ = slug;
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    match active.current() {
        Some(workspace) => rsx! {
            WorkspaceGit { key: "{workspace.id.0}", slug: workspace.id.0 }
        },
        None => rsx! {
            div {
                class: "flex size-full items-center justify-center gap-2 bg-card text-sm text-muted-foreground",
                role: "status",
                span {
                    class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                    aria_hidden: "true",
                }
                "Loading workspace Git checkout…"
            }
        },
    }
}

#[component]
fn WorkspaceGit(slug: String) -> Element {
    let mut refresh_key = use_signal(|| 0_u64);
    let mut selected = use_signal(|| None::<SelectedChange>);
    let mut expanded_diff = use_signal(|| false);
    let view = use_signal(SidebarView::default);
    let selected_commit = use_signal(|| None::<String>);
    let RepositoryResources {
        snapshot,
        diff,
        conflict,
        commit_detail,
    } = use_repository_resources(&slug, refresh_key, selected, expanded_diff, selected_commit);
    let mut drawer = use_signal(|| false);
    let mut sidebar_open = use_signal(|| true);
    let mut branch_dialog_target = use_signal(|| None::<String>);
    let mut branch_start_point = use_signal(|| None::<String>);
    let mut tag_target = use_signal(|| None::<String>);
    let mut branch_menu = use_signal(|| false);
    let mut remote_target = use_signal(|| None::<RemoteInfo>);
    let mut pending = use_signal(|| false);
    let refreshing = use_signal(|| false);
    let mut dialog = use_signal(GitDialog::default);
    let mut toast = use_signal(|| None::<(String, Tone)>);
    let mut operation_error = use_signal(|| None::<String>);
    let mut retry_commit = use_signal(|| None::<CommitRequest>);
    let mut comparison = use_signal(|| None::<BranchComparison>);
    let mut compare_target = use_signal(|| None::<String>);

    let drawer_blocked = dialog() != GitDialog::None;
    use_effect(move || {
        if dialog() != GitDialog::None {
            drawer.set(false);
        }
    });
    use_effect(move || {
        let _ = selected();
        expanded_diff.set(false);
    });

    use_effect(move || {
        if dialog() == GitDialog::None
            && let Some(error) = operation_error()
        {
            toast.set(Some((error, Tone::Destructive)));
        }
    });

    use_effect(move || match snapshot() {
        Some(Err(error)) => {
            toast.set(Some((server_error_message(error), Tone::Destructive)));
        }
        Some(Ok(snapshot)) => {
            if let Err(error) = snapshot.remotes {
                toast.set(Some((error.to_string(), Tone::Destructive)));
            }
        }
        None => {}
    });

    let initialize_slug = slug.clone();
    let on_initialize = move |_| {
        let slug = initialize_slug.clone();
        pending.set(true);
        operation_error.set(None);
        spawn(async move {
            let result = api::initialize_repository(slug).await;
            pending.set(false);
            match result {
                Ok(()) => {
                    *refresh_key.write() += 1;
                    toast.set(Some(("Initialized Git repository".into(), Tone::Success)));
                }
                Err(error) => operation_error.set(Some(server_error_message(error))),
            }
        });
    };

    let on_mutation = mutation_handler(
        slug.clone(),
        pending,
        operation_error,
        selected,
        dialog,
        refresh_key,
        toast,
    );
    let on_repository_action = repository_action_handler(
        slug.clone(),
        RepositoryActionSignals {
            pending,
            refreshing,
            operation_error,
            selected,
            dialog,
            refresh_key,
            toast,
        },
    );
    let on_compare = compare_handler(slug.clone(), pending, operation_error, comparison);

    let commit_slug = slug.clone();
    let on_commit = move |request: CommitRequest| {
        let slug = commit_slug.clone();
        let retry = CommitRequest {
            message: request.message.clone(),
            amend: request.amend,
            skip_hooks: request.skip_hooks,
            signing_passphrase: None,
        };
        pending.set(true);
        operation_error.set(None);
        spawn(async move {
            let result = api::commit_changes(slug, request).await;
            pending.set(false);
            match result {
                Ok(CommitOutcome::Committed { commit }) => {
                    dialog.set(GitDialog::None);
                    retry_commit.set(None);
                    selected.set(None);
                    *refresh_key.write() += 1;
                    toast.set(Some((
                        format!("Committed {} · {}", short_oid(&commit.oid), commit.summary),
                        Tone::Success,
                    )));
                }
                Ok(CommitOutcome::SigningPassphraseRequired { message }) => {
                    retry_commit.set(Some(retry));
                    operation_error.set(Some(message));
                    dialog.set(GitDialog::SigningRetry);
                }
                Err(error) => operation_error.set(Some(server_error_message(error))),
            }
        });
    };

    let on_history_action = EventHandler::new(move |(action, oid): (HistoryAction, String)| {
        let mut selected_commit = selected_commit;
        selected_commit.set(Some(oid.clone()));
        operation_error.set(None);
        match action {
            HistoryAction::Checkout => dialog.set(GitDialog::CheckoutCommit),
            HistoryAction::CreateBranch => {
                branch_dialog_target.set(None);
                branch_start_point.set(Some(oid));
                dialog.set(GitDialog::CreateBranch);
            }
            HistoryAction::CreateTag => {
                tag_target.set(Some(oid));
                dialog.set(GitDialog::Tags);
            }
            HistoryAction::Revert => dialog.set(GitDialog::RevertCommit),
            HistoryAction::CopyHash => copy_commit_hash(oid, toast),
        }
    });

    let signing_slug = slug.clone();
    let on_signing_retry = move |passphrase: String| {
        let Some(mut request) = retry_commit() else {
            operation_error.set(Some("The commit retry is no longer available.".into()));
            return;
        };
        request.signing_passphrase = Some(passphrase);
        let retry = CommitRequest {
            message: request.message.clone(),
            amend: request.amend,
            skip_hooks: request.skip_hooks,
            signing_passphrase: None,
        };
        let slug = signing_slug.clone();
        pending.set(true);
        operation_error.set(None);
        spawn(async move {
            let result = api::commit_changes(slug, request).await;
            pending.set(false);
            match result {
                Ok(CommitOutcome::Committed { commit }) => {
                    dialog.set(GitDialog::None);
                    retry_commit.set(None);
                    selected.set(None);
                    *refresh_key.write() += 1;
                    toast.set(Some((
                        format!(
                            "Signed commit {} · {}",
                            short_oid(&commit.oid),
                            commit.summary
                        ),
                        Tone::Success,
                    )));
                }
                Ok(CommitOutcome::SigningPassphraseRequired { message }) => {
                    retry_commit.set(Some(retry));
                    operation_error.set(Some(message));
                }
                Err(error) => operation_error.set(Some(server_error_message(error))),
            }
        });
    };

    let repository_snapshot = snapshot();
    let status_loading = repository_snapshot.is_none();
    let status_error = repository_snapshot
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(ToString::to_string);
    let repository_missing = repository_snapshot.as_ref().is_some_and(|result| {
        matches!(
            result,
            Ok(snapshot) if snapshot.state == RepositoryState::Uninitialized
        )
    });
    let repository = repository_snapshot
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(|snapshot| match &snapshot.state {
            RepositoryState::Ready(repository) => Some(repository),
            RepositoryState::Uninitialized => None,
        })
        .cloned()
        .unwrap_or_default();
    let selected_file_change = selected().and_then(|selection| {
        repository
            .changes
            .iter()
            .find(|change| change.path.as_str() == selection.path)
            .cloned()
    });
    let loaded_snapshot = repository_snapshot
        .as_ref()
        .and_then(|result| result.as_ref().ok());
    let branch_list = loaded_snapshot
        .and_then(|snapshot| snapshot.branches.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let history_loading = repository_snapshot.is_none();
    let history_error = loaded_snapshot
        .and_then(|snapshot| snapshot.history.as_ref().err())
        .map(ToString::to_string)
        .or_else(|| status_error.clone());
    let commit_list = loaded_snapshot
        .and_then(|snapshot| snapshot.history.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let tag_list = loaded_snapshot
        .and_then(|snapshot| snapshot.tags.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let remotes_loading = repository_snapshot.is_none();
    let remote_list = loaded_snapshot
        .and_then(|snapshot| snapshot.remotes.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let selected_history_commit = selected_commit()
        .and_then(|oid| commit_list.iter().find(|commit| commit.oid == oid).cloned());
    let branch = repository.branch.head.as_deref().unwrap_or("Detached HEAD");
    let upstream = repository
        .branch
        .upstream
        .as_deref()
        .unwrap_or("No upstream");
    let commits_to_pull = repository.branch.behind;
    let commits_to_push = repository.branch.ahead;
    let diff_loading = diff.state() == UseResourceState::Pending;
    let conflict_loading = conflict.state() == UseResourceState::Pending;
    let commit_detail_loading = commit_detail.state() == UseResourceState::Pending;

    rsx! {
        if status_loading {
            div {
                class: "flex size-full items-center justify-center gap-2 bg-card text-sm text-muted-foreground",
                role: "status",
                span {
                    class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                    aria_hidden: "true",
                }
                "Loading repository status…"
            }
        } else if let Some(status_error) = status_error {
            div { class: "flex size-full flex-col items-center justify-center gap-3 bg-card p-6 text-center",
                strong { class: "text-sm text-destructive", "Could not load repository status" }
                p { class: "max-w-lg text-xs text-muted-foreground", "{status_error}" }
                Button {
                    label: "Try again",
                    kind: ButtonKind::Ghost,
                    onclick: move |_| *refresh_key.write() += 1,
                }
            }
        } else if repository_missing {
            RepositoryWelcome { pending: pending(), on_initialize }
        } else {
            div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[310px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
                if sidebar_open() {
                    aside { class: "min-h-0 min-w-0 border-r border-border bg-sidebar max-md:hidden",
                        GitSidebar {
                            repository: repository.clone(),
                            view,
                            commits: commit_list.clone(),
                            history_loading,
                            history_error: history_error.clone(),
                            selected_commit,
                            selected,
                            pending: pending(),
                            on_select: move |()| {},
                            on_history_action,
                            on_mutation,
                        }
                    }
                }
                section { class: "flex min-h-0 min-w-0 flex-col overflow-hidden max-md:h-full",
                    PanelHeader { kind: PanelHeaderKind::Repository,
                        div { class: "flex min-w-0 flex-1 items-center gap-1.5",
                            div { class: "shrink-0 max-md:hidden",
                                IconButton {
                                    label: if sidebar_open() { "Hide Git sidebar" } else { "Show Git sidebar" },
                                    icon: AppIcon::Explorer,
                                    pressed: sidebar_open(),
                                    onclick: move |_| sidebar_open.toggle(),
                                }
                            }
                            div { class: "hidden shrink-0 max-md:block",
                                IconButton {
                                    label: "Open Git sidebar",
                                    icon: AppIcon::Explorer,
                                    onclick: move |_| drawer.set(true),
                                }
                            }
                            div { class: "flex min-w-0 flex-1 items-center gap-1",
                                BranchWorktreeMenu {
                                    branches: branch_list.clone(),
                                    current_branch: branch.to_owned(),
                                    pending: pending(),
                                    repository_revision: refresh_key,
                                    on_action: move |action| {
                                        match action {
                                            BranchWorktreeAction::Switch(name) => {
                                                on_repository_action.call(RepositoryAction::SwitchBranch(name));
                                            }
                                            BranchWorktreeAction::Compare(name) => {
                                                operation_error.set(None);
                                                comparison.set(None);
                                                compare_target.set(Some(name));
                                                dialog.set(GitDialog::CompareMerge);
                                            }
                                            BranchWorktreeAction::NewBranch(name) => {
                                                branch_dialog_target.set(None);
                                                branch_start_point.set(Some(name));
                                                dialog.set(GitDialog::CreateBranch);
                                            }
                                            BranchWorktreeAction::Tags(name) => {
                                                tag_target.set(Some(name));
                                                dialog.set(GitDialog::Tags);
                                            }
                                            BranchWorktreeAction::Delete(name) => {
                                                branch_dialog_target.set(Some(name));
                                                dialog.set(GitDialog::DeleteBranch);
                                            }
                                        }
                                    },
                                }
                                DropdownMenu {
                                    open: branch_menu(),
                                    on_open_change: move |open: bool| branch_menu.set(open),
                                    div { class: "relative",
                                        MenuTrigger {
                                            label: "Branch actions",
                                            icon: AppIcon::MoreVertical,
                                            open: branch_menu(),
                                            size: ControlSize::Small,
                                            on_toggle: move |()| branch_menu.toggle(),
                                        }
                                        MenuContent { class: "left-0 w-46",
                                            DropdownMenuItem::<GitDialog> {
                                                value: GitDialog::CompareMerge,
                                                index: 0_usize,
                                                disabled: pending() || repository.branch.head.is_none() || branch_list.len() < 2,
                                                on_select: move |_| {
                                                    operation_error.set(None);
                                                    comparison.set(None);
                                                    compare_target.set(None);
                                                    dialog.set(GitDialog::CompareMerge);
                                                },
                                                "Compare branch"
                                            }
                                            DropdownMenuItem::<GitDialog> {
                                                value: GitDialog::CreateBranch,
                                                index: 1_usize,
                                                disabled: pending(),
                                                on_select: move |_| {
                                                    branch_dialog_target.set(None);
                                                    branch_start_point.set(None);
                                                    dialog.set(GitDialog::CreateBranch);
                                                },
                                                "New branch"
                                            }
                                            DropdownMenuItem::<GitDialog> {
                                                value: GitDialog::RenameBranch,
                                                index: 2_usize,
                                                disabled: pending() || repository.branch.head.is_none(),
                                                on_select: move |_| dialog.set(GitDialog::RenameBranch),
                                                "Rename branch"
                                            }
                                            DropdownMenuItem::<GitDialog> {
                                                value: GitDialog::Tags,
                                                index: 3_usize,
                                                disabled: pending(),
                                                on_select: move |_| {
                                                    operation_error.set(None);
                                                    tag_target.set(None);
                                                    dialog.set(GitDialog::Tags);
                                                },
                                                "Tags ({tag_list.len()})"
                                            }
                                            hr {}
                                            DropdownMenuItem::<GitDialog> {
                                                class: "!text-destructive",
                                                value: GitDialog::DiscardAll,
                                                index: 4_usize,
                                                disabled: pending() || repository.changes.is_empty(),
                                                on_select: move |_| dialog.set(GitDialog::DiscardAll),
                                                "Discard all changes"
                                            }
                                        }
                                    }
                                }
                            }
                            if !repository.changes.is_empty() {
                                span { class: "truncate text-[11px] text-muted-foreground max-lg:hidden @max-[780px]:hidden",
                                    {
                                        format!(
                                            "{} {} changed",
                                            repository.changes.len(),
                                            if repository.changes.len() == 1 { "file" } else { "files" },
                                        )
                                    }
                                }
                            }
                            if repository.conflict_count() > 0 {
                                span { class: "text-[11px] text-destructive @max-[520px]:hidden",
                                    {format!("{} conflicts", repository.conflict_count())}
                                }
                            }
                            div { class: "min-w-0 max-[520px]:hidden @max-[640px]:hidden",
                                RemoteManager {
                                    remotes: remote_list.clone(),
                                    upstream: upstream.to_owned(),
                                    loading: remotes_loading,
                                    pending: pending(),
                                    on_add: move |()| {
                                        remote_target.set(None);
                                        operation_error.set(None);
                                        dialog.set(GitDialog::AddRemote);
                                    },
                                    on_edit: move |remote| {
                                        remote_target.set(Some(remote));
                                        operation_error.set(None);
                                        dialog.set(GitDialog::EditRemote);
                                    },
                                    on_remove: move |remote| {
                                        remote_target.set(Some(remote));
                                        operation_error.set(None);
                                        dialog.set(GitDialog::RemoveRemote);
                                    },
                                    on_fetch: move |name| {
                                        on_repository_action.call(RepositoryAction::FetchRemote(name));
                                    },
                                }
                            }
                        }
                        div { class: "git-toolbar flex shrink-0 items-center gap-1",
                            if pending() {
                                span {
                                    class: "flex items-center gap-1.5 px-1 text-[10px] text-muted-foreground @max-[640px]:hidden",
                                    role: "status",
                                    span {
                                        class: "size-3 animate-spin rounded-full border-2 border-border border-t-primary",
                                        aria_hidden: "true",
                                    }
                                    if refreshing() {
                                        "Refreshing…"
                                    } else {
                                        "Working…"
                                    }
                                }
                            }
                            if repository.conflict_count() > 0 {
                                button {
                                    class: "h-7 rounded-md bg-destructive/10 px-2 text-[11px] text-destructive hover:bg-destructive/20 @max-[520px]:hidden",
                                    disabled: pending(),
                                    onclick: move |_| {
                                        operation_error.set(None);
                                        dialog.set(GitDialog::AbortMerge);
                                    },
                                    "Abort merge"
                                }
                            }
                            button {
                                class: "inline-flex h-7 items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50",
                                title: "Commit staged changes",
                                "aria-label": "Commit staged changes",
                                disabled: pending() || repository.staged_count() == 0,
                                onclick: move |_| {
                                    operation_error.set(None);
                                    dialog.set(GitDialog::Commit);
                                },
                                Icon { icon: AppIcon::Commit, size: 14 }
                                span { "Commit" }
                            }
                            GitSyncButton {
                                current_branch: repository.branch.head.clone(),
                                remotes: remote_list.clone(),
                                has_upstream: repository.branch.upstream.is_some(),
                                ahead: commits_to_push,
                                behind: commits_to_pull,
                                conflicts: repository.conflict_count(),
                                pending: pending(),
                                refreshing: refreshing(),
                                on_action: move |action| match action {
                                    GitSyncAction::AddRemote => {
                                        operation_error.set(None);
                                        dialog.set(GitDialog::AddRemote);
                                    }
                                    GitSyncAction::Publish(remote) => {
                                        on_repository_action.call(RepositoryAction::Publish(remote));
                                    }
                                    GitSyncAction::Pull => {
                                        on_repository_action.call(RepositoryAction::Pull);
                                    }
                                    GitSyncAction::Push => {
                                        on_repository_action
                                            .call(RepositoryAction::Push {
                                                force_with_lease: false,
                                            });
                                    }
                                    GitSyncAction::Fetch => {
                                        on_repository_action.call(RepositoryAction::Refresh);
                                    }
                                    GitSyncAction::AbortMerge => {
                                        operation_error.set(None);
                                        dialog.set(GitDialog::AbortMerge);
                                    }
                                },
                            }
                        }
                    }
                    div { class: "touch-scroll-region min-h-0 min-w-0 flex-1 touch-auto overflow-auto bg-background",
                        if view() == SidebarView::History {
                            HistoryDetail {
                                detail: if commit_detail_loading { None } else { commit_detail().flatten() },
                                selected: selected_commit().is_some(),
                                pending: pending(),
                                on_checkout: move |_| {
                                    operation_error.set(None);
                                    dialog.set(GitDialog::CheckoutCommit);
                                },
                                on_revert: move |_| {
                                    operation_error.set(None);
                                    dialog.set(GitDialog::RevertCommit);
                                },
                            }
                        } else {
                            ChangeDetail {
                                selection: selected(),
                                change: selected_file_change,
                                diff: if diff_loading { None } else { diff().flatten() },
                                conflict: if conflict_loading { None } else { conflict().flatten() },
                                expanded: expanded_diff(),
                                pending: pending(),
                                on_expand: move |()| expanded_diff.toggle(),
                                on_mutation,
                            }
                        }
                    }
                }
                if drawer() && !drawer_blocked {
                    Drawer {
                        title: "Repository changes",
                        label: "Git repository sidebar",
                        content_class: "h-full w-[min(330px,88vw)] justify-self-start border-0 border-r border-border bg-sidebar shadow-[15px_0_50px_#0008]",
                        restore_focus: "button[aria-label='Open Git sidebar']",
                        on_close: move |()| drawer.set(false),
                        GitSidebar {
                            repository: repository.clone(),
                            view,
                            commits: commit_list.clone(),
                            history_loading,
                            history_error: history_error.clone(),
                            selected_commit,
                            selected,
                            pending: pending(),
                            on_select: move |()| drawer.set(false),
                            on_history_action,
                            on_mutation,
                        }
                    }
                }
            }
        }

        if dialog() == GitDialog::Commit {
            CommitDialog {
                workspace_slug: slug.clone(),
                initial_message: retry_commit().map(|request| request.message).unwrap_or_default(),
                pending: pending(),
                error: operation_error(),
                on_close: move |()| {
                    if !pending() {
                        dialog.set(GitDialog::None);
                        operation_error.set(None);
                    }
                },
                on_submit: on_commit,
            }
        }
        if dialog() == GitDialog::SigningRetry {
            SigningDialog {
                pending: pending(),
                error: operation_error(),
                on_close: move |()| {
                    if !pending() {
                        dialog.set(GitDialog::None);
                        retry_commit.set(None);
                        operation_error.set(None);
                    }
                },
                on_submit: on_signing_retry,
            }
        }
        if matches!(
            dialog(),
            GitDialog::CreateBranch | GitDialog::RenameBranch | GitDialog::DeleteBranch
        )
        {
            BranchDialog {
                action: dialog(),
                current_branch: repository.branch.head.clone().unwrap_or_default(),
                branches: branch_list.clone(),
                initial_name: branch_dialog_target(),
                start_point: branch_start_point(),
                pending: pending(),
                error: operation_error(),
                on_close: move |()| {
                    if !pending() {
                        dialog.set(GitDialog::None);
                        branch_dialog_target.set(None);
                        branch_start_point.set(None);
                        operation_error.set(None);
                    }
                },
                on_submit: move |name| match dialog() {
                    GitDialog::CreateBranch => {
                        on_repository_action
                            .call(
                                RepositoryAction::CreateBranch(
                                    branch_request(name, branch_start_point()),
                                ),
                            );
                    }
                    GitDialog::RenameBranch => {
                        on_repository_action.call(RepositoryAction::RenameBranch(name));
                    }
                    GitDialog::DeleteBranch => {
                        on_repository_action.call(RepositoryAction::DeleteBranch(name));
                    }
                    _ => {}
                },
            }
        }
        if dialog() == GitDialog::DiscardAll {
            DiscardAllDialog {
                changed_files: repository.changes.len(),
                pending: pending(),
                error: operation_error(),
                on_close: move |()| {
                    if !pending() {
                        dialog.set(GitDialog::None);
                        operation_error.set(None);
                    }
                },
                on_confirm: move |()| {
                    on_mutation
                        .call(
                            Mutation::DiscardAll(
                                repository
                                    .changes
                                    .iter()
                                    .map(|change| change.path.as_str().to_owned())
                                    .collect(),
                            ),
                        );
                },
            }
        }
        if matches!(dialog(), GitDialog::AddRemote | GitDialog::EditRemote) {
            RemoteDialog {
                remote: remote_target(),
                pending: pending(),
                error: operation_error(),
                on_close: move |()| {
                    if !pending() {
                        dialog.set(GitDialog::None);
                        remote_target.set(None);
                        operation_error.set(None);
                    }
                },
                on_submit: move |request| {
                    if let Some(previous) = remote_target() {
                        on_repository_action
                            .call(RepositoryAction::UpdateRemote {
                                previous_name: previous.name,
                                request,
                            });
                    } else {
                        on_repository_action.call(RepositoryAction::AddRemote(request));
                    }
                },
            }
        }
        if dialog() == GitDialog::RemoveRemote {
            if let Some(remote) = remote_target() {
                RemoveRemoteDialog {
                    remote: remote.clone(),
                    pending: pending(),
                    error: operation_error(),
                    on_close: move |()| {
                        if !pending() {
                            dialog.set(GitDialog::None);
                            remote_target.set(None);
                            operation_error.set(None);
                        }
                    },
                    on_confirm: move |()| {
                        on_repository_action
                            .call(RepositoryAction::RemoveRemote(remote.name.clone()));
                    },
                }
            }
        }
        if dialog() == GitDialog::ForcePush {
            ForcePushDialog {
                pending: pending(),
                error: operation_error(),
                on_close: move |()| {
                    if !pending() {
                        dialog.set(GitDialog::None);
                        operation_error.set(None);
                    }
                },
                on_confirm: move |()| {
                    on_repository_action
                        .call(RepositoryAction::Push {
                            force_with_lease: true,
                        });
                },
            }
        }
        if dialog() == GitDialog::Tags {
            TagDialog {
                tags: tag_list.clone(),
                target: tag_target(),
                pending: pending(),
                error: operation_error(),
                on_close: move |()| {
                    if !pending() {
                        dialog.set(GitDialog::None);
                        tag_target.set(None);
                        operation_error.set(None);
                    }
                },
                on_create: move |request| {
                    on_repository_action.call(RepositoryAction::CreateTag(request));
                },
                on_delete: move |name| {
                    on_repository_action.call(RepositoryAction::DeleteTag(name));
                },
            }
        }
        if dialog() == GitDialog::CompareMerge {
            CompareMergeDialog {
                current_branch: repository.branch.head.clone().unwrap_or_default(),
                branches: branch_list.clone(),
                initial_target: compare_target(),
                comparison: comparison(),
                pending: pending(),
                error: operation_error(),
                on_close: move |()| {
                    if !pending() {
                        dialog.set(GitDialog::None);
                        comparison.set(None);
                        compare_target.set(None);
                        operation_error.set(None);
                    }
                },
                on_compare: move |target| {
                    if let Some(base) = repository.branch.head.clone() {
                        on_compare.call((base, target));
                    }
                },
                on_merge: move |target| {
                    on_repository_action.call(RepositoryAction::Merge(target));
                },
            }
        }
        if dialog() == GitDialog::AbortMerge {
            AbortMergeDialog {
                pending: pending(),
                error: operation_error(),
                on_close: move |()| {
                    if !pending() {
                        dialog.set(GitDialog::None);
                        operation_error.set(None);
                    }
                },
                on_confirm: move |()| {
                    on_repository_action.call(RepositoryAction::AbortMerge);
                },
            }
        }
        if matches!(dialog(), GitDialog::CheckoutCommit | GitDialog::RevertCommit) {
            if let Some(commit) = selected_history_commit {
                CommitHistoryActionDialog {
                    action: dialog(),
                    commit: commit.clone(),
                    pending: pending(),
                    error: operation_error(),
                    on_close: move |()| {
                        if !pending() {
                            dialog.set(GitDialog::None);
                            operation_error.set(None);
                        }
                    },
                    on_confirm: move |()| {
                        let action = if dialog() == GitDialog::CheckoutCommit {
                            RepositoryAction::CheckoutCommit(commit.oid.clone())
                        } else {
                            RepositoryAction::RevertCommit(commit.oid.clone())
                        };
                        on_repository_action.call(action);
                    },
                }
            }
        }
        if let Some((message, tone)) = toast() {
            Toast { message, tone, on_close: move |()| toast.set(None) }
        }
    }
}
