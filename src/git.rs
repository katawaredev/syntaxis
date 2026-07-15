use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem, DropdownMenuTrigger};
use syntaxis_git::{
    parse_diff_hunks, BranchComparison, BranchInfo, BranchRequest, ChangeKind, CommitDetail,
    CommitInfo, CommitOutcome, CommitRequest, ConflictChoice, ConflictFile, DiffKind, FileChange,
    HunkAction, MergeOutcome, PushOutcome, RemoteInfo, RemoteRequest, RepositoryStatus, TagInfo,
    TagRequest, UnifiedDiff,
};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, Checkbox, ControlSize, DialogActions, DialogForm, Drawer, Field,
    Icon, IconButton, MenuContent, MenuTrigger, Modal, PanelHeader, PanelHeaderKind, TextArea,
    TextInput, TextInputType, Toast, Tone,
};

pub(crate) mod api;

mod changes;
mod dialogs;
mod history;
mod remotes;

use changes::{ChangeDetail, GitSidebar, RawPatch};
use dialogs::{
    AbortMergeDialog, BranchDialog, CommitDialog, CommitHistoryActionDialog, CompareMergeDialog,
    DiscardAllDialog, ForcePushDialog, RemoteDialog, RemoveRemoteDialog, SigningDialog, TagDialog,
};
use history::HistoryDetail;
use remotes::RemoteManager;

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectedChange {
    path: String,
    kind: DiffKind,
    conflicted: bool,
}

#[derive(Clone)]
enum Mutation {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    Discard(Vec<String>),
    DiscardAll(Vec<String>),
    Hunk {
        path: String,
        kind: DiffKind,
        index: usize,
        fingerprint: u64,
        action: HunkAction,
    },
    ResolveConflict {
        path: String,
        index: usize,
        fingerprint: u64,
        choice: ConflictChoice,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum GitDialog {
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

#[derive(Clone)]
enum RepositoryAction {
    SwitchBranch(String),
    CreateBranch(BranchRequest),
    RenameBranch(String),
    DeleteBranch(String),
    CreateTag(TagRequest),
    DeleteTag(String),
    CheckoutCommit(String),
    RevertCommit(String),
    Merge(String),
    AbortMerge,
    Fetch,
    FetchRemote(String),
    AddRemote(RemoteRequest),
    UpdateRemote {
        previous_name: String,
        request: RemoteRequest,
    },
    RemoveRemote(String),
    Push {
        force_with_lease: bool,
    },
}

#[component]
pub fn Git(slug: String) -> Element {
    let mut refresh_key = use_signal(|| 0_u64);
    let status_slug = slug.clone();
    let status = use_resource(move || {
        let slug = status_slug.clone();
        let _ = refresh_key();
        async move { api::repository_status(slug).await }
    });
    let branches_slug = slug.clone();
    let branches = use_resource(move || {
        let slug = branches_slug.clone();
        let _ = refresh_key();
        async move { api::branches(slug).await }
    });
    let remotes_slug = slug.clone();
    let remotes = use_resource(move || {
        let slug = remotes_slug.clone();
        let _ = refresh_key();
        async move { api::remotes(slug).await }
    });
    let tags_slug = slug.clone();
    let tags = use_resource(move || {
        let slug = tags_slug.clone();
        let _ = refresh_key();
        async move { api::tags(slug).await }
    });
    let history_slug = slug.clone();
    let history = use_resource(move || {
        let slug = history_slug.clone();
        let _ = refresh_key();
        async move { api::history(slug, 100).await }
    });
    let mut selected = use_signal(|| None::<SelectedChange>);
    let mut expanded_diff = use_signal(|| false);
    let view = use_signal(SidebarView::default);
    let selected_commit = use_signal(|| None::<String>);
    let diff_slug = slug.clone();
    let diff = use_resource(move || {
        let slug = diff_slug.clone();
        let _ = refresh_key();
        let selection = selected();
        let expanded = expanded_diff();
        async move {
            if let Some(selection) = selection {
                Some(api::repository_diff(slug, selection.path, selection.kind, expanded).await)
            } else {
                None
            }
        }
    });
    let conflict_slug = slug.clone();
    let conflict = use_resource(move || {
        let slug = conflict_slug.clone();
        let _ = refresh_key();
        let selection = selected();
        async move {
            if let Some(selection) = selection.filter(|selection| selection.conflicted) {
                Some(api::conflict_file(slug, selection.path).await)
            } else {
                None
            }
        }
    });
    let detail_slug = slug.clone();
    let commit_detail = use_resource(move || {
        let slug = detail_slug.clone();
        let revision = selected_commit();
        async move {
            if let Some(revision) = revision {
                Some(api::commit_detail(slug, revision).await)
            } else {
                None
            }
        }
    });
    let mut drawer = use_signal(|| false);
    let mut sidebar_open = use_signal(|| true);
    let mut branch_selector_menu = use_signal(|| false);
    let mut branch_options = use_signal(|| None::<String>);
    let mut branch_dialog_target = use_signal(|| None::<String>);
    let mut branch_start_point = use_signal(|| None::<String>);
    let mut tag_target = use_signal(|| None::<String>);
    let mut branch_menu = use_signal(|| false);
    let mut remote_target = use_signal(|| None::<RemoteInfo>);
    let mut pending = use_signal(|| false);
    let mut dialog = use_signal(GitDialog::default);
    let mut toast = use_signal(|| None::<(String, Tone)>);
    let mut operation_error = use_signal(|| None::<String>);
    let mut retry_commit = use_signal(|| None::<CommitRequest>);
    let mut comparison = use_signal(|| None::<BranchComparison>);
    let mut compare_target = use_signal(|| None::<String>);

    use_effect(move || {
        let _ = selected();
        expanded_diff.set(false);
    });

    use_effect(move || {
        if let Some(error) = operation_error() {
            toast.set(Some((error, Tone::Destructive)));
        }
    });

    use_effect(move || {
        if let Some(Err(error)) = status() {
            toast.set(Some((server_error_message(error), Tone::Destructive)));
        }
    });

    use_effect(move || {
        if let Some(Err(error)) = remotes() {
            toast.set(Some((server_error_message(error), Tone::Destructive)));
        }
    });

    let mutation_slug = slug.clone();
    let on_mutation = EventHandler::new(move |mutation: Mutation| {
        let slug = mutation_slug.clone();
        let closes_dialog = matches!(&mutation, Mutation::DiscardAll(_));
        let show_success_toast = !matches!(
            &mutation,
            Mutation::Stage(_)
                | Mutation::Unstage(_)
                | Mutation::Hunk {
                    action: HunkAction::Stage | HunkAction::Unstage,
                    ..
                }
        );
        let selection_after = match &mutation {
            Mutation::Hunk { path, kind, .. } => Some(SelectedChange {
                path: path.clone(),
                kind: *kind,
                conflicted: false,
            }),
            _ => None,
        };
        pending.set(true);
        operation_error.set(None);
        spawn(async move {
            let (action, result) = match mutation {
                Mutation::Stage(paths) => ("Staged changes", api::stage_paths(slug, paths).await),
                Mutation::Unstage(paths) => {
                    ("Unstaged changes", api::unstage_paths(slug, paths).await)
                }
                Mutation::Discard(paths) => {
                    ("Discarded changes", api::discard_paths(slug, paths).await)
                }
                Mutation::DiscardAll(paths) => {
                    let result = match api::unstage_paths(slug.clone(), paths.clone()).await {
                        Ok(()) => api::discard_paths(slug, paths).await,
                        Err(error) => Err(error),
                    };
                    ("Discarded all changes", result)
                }
                Mutation::Hunk {
                    path,
                    kind,
                    index,
                    fingerprint,
                    action,
                } => (
                    match action {
                        HunkAction::Stage => "Staged hunk",
                        HunkAction::Unstage => "Unstaged hunk",
                        HunkAction::Discard => "Discarded hunk",
                    },
                    api::apply_hunk(slug, path, kind, index, fingerprint, action).await,
                ),
                Mutation::ResolveConflict {
                    path,
                    index,
                    fingerprint,
                    choice,
                } => (
                    match choice {
                        ConflictChoice::Current => "Kept current conflict block",
                        ConflictChoice::Incoming => "Accepted incoming conflict block",
                        ConflictChoice::Both => "Merged both conflict blocks",
                    },
                    api::resolve_conflict(slug, path, index, fingerprint, choice)
                        .await
                        .map(|_| ()),
                ),
            };
            pending.set(false);
            match result {
                Ok(()) => {
                    selected.set(selection_after);
                    if closes_dialog {
                        dialog.set(GitDialog::None);
                    }
                    *refresh_key.write() += 1;
                    if show_success_toast {
                        toast.set(Some((action.into(), Tone::Success)));
                    }
                }
                Err(error) => operation_error.set(Some(server_error_message(error))),
            }
        });
    });

    let action_slug = slug.clone();
    let on_repository_action = EventHandler::new(move |action: RepositoryAction| {
        let slug = action_slug.clone();
        pending.set(true);
        operation_error.set(None);
        spawn(async move {
            let result = match action {
                RepositoryAction::SwitchBranch(name) => api::switch_branch(slug, name)
                    .await
                    .map(|()| "Switched branch".to_owned()),
                RepositoryAction::CreateBranch(request) => api::create_branch(slug, request)
                    .await
                    .map(|()| "Created and switched branch".to_owned()),
                RepositoryAction::RenameBranch(name) => api::rename_branch(slug, name)
                    .await
                    .map(|()| "Renamed branch".to_owned()),
                RepositoryAction::DeleteBranch(name) => api::delete_branch(slug, name, false)
                    .await
                    .map(|()| "Deleted branch".to_owned()),
                RepositoryAction::CreateTag(request) => api::create_tag(slug, request)
                    .await
                    .map(|()| "Created tag".to_owned()),
                RepositoryAction::DeleteTag(name) => api::delete_tag(slug, name)
                    .await
                    .map(|()| "Deleted tag".to_owned()),
                RepositoryAction::CheckoutCommit(revision) => api::checkout_commit(slug, revision)
                    .await
                    .map(|()| "Checked out commit in detached HEAD mode".to_owned()),
                RepositoryAction::RevertCommit(revision) => api::revert_commit(slug, revision)
                    .await
                    .map(|()| "Created revert commit".to_owned()),
                RepositoryAction::Merge(branch) => match api::merge(slug, branch).await {
                    Ok(MergeOutcome::Merged { message }) => Ok(message),
                    Ok(MergeOutcome::Conflicts { paths }) => {
                        pending.set(false);
                        dialog.set(GitDialog::None);
                        *refresh_key.write() += 1;
                        operation_error.set(Some(format!(
                            "Merge stopped with conflicts in {} file(s). Resolve the highlighted files or abort the merge.",
                            paths.len()
                        )));
                        return;
                    }
                    Err(error) => Err(error),
                },
                RepositoryAction::AbortMerge => api::abort_merge(slug)
                    .await
                    .map(|()| "Aborted merge".to_owned()),
                RepositoryAction::Fetch => api::fetch(slug).await.map(|result| result.message),
                RepositoryAction::FetchRemote(name) => api::fetch_remote(slug, name)
                    .await
                    .map(|result| result.message),
                RepositoryAction::AddRemote(request) => api::add_remote(slug, request)
                    .await
                    .map(|()| "Added remote".to_owned()),
                RepositoryAction::UpdateRemote {
                    previous_name,
                    request,
                } => api::update_remote(slug, previous_name, request)
                    .await
                    .map(|()| "Updated remote".to_owned()),
                RepositoryAction::RemoveRemote(name) => api::remove_remote(slug, name)
                    .await
                    .map(|()| "Removed remote".to_owned()),
                RepositoryAction::Push { force_with_lease } => {
                    match api::push(slug, force_with_lease).await {
                        Ok(PushOutcome::Pushed { result }) => Ok(result.message),
                        Ok(PushOutcome::ForceWithLeaseRequired { message }) => {
                            operation_error.set(Some(message));
                            dialog.set(GitDialog::ForcePush);
                            pending.set(false);
                            return;
                        }
                        Err(error) => Err(error),
                    }
                }
            };
            pending.set(false);
            match result {
                Ok(message) => {
                    dialog.set(GitDialog::None);
                    selected.set(None);
                    *refresh_key.write() += 1;
                    toast.set(Some((message, Tone::Success)));
                }
                Err(error) => operation_error.set(Some(server_error_message(error))),
            }
        });
    });

    let compare_slug = slug.clone();
    let on_compare = EventHandler::new(move |(base, head): (String, String)| {
        let slug = compare_slug.clone();
        pending.set(true);
        operation_error.set(None);
        comparison.set(None);
        spawn(async move {
            match api::compare(slug, base, head).await {
                Ok(result) => comparison.set(Some(result)),
                Err(error) => operation_error.set(Some(server_error_message(error))),
            }
            pending.set(false);
        });
    });

    let commit_slug = slug.clone();
    let on_commit = move |request: CommitRequest| {
        let slug = commit_slug.clone();
        let retry = CommitRequest {
            message: request.message.clone(),
            amend: request.amend,
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

    let status_snapshot = status();
    let repository = status_snapshot
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let branch_list = branches()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let commit_list = history()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let tag_list = tags()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let remotes_snapshot = remotes();
    let remotes_loading = remotes_snapshot.is_none();
    let remote_list = remotes_snapshot
        .as_ref()
        .and_then(|result| result.as_ref().ok())
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

    rsx! {
        div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[310px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open() {
                aside { class: "min-h-0 min-w-0 border-r border-border bg-sidebar max-md:hidden",
                    GitSidebar {
                        repository: repository.clone(),
                        view,
                        commits: commit_list.clone(),
                        selected_commit,
                        selected,
                        pending: pending(),
                        on_mutation,
                    }
                }
            }
            section { class: "flex min-h-0 min-w-0 flex-col overflow-hidden max-md:h-full",
                PanelHeader { kind: PanelHeaderKind::Repository,
                    div { class: "flex min-w-0 items-center gap-1.5",
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
                        div { class: "flex min-w-0 items-center gap-1",
                            DropdownMenu {
                                open: branch_selector_menu(),
                                on_open_change: move |open: bool| {
                                    branch_selector_menu.set(open);
                                    if !open {
                                        branch_options.set(None);
                                    }
                                },
                                div { class: "relative",
                                    DropdownMenuTrigger {
                                        class: "inline-flex h-7 max-w-48 items-center gap-1.5 rounded-md bg-transparent px-1.5 text-xs text-foreground hover:bg-accent disabled:opacity-50",
                                        aria_disabled: pending() || branch_list.is_empty(),
                                        "aria-label": "Switch branch",
                                        title: "Switch branch",
                                        Icon {
                                            icon: AppIcon::GitBranch,
                                            size: 13,
                                        }
                                        span { class: "truncate", "{branch}" }
                                    }
                                    MenuContent { class: "left-0 w-66",
                                        for item in branch_list.clone() {
                                            if !item.name.ends_with("/HEAD") && (!item.remote || item.name.contains('/')) {
                                                div { class: "rounded-md",
                                                    div { class: "flex min-w-0 items-center gap-1",
                                                        button {
                                                            class: if item.current { "flex min-h-8 min-w-0 flex-1 items-center gap-2 rounded-sm px-2 text-left text-xs text-foreground" } else { "flex min-h-8 min-w-0 flex-1 items-center gap-2 rounded-sm px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground" },
                                                            disabled: item.current || pending(),
                                                            onclick: {
                                                                let name = item.name.clone();
                                                                move |_| {
                                                                    branch_selector_menu.set(false);
                                                                    branch_options.set(None);
                                                                    on_repository_action.call(RepositoryAction::SwitchBranch(name.clone()));
                                                                }
                                                            },
                                                            Icon {
                                                                icon: AppIcon::GitBranch,
                                                                size: 13,
                                                            }
                                                            span { class: "min-w-0 flex-1 truncate",
                                                                "{item.name}"
                                                            }
                                                            if item.remote {
                                                                span { class: "shrink-0 text-[9px] text-muted-foreground",
                                                                    "remote"
                                                                }
                                                            }
                                                        }
                                                        button {
                                                            class: "grid size-7 shrink-0 place-items-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground",
                                                            "aria-label": "Branch actions for {item.name}",
                                                            title: "Branch actions for {item.name}",
                                                            onclick: {
                                                                let name = item.name.clone();
                                                                move |event: MouseEvent| {
                                                                    event.stop_propagation();
                                                                    if branch_options().as_deref() == Some(name.as_str()) {
                                                                        branch_options.set(None);
                                                                    } else {
                                                                        branch_options.set(Some(name.clone()));
                                                                    }
                                                                }
                                                            },
                                                            Icon {
                                                                icon: AppIcon::MoreVertical,
                                                                size: 14,
                                                            }
                                                        }
                                                    }
                                                    if branch_options().as_deref() == Some(item.name.as_str()) {
                                                        div { class: "mx-1 mb-1 grid gap-0.5 border-l border-border pl-2",
                                                            button {
                                                                class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                                                disabled: item.current || pending(),
                                                                onclick: {
                                                                    let name = item.name.clone();
                                                                    move |_| {
                                                                        branch_selector_menu.set(false);
                                                                        branch_options.set(None);
                                                                        operation_error.set(None);
                                                                        comparison.set(None);
                                                                        compare_target.set(Some(name.clone()));
                                                                        dialog.set(GitDialog::CompareMerge);
                                                                    }
                                                                },
                                                                "Compare with current"
                                                            }
                                                            button {
                                                                class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                                                disabled: pending(),
                                                                onclick: {
                                                                    let name = item.name.clone();
                                                                    move |_| {
                                                                        branch_selector_menu.set(false);
                                                                        branch_options.set(None);
                                                                        branch_dialog_target.set(None);
                                                                        branch_start_point.set(Some(name.clone()));
                                                                        dialog.set(GitDialog::CreateBranch);
                                                                    }
                                                                },
                                                                "New branch from here"
                                                            }
                                                            button {
                                                                class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                                                disabled: pending(),
                                                                onclick: {
                                                                    let name = item.name.clone();
                                                                    move |_| {
                                                                        branch_selector_menu.set(false);
                                                                        branch_options.set(None);
                                                                        tag_target.set(Some(name.clone()));
                                                                        dialog.set(GitDialog::Tags);
                                                                    }
                                                                },
                                                                "Create tag here"
                                                            }
                                                            if !item.current && !item.remote {
                                                                button {
                                                                    class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-destructive hover:bg-destructive/10",
                                                                    disabled: pending(),
                                                                    onclick: {
                                                                        let name = item.name.clone();
                                                                        move |_| {
                                                                            branch_selector_menu.set(false);
                                                                            branch_options.set(None);
                                                                            branch_dialog_target.set(Some(name.clone()));
                                                                            dialog.set(GitDialog::DeleteBranch);
                                                                        }
                                                                    },
                                                                    "Delete branch"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
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
                            span { class: "truncate text-[11px] text-muted-foreground max-lg:hidden",
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
                            span { class: "text-[11px] text-destructive",
                                {format!("{} conflicts", repository.conflict_count())}
                            }
                        }
                        div { class: "min-w-0",
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
                    div { class: "flex shrink-0 items-center gap-1",
                        if repository.conflict_count() > 0 {
                            button {
                                class: "h-7 rounded-md bg-destructive/10 px-2 text-[11px] text-destructive hover:bg-destructive/20",
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
                            disabled: pending() || repository.staged_count() == 0,
                            onclick: move |_| {
                                operation_error.set(None);
                                dialog.set(GitDialog::Commit);
                            },
                            Icon { icon: AppIcon::Commit, size: 14 }
                            "Commit"
                        }
                        button {
                            class: "inline-flex h-7 items-center gap-1 rounded-md bg-transparent px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50 max-lg:px-1.5",
                            title: "Pull remote changes",
                            "aria-label": "Pull remote changes",
                            disabled: pending(),
                            onclick: move |_| on_repository_action.call(RepositoryAction::Fetch),
                            Icon { icon: AppIcon::Fetch, size: 14 }
                            span { class: "max-lg:hidden", "Pull" }
                        }
                        button {
                            class: "inline-flex h-7 items-center gap-1 rounded-md bg-transparent px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50 max-lg:px-1.5",
                            title: "Push commits",
                            "aria-label": "Push commits",
                            disabled: pending(),
                            onclick: move |_| {
                                on_repository_action
                                    .call(RepositoryAction::Push {
                                        force_with_lease: false,
                                    });
                            },
                            Icon { icon: AppIcon::Push, size: 14 }
                            span { class: "max-lg:hidden", "Push" }
                        }
                        button {
                            class: "inline-flex h-7 items-center gap-1 rounded-md bg-transparent px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50 max-lg:px-1.5",
                            title: "Refresh repository",
                            "aria-label": "Refresh repository",
                            disabled: pending(),
                            onclick: move |_| *refresh_key.write() += 1,
                            Icon { icon: AppIcon::Refresh, size: 14 }
                            span { class: "max-lg:hidden", "Refresh" }
                        }
                    }
                }
                div { class: "min-h-0 min-w-0 flex-1 overflow-auto bg-background",
                    if view() == SidebarView::History {
                        HistoryDetail {
                            detail: commit_detail().flatten(),
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
                            diff: diff().flatten(),
                            conflict: conflict().flatten(),
                            expanded: expanded_diff(),
                            pending: pending(),
                            on_expand: move |()| expanded_diff.toggle(),
                            on_mutation,
                        }
                    }
                }
            }
            if drawer() {
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
                        selected_commit,
                        selected,
                        pending: pending(),
                        on_mutation,
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

fn change_label(kind: Option<ChangeKind>) -> &'static str {
    match kind {
        Some(ChangeKind::Modified) => "M",
        Some(ChangeKind::TypeChanged) => "T",
        Some(ChangeKind::Added) => "A",
        Some(ChangeKind::Deleted) => "D",
        Some(ChangeKind::Renamed) => "R",
        Some(ChangeKind::Copied) => "C",
        Some(ChangeKind::Untracked) => "U",
        Some(ChangeKind::Unmerged) => "!",
        None => "",
    }
}

fn change_badge_class(kind: Option<ChangeKind>) -> &'static str {
    match kind {
        Some(ChangeKind::Added | ChangeKind::Untracked) => "border-emerald-400 text-emerald-400",
        Some(ChangeKind::Deleted | ChangeKind::Unmerged) => "border-red-400 text-red-400",
        Some(ChangeKind::Renamed | ChangeKind::Copied) => "border-sky-400 text-sky-400",
        Some(ChangeKind::Modified | ChangeKind::TypeChanged) => "border-amber-400 text-amber-400",
        None => "border-muted-foreground text-muted-foreground",
    }
}

fn diff_line_class(line: &str) -> &'static str {
    if line.starts_with('+') && !line.starts_with("+++") {
        "diff-line added"
    } else if line.starts_with('-') && !line.starts_with("---") {
        "diff-line removed"
    } else if line.starts_with("@@") {
        "diff-line bg-secondary text-primary"
    } else {
        "diff-line context"
    }
}

fn short_oid(oid: &str) -> &str {
    oid.get(..7).unwrap_or(oid)
}

fn server_error_message(error: ServerFnError) -> String {
    match error {
        ServerFnError::ServerError { message, .. } => message,
        other => other.to_string(),
    }
}

fn branch_request(name: String, start_point: Option<String>) -> BranchRequest {
    BranchRequest { name, start_point }
}

fn remote_request(name: String, fetch_url: String, push_url: Option<String>) -> RemoteRequest {
    RemoteRequest {
        name,
        fetch_url,
        push_url,
    }
}

fn display_remote_url(url: &str) -> String {
    if let Some((scheme, remainder)) = url.split_once("://") {
        let visible = remainder
            .split_once('@')
            .map_or(remainder, |(_, visible)| visible);
        return format!("{scheme}://{visible}");
    }
    if let Some((credentials, visible)) = url.split_once('@') {
        if !credentials.contains('/') {
            return visible.to_owned();
        }
    }
    url.to_owned()
}
