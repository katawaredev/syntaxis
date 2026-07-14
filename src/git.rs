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
                    toast.set(Some((action.into(), Tone::Success)));
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

#[component]
fn RemoteManager(
    remotes: Vec<RemoteInfo>,
    upstream: String,
    loading: bool,
    pending: bool,
    on_add: EventHandler<()>,
    on_edit: EventHandler<RemoteInfo>,
    on_remove: EventHandler<RemoteInfo>,
    on_fetch: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut options = use_signal(|| None::<String>);
    if loading {
        return rsx! {
            button {
                class: "inline-flex h-7 shrink-0 items-center rounded-md px-2 text-[11px] text-muted-foreground opacity-60",
                disabled: true,
                "Remotes…"
            }
        };
    }
    if remotes.is_empty() {
        return rsx! {
            button {
                class: "inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50",
                disabled: pending,
                onclick: move |_| on_add.call(()),
                Icon { icon: AppIcon::Plus, size: 12 }
                "Add remote"
            }
        };
    }
    let label = if upstream == "No upstream" {
        format!(
            "{} {}",
            remotes.len(),
            if remotes.len() == 1 {
                "remote"
            } else {
                "remotes"
            },
        )
    } else {
        upstream
    };
    rsx! {
        DropdownMenu {
            open: open(),
            on_open_change: move |next: bool| {
                open.set(next);
                if !next {
                    options.set(None);
                }
            },
            div { class: "relative",
                DropdownMenuTrigger {
                    class: "inline-flex h-7 max-w-44 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground",
                    "aria-label": "Manage Git remotes",
                    title: "Manage Git remotes",
                    span { class: "truncate", "{label}" }
                }
                MenuContent { class: "right-0 w-80",
                    div { class: "px-2 py-1 text-[9px] font-medium uppercase tracking-wider text-muted-foreground",
                        "Remotes"
                    }
                    for remote in remotes {
                        div { class: "rounded-md",
                            div { class: "px-1",
                                button {
                                    class: "flex w-full min-w-0 items-center gap-2 rounded-sm px-1 py-1.5 text-left text-muted-foreground hover:bg-accent hover:text-foreground",
                                    "aria-expanded": options().as_deref() == Some(remote.name.as_str()),
                                    "aria-label": "Show actions for remote {remote.name}",
                                    title: "Actions for remote {remote.name}",
                                    onclick: {
                                        let name = remote.name.clone();
                                        move |_| {
                                            if options().as_deref() == Some(name.as_str()) {
                                                options.set(None);
                                            } else {
                                                options.set(Some(name.clone()));
                                            }
                                        }
                                    },
                                    span { class: "min-w-0 flex-1",
                                        strong { class: "block truncate text-xs font-medium text-foreground",
                                            "{remote.name}"
                                        }
                                        small { class: "block truncate text-[9px] text-muted-foreground",
                                            {display_remote_url(&remote.fetch_url)}
                                        }
                                    }
                                    Icon { icon: AppIcon::MoreVertical, size: 14 }
                                }
                            }
                            if options().as_deref() == Some(remote.name.as_str()) {
                                div { class: "mx-1 mb-1 grid gap-0.5 border-l border-border pl-2",
                                    button {
                                        class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                        disabled: pending,
                                        onclick: {
                                            let name = remote.name.clone();
                                            move |_| {
                                                open.set(false);
                                                options.set(None);
                                                on_fetch.call(name.clone());
                                            }
                                        },
                                        "Fetch"
                                    }
                                    button {
                                        class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                        disabled: pending,
                                        onclick: {
                                            let remote = remote.clone();
                                            move |_| {
                                                open.set(false);
                                                options.set(None);
                                                on_edit.call(remote.clone());
                                            }
                                        },
                                        "Edit remote"
                                    }
                                    button {
                                        class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-destructive hover:bg-destructive/10",
                                        disabled: pending,
                                        onclick: {
                                            let remote = remote.clone();
                                            move |_| {
                                                open.set(false);
                                                options.set(None);
                                                on_remove.call(remote.clone());
                                            }
                                        },
                                        "Remove remote"
                                    }
                                }
                            }
                        }
                    }
                    hr {}
                    button {
                        class: "flex min-h-8 w-full items-center gap-2 rounded-sm px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                        disabled: pending,
                        onclick: move |_| {
                            open.set(false);
                            options.set(None);
                            on_add.call(());
                        },
                        Icon { icon: AppIcon::Plus, size: 13 }
                        "Add remote"
                    }
                }
            }
        }
    }
}

#[component]
fn GitSidebar(
    repository: RepositoryStatus,
    view: Signal<SidebarView>,
    commits: Vec<CommitInfo>,
    mut selected_commit: Signal<Option<String>>,
    selected: Signal<Option<SelectedChange>>,
    pending: bool,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    let conflicts = repository
        .changes
        .iter()
        .filter(|change| change.conflicted)
        .cloned()
        .collect::<Vec<_>>();
    let staged = repository
        .changes
        .iter()
        .filter(|change| change.is_staged() && !change.conflicted)
        .cloned()
        .collect::<Vec<_>>();
    let unstaged = repository
        .changes
        .iter()
        .filter(|change| change.is_unstaged() && !change.conflicted)
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        div { class: "flex h-full min-h-0 flex-col bg-background",
            div { class: "grid h-12 min-h-12 grid-cols-2 items-center gap-1 border-b border-border p-1.25",
                button {
                    class: if view() == SidebarView::Changes { "h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground" } else { "h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
                    onclick: move |_| view.set(SidebarView::Changes),
                    "Changes ({repository.changes.len()})"
                }
                button {
                    class: if view() == SidebarView::History { "h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground" } else { "h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
                    onclick: move |_| view.set(SidebarView::History),
                    "History"
                }
            }
            if view() == SidebarView::Changes {
                div { class: "min-h-0 flex-1 overflow-y-auto p-2",
                    if repository.changes.is_empty() {
                        div { class: "grid h-full min-h-40 place-items-center p-4 text-center text-xs text-muted-foreground",
                            "Working tree clean."
                        }
                    } else {
                        div { class: "space-y-3",
                            ChangeSection {
                                title: "Conflicts",
                                changes: conflicts,
                                kind: DiffKind::Worktree,
                                selected,
                                pending,
                                batch_label: None,
                                on_mutation,
                            }
                            ChangeSection {
                                title: "Staged",
                                changes: staged,
                                kind: DiffKind::Staged,
                                selected,
                                pending,
                                batch_label: Some("Unstage".into()),
                                on_mutation,
                            }
                            ChangeSection {
                                title: "Changes",
                                changes: unstaged,
                                kind: DiffKind::Worktree,
                                selected,
                                pending,
                                batch_label: Some("Stage".into()),
                                on_mutation,
                            }
                        }
                    }
                }
            } else {
                div { class: "min-h-0 flex-1 space-y-1 overflow-y-auto p-2",
                    for commit in commits {
                        button {
                            class: if selected_commit().as_deref() == Some(commit.oid.as_str()) { "flex w-full min-w-0 gap-2 rounded-md bg-muted p-2 text-left text-foreground" } else { "flex w-full min-w-0 gap-2 rounded-md p-2 text-left text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
                            onclick: {
                                let oid = commit.oid.clone();
                                move |_| selected_commit.set(Some(oid.clone()))
                            },
                            span { class: "mt-1.5 size-2 shrink-0 rounded-full border-2 border-primary" }
                            span { class: "min-w-0",
                                strong { class: "block truncate text-xs font-medium", "{commit.subject}" }
                                small { class: "mt-1 block truncate font-mono text-[10px] text-muted-foreground",
                                    "{commit.short_oid} · {commit.author_name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChangeSection(
    title: String,
    changes: Vec<FileChange>,
    kind: DiffKind,
    selected: Signal<Option<SelectedChange>>,
    pending: bool,
    batch_label: Option<String>,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    if changes.is_empty() {
        return rsx! {};
    }
    let batch_paths = changes
        .iter()
        .map(|change| change.path.as_str().to_owned())
        .collect::<Vec<_>>();
    rsx! {
        section {
            header { class: "mb-1 flex min-h-7 items-center justify-between px-1 text-xs font-medium text-muted-foreground",
                span { "{title} ({changes.len()})" }
                if let Some(label) = batch_label {
                    button {
                        class: "h-6 rounded-md border border-border bg-background px-2 text-[10px] text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50",
                        disabled: pending,
                        onclick: move |_| {
                            if kind == DiffKind::Staged {
                                on_mutation.call(Mutation::Unstage(batch_paths.clone()));
                            } else {
                                on_mutation.call(Mutation::Stage(batch_paths.clone()));
                            }
                        },
                        "{label} ({changes.len()})"
                    }
                }
            }
            div { class: "space-y-1",
                for change in changes {
                    ChangeRow { change, kind, selected }
                }
            }
        }
    }
}

#[component]
fn ChangeRow(
    change: FileChange,
    kind: DiffKind,
    mut selected: Signal<Option<SelectedChange>>,
) -> Element {
    let path = change.path.as_str().to_owned();
    let selection = SelectedChange {
        path: path.clone(),
        kind,
        conflicted: change.conflicted,
    };
    let active = selected().as_ref() == Some(&selection);
    let change_kind = if change.conflicted {
        Some(ChangeKind::Unmerged)
    } else if kind == DiffKind::Staged {
        change.index
    } else {
        change.worktree
    };
    let (additions, deletions) = if kind == DiffKind::Staged {
        (change.staged_additions, change.staged_deletions)
    } else {
        (change.unstaged_additions, change.unstaged_deletions)
    };
    let label = change_label(change_kind);
    rsx! {
        button {
            class: if active { "flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md bg-muted p-2 text-left text-xs text-foreground" } else { "flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md p-2 text-left text-xs text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
            onclick: move |_| selected.set(Some(selection.clone())),
            span { class: "grid size-4 shrink-0 place-items-center rounded-[5px] border text-[8px] font-bold {change_badge_class(change_kind)}",
                "{label}"
            }
            span { class: "min-w-0 flex-1 truncate", "{path}" }
            span { class: "shrink-0 text-[10px] text-emerald-400", "+{additions}" }
            span { class: "shrink-0 text-[10px] text-red-400", "−{deletions}" }
        }
    }
}

#[component]
fn ChangeDetail(
    selection: Option<SelectedChange>,
    diff: Option<Result<UnifiedDiff, ServerFnError>>,
    conflict: Option<Result<ConflictFile, ServerFnError>>,
    expanded: bool,
    pending: bool,
    on_expand: EventHandler<()>,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    let Some(selection) = selection else {
        return rsx! {
            div { class: "grid h-full min-h-60 place-items-center p-8 text-center text-sm text-muted-foreground",
                "Select a changed file to inspect its Git-generated diff."
            }
        };
    };
    if selection.conflicted {
        return rsx! {
            ConflictDetail {
                selection,
                conflict,
                pending,
                on_mutation,
            }
        };
    }
    let result = diff;
    rsx! {
        div { class: "working-diff min-h-full",
            div { class: "diff-titlebar",
                div {
                    span { class: "relative size-4 shrink-0 rounded-[5px] border-2 border-amber-400 text-amber-400 after:absolute after:top-1/2 after:left-1/2 after:size-1.5 after:-translate-1/2 after:rounded-full after:bg-current" }
                    strong { class: "truncate text-sm font-medium", "{selection.path}" }
                    button {
                        class: "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md border border-border bg-background px-2.5 text-xs text-muted-foreground hover:bg-muted hover:text-foreground",
                        title: if expanded { "Collapse diff context" } else { "Expand diff context" },
                        "aria-label": if expanded { "Collapse diff context" } else { "Expand diff context" },
                        onclick: move |_| on_expand.call(()),
                        Icon {
                            icon: if expanded { AppIcon::Collapse } else { AppIcon::Expand },
                            size: 13,
                        }
                        if expanded {
                            "Collapse"
                        } else {
                            "Expand"
                        }
                    }
                }
                div {
                    if selection.kind == DiffKind::Staged {
                        button {
                            class: "inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-background px-2.5 text-xs text-foreground hover:bg-muted disabled:opacity-50",
                            disabled: pending,
                            onclick: {
                                let path = selection.path.clone();
                                move |_| on_mutation.call(Mutation::Unstage(vec![path.clone()]))
                            },
                            "− Unstage file"
                        }
                    } else {
                        button {
                            class: "inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-background px-2.5 text-xs text-foreground hover:bg-muted disabled:opacity-50",
                            disabled: pending,
                            onclick: {
                                let path = selection.path.clone();
                                move |_| on_mutation.call(Mutation::Stage(vec![path.clone()]))
                            },
                            Icon { icon: AppIcon::Plus, size: 13 }
                            "Stage file"
                        }
                        button {
                            class: "inline-flex h-8 items-center gap-1.5 rounded-md bg-destructive/12 px-2.5 text-xs text-destructive hover:bg-destructive/20 disabled:opacity-50",
                            disabled: pending,
                            onclick: {
                                let path = selection.path.clone();
                                move |_| on_mutation.call(Mutation::Discard(vec![path.clone()]))
                            },
                            Icon { icon: AppIcon::Refresh, size: 13 }
                            "Discard"
                        }
                    }
                }
            }
            match result {
                None => rsx! {
                    div { class: "grid min-h-48 place-items-center text-xs text-muted-foreground", "Loading Git diff…" }
                },
                Some(Err(error)) => rsx! {
                    div { class: "m-4 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive",
                        "Could not load diff: {error}"
                    }
                },
                Some(Ok(diff)) if diff.binary => rsx! {
                    div { class: "grid min-h-48 place-items-center p-8 text-center text-xs text-muted-foreground",
                        "This file has a binary Git diff and cannot be displayed as text."
                    }
                },
                Some(Ok(diff)) if diff.patch.is_empty() => rsx! {
                    div { class: "grid min-h-48 place-items-center text-xs text-muted-foreground",
                        "Git reported no textual changes for this file."
                    }
                },
                Some(Ok(diff)) => rsx! {
                    if expanded {
                        RawPatch { patch: diff.patch }
                    } else {
                        HunkDiff {
                            diff,
                            selection,
                            pending,
                            on_mutation,
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn ConflictDetail(
    selection: SelectedChange,
    conflict: Option<Result<ConflictFile, ServerFnError>>,
    pending: bool,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    rsx! {
        div { class: "min-h-full",
            div { class: "diff-titlebar",
                div {
                    span { class: "relative size-4 shrink-0 rounded-[5px] border-2 border-red-400 text-red-400 after:absolute after:top-1/2 after:left-1/2 after:size-1.5 after:-translate-1/2 after:rounded-full after:bg-current" }
                    strong { class: "truncate text-sm font-medium", "{selection.path}" }
                }
                span { class: "text-xs text-muted-foreground", "Resolve every block to stage the file" }
            }
            match conflict {
                None => rsx! {
                    div { class: "grid min-h-48 place-items-center text-xs text-muted-foreground", "Loading conflict…" }
                },
                Some(Err(error)) => rsx! {
                    div { class: "grid min-h-48 place-items-center p-8 text-center",
                        div { class: "max-w-lg rounded-md border border-warning/40 bg-warning/10 p-4",
                            h2 { class: "text-sm font-semibold text-warning", "Block resolution unavailable" }
                            p { class: "mt-2 text-xs leading-relaxed text-muted-foreground",
                                "{error} The file was left unchanged; resolve it with an external Git tool, then stage it here, or abort the merge."
                            }
                        }
                    }
                },
                Some(Ok(file)) => rsx! {
                    div { class: "space-y-3 p-3",
                        for block in file.blocks {
                            section { class: "overflow-hidden rounded-md border border-border bg-card",
                                header { class: "flex min-h-9 items-center justify-between gap-2 border-b border-border bg-muted/45 px-3 py-1.5 text-[10px] text-muted-foreground",
                                    div { class: "flex min-w-0 items-center gap-2",
                                        strong { class: "font-medium text-foreground", "Conflict {block.index + 1}" }
                                        span { class: "truncate", "{block.current_label} → {block.incoming_label}" }
                                        span { class: "text-red-400", "−{block.current.lines().count()}" }
                                        span { class: "text-emerald-400", "+{block.incoming.lines().count()}" }
                                    }
                                    div { class: "flex gap-1",
                                        button {
                                            class: "rounded-md border border-border bg-background px-2 py-1 text-[10px] text-foreground hover:bg-muted",
                                            disabled: pending,
                                            onclick: {
                                                let path = selection.path.clone();
                                                let index = block.index;
                                                let fingerprint = block.fingerprint;
                                                move |_| {
                                                    on_mutation
                                                        .call(Mutation::ResolveConflict {
                                                            path: path.clone(),
                                                            index,
                                                            fingerprint,
                                                            choice: ConflictChoice::Incoming,
                                                        });
                                                }
                                            },
                                            "Accept"
                                        }
                                        button {
                                            class: "rounded-md bg-destructive/12 px-2 py-1 text-[10px] text-destructive hover:bg-destructive/20",
                                            disabled: pending,
                                            onclick: {
                                                let path = selection.path.clone();
                                                let index = block.index;
                                                let fingerprint = block.fingerprint;
                                                move |_| {
                                                    on_mutation
                                                        .call(Mutation::ResolveConflict {
                                                            path: path.clone(),
                                                            index,
                                                            fingerprint,
                                                            choice: ConflictChoice::Current,
                                                        });
                                                }
                                            },
                                            "Reject"
                                        }
                                        button {
                                            class: "rounded-md bg-secondary px-2 py-1 text-[10px] text-secondary-foreground hover:bg-muted",
                                            disabled: pending,
                                            onclick: {
                                                let path = selection.path.clone();
                                                let index = block.index;
                                                let fingerprint = block.fingerprint;
                                                move |_| {
                                                    on_mutation
                                                        .call(Mutation::ResolveConflict {
                                                            path: path.clone(),
                                                            index,
                                                            fingerprint,
                                                            choice: ConflictChoice::Both,
                                                        });
                                                }
                                            },
                                            "Merge"
                                        }
                                    }
                                }
                                div { class: "unified-diff overflow-x-auto font-mono text-[11px] leading-relaxed",
                                    for (line_number, line) in block.current.lines().enumerate() {
                                        div { class: "diff-line removed",
                                            span { "{line_number + 1}" }
                                            code { "-{line}" }
                                        }
                                    }
                                    for (line_number, line) in block.incoming.lines().enumerate() {
                                        div { class: "diff-line added",
                                            span { "{line_number + 1}" }
                                            code { "+{line}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn HunkDiff(
    diff: UnifiedDiff,
    selection: SelectedChange,
    pending: bool,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    let hunks = parse_diff_hunks(&diff.patch);
    let Ok(hunks) = hunks else {
        return rsx! {
            RawPatch { patch: diff.patch }
        };
    };
    if hunks.is_empty() {
        return rsx! {
            RawPatch { patch: diff.patch }
        };
    }
    rsx! {
        div { class: "min-w-165 space-y-3 overflow-x-auto p-3 font-mono text-[11px] leading-relaxed",
            for hunk in hunks {
                section { class: "overflow-hidden rounded-md border border-border bg-card",
                    header { class: "flex min-h-9 items-center justify-between gap-3 border-b border-border bg-muted/45 px-3 py-1.5 font-sans text-[10px] text-muted-foreground",
                        span { class: "min-w-0 truncate font-mono", "{hunk.header}" }
                        div { class: "flex shrink-0 items-center gap-1",
                            if hunk.deletions > 0 {
                                span { class: "mr-1 text-red-400", "−{hunk.deletions}" }
                            }
                            if hunk.additions > 0 {
                                span { class: "mr-1 text-emerald-400", "+{hunk.additions}" }
                            }
                            if selection.kind == DiffKind::Staged {
                                button {
                                    class: "rounded-md border border-border bg-background px-2 py-1 text-[10px] text-foreground hover:bg-muted",
                                    disabled: pending,
                                    onclick: {
                                        let path = selection.path.clone();
                                        let index = hunk.index;
                                        let fingerprint = hunk.fingerprint;
                                        move |_| {
                                            on_mutation
                                                .call(Mutation::Hunk {
                                                    path: path.clone(),
                                                    kind: DiffKind::Staged,
                                                    index,
                                                    fingerprint,
                                                    action: HunkAction::Unstage,
                                                });
                                        }
                                    },
                                    "Unstage"
                                }
                            } else {
                                button {
                                    class: "rounded-md border border-border bg-background px-2 py-1 text-[10px] text-foreground hover:bg-muted",
                                    disabled: pending,
                                    onclick: {
                                        let path = selection.path.clone();
                                        let index = hunk.index;
                                        let fingerprint = hunk.fingerprint;
                                        move |_| {
                                            on_mutation
                                                .call(Mutation::Hunk {
                                                    path: path.clone(),
                                                    kind: DiffKind::Worktree,
                                                    index,
                                                    fingerprint,
                                                    action: HunkAction::Stage,
                                                });
                                        }
                                    },
                                    "Accept"
                                }
                                button {
                                    class: "rounded-md bg-destructive/12 px-2 py-1 text-[10px] text-destructive hover:bg-destructive/20",
                                    disabled: pending,
                                    onclick: {
                                        let path = selection.path.clone();
                                        let index = hunk.index;
                                        let fingerprint = hunk.fingerprint;
                                        move |_| {
                                            on_mutation
                                                .call(Mutation::Hunk {
                                                    path: path.clone(),
                                                    kind: DiffKind::Worktree,
                                                    index,
                                                    fingerprint,
                                                    action: HunkAction::Discard,
                                                });
                                        }
                                    },
                                    "Reject"
                                }
                            }
                        }
                    }
                    div { class: "unified-diff",
                        for (line_number, line) in hunk.body.lines().enumerate() {
                            div { class: diff_line_class(line),
                                span { "{line_number + 1}" }
                                code { "{line}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RawPatch(patch: String) -> Element {
    rsx! {
        div { class: "unified-diff min-w-165 overflow-x-auto py-1 font-mono text-[11px] leading-relaxed",
            for (line_number, line) in patch.lines().enumerate() {
                div { class: diff_line_class(line),
                    span { "{line_number + 1}" }
                    code { "{line}" }
                }
            }
        }
    }
}

#[component]
fn HistoryDetail(
    detail: Option<Result<CommitDetail, ServerFnError>>,
    pending: bool,
    on_checkout: EventHandler<String>,
    on_revert: EventHandler<String>,
) -> Element {
    let Some(detail) = detail else {
        return rsx! {
            div { class: "grid h-full min-h-60 place-items-center p-8 text-center text-sm text-muted-foreground",
                "Select a commit to inspect its Git-generated patch."
            }
        };
    };
    let detail = match detail {
        Ok(detail) => detail,
        Err(error) => {
            return rsx! {
                div { class: "m-4 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive",
                    "Could not load commit: {error}"
                }
            }
        }
    };
    rsx! {
        div { class: "min-h-full min-w-165",
            header { class: "flex items-start justify-between gap-4 border-b border-border bg-card px-4 py-3",
                div { class: "min-w-0",
                    p { class: "font-mono text-[9px] tracking-wider text-primary",
                        {format!("COMMIT {}", detail.commit.short_oid)}
                    }
                    h2 { class: "mt-1 text-base font-semibold", {detail.commit.subject.clone()} }
                    p { class: "mt-1 text-[11px] text-muted-foreground",
                        {format!("{} <{}>", detail.commit.author_name, detail.commit.author_email)}
                    }
                    div { class: "mt-2 flex gap-3 text-[10px] text-muted-foreground",
                        span { {format!("{} files", detail.files_changed)} }
                        span { class: "text-success", {format!("+{}", detail.additions)} }
                        span { class: "text-destructive", {format!("−{}", detail.deletions)} }
                    }
                }
                div { class: "flex shrink-0 gap-1",
                    Button {
                        label: "Checkout",
                        kind: ButtonKind::Ghost,
                        size: ControlSize::Small,
                        disabled: pending,
                        onclick: {
                            let oid = detail.commit.oid.clone();
                            move |_| on_checkout.call(oid.clone())
                        },
                    }
                    Button {
                        label: "Revert",
                        kind: ButtonKind::Ghost,
                        size: ControlSize::Small,
                        disabled: pending,
                        onclick: {
                            let oid = detail.commit.oid.clone();
                            move |_| on_revert.call(oid.clone())
                        },
                    }
                }
            }
            if detail.patch.is_empty() {
                div { class: "grid min-h-48 place-items-center text-xs text-muted-foreground",
                    "This commit has no textual patch."
                }
            } else {
                div { class: "unified-diff overflow-x-auto py-1 font-mono text-[11px] leading-relaxed",
                    for (line_number, line) in detail.patch.lines().enumerate() {
                        div { class: diff_line_class(line),
                            span { "{line_number + 1}" }
                            code { "{line}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RemoteDialog(
    remote: Option<RemoteInfo>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<RemoteRequest>,
) -> Element {
    let editing = remote.is_some();
    let initial_name = remote
        .as_ref()
        .map_or_else(|| "origin".into(), |remote| remote.name.clone());
    let initial_fetch_url = remote
        .as_ref()
        .map(|remote| remote.fetch_url.clone())
        .unwrap_or_default();
    let initial_push_url = remote.as_ref().map_or_else(String::new, |remote| {
        if remote.push_url == remote.fetch_url {
            String::new()
        } else {
            remote.push_url.clone()
        }
    });
    let mut name = use_signal(|| initial_name);
    let mut fetch_url = use_signal(|| initial_fetch_url);
    let mut push_url = use_signal(|| initial_push_url);
    rsx! {
        Modal {
            title: if editing { "Edit remote" } else { "Add remote" },
            description: if editing { "Rename the remote or update its fetch and push URLs." } else { "Add a named Git remote. The push URL defaults to the fetch URL." },
            on_close,
            DialogForm {
                Field { control_id: "remote-name", label: "Name",
                    TextInput {
                        value: name(),
                        autofocus: true,
                        disabled: pending,
                        placeholder: "origin",
                        oninput: move |event: FormEvent| name.set(event.value()),
                    }
                }
                Field { control_id: "remote-fetch-url", label: "Fetch URL",
                    TextInput {
                        value: fetch_url(),
                        disabled: pending,
                        placeholder: "https://example.com/owner/repository.git",
                        oninput: move |event: FormEvent| fetch_url.set(event.value()),
                    }
                }
                Field {
                    control_id: "remote-push-url",
                    label: "Push URL (optional)",
                    TextInput {
                        value: push_url(),
                        disabled: pending,
                        placeholder: "Uses the fetch URL when empty",
                        oninput: move |event: FormEvent| push_url.set(event.value()),
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Saving…" } else if editing { "Save remote" } else { "Add remote" },
                        kind: ButtonKind::Primary,
                        disabled: pending || name().trim().is_empty() || fetch_url().trim().is_empty(),
                        onclick: move |_| {
                            let push = push_url();
                            on_submit
                                .call(
                                    remote_request(
                                        name().trim().to_owned(),
                                        fetch_url().trim().to_owned(),
                                        (!push.trim().is_empty()).then(|| push.trim().to_owned()),
                                    ),
                                );
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn RemoveRemoteDialog(
    remote: RemoteInfo,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Remove remote?",
            description: "This removes the remote configuration and its remote-tracking branches. It does not delete the remote repository.",
            on_close,
            DialogForm {
                div { class: "rounded-md border border-border bg-secondary/50 p-3",
                    strong { class: "block text-xs", "{remote.name}" }
                    small { class: "mt-1 block truncate text-[10px] text-muted-foreground",
                        {display_remote_url(&remote.fetch_url)}
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Removing…" } else { "Remove remote" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
fn BranchDialog(
    action: GitDialog,
    current_branch: String,
    branches: Vec<BranchInfo>,
    initial_name: Option<String>,
    start_point: Option<String>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let initial = if let Some(initial_name) = initial_name {
        initial_name
    } else if action == GitDialog::RenameBranch {
        current_branch.clone()
    } else if action == GitDialog::DeleteBranch {
        branches
            .iter()
            .find(|branch| !branch.current && !branch.remote)
            .map(|branch| branch.name.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let mut name = use_signal(|| initial);
    let (title, description, confirm) = match action {
        GitDialog::CreateBranch => (
            "Create branch",
            "Create a branch at HEAD and switch to it.",
            "Create branch",
        ),
        GitDialog::RenameBranch => (
            "Rename branch",
            "Rename the current local branch.",
            "Rename",
        ),
        GitDialog::DeleteBranch => (
            "Delete branch",
            "Delete a fully merged local branch.",
            "Delete branch",
        ),
        _ => (
            "Branch action",
            "Update this repository branch.",
            "Continue",
        ),
    };
    let description = if action == GitDialog::CreateBranch {
        start_point.as_ref().map_or_else(
            || description.to_owned(),
            |start_point| format!("Create a branch from {start_point} and switch to it."),
        )
    } else {
        description.to_owned()
    };
    rsx! {
        Modal { title, description, on_close,
            DialogForm {
                Field { control_id: "branch-name", label: "Branch name",
                    if action == GitDialog::DeleteBranch {
                        select {
                            id: "branch-name",
                            class: "h-9 w-full rounded-md border border-input bg-background px-2 text-xs",
                            value: name(),
                            disabled: pending,
                            onchange: move |event| name.set(event.value()),
                            for branch in branches {
                                if !branch.current && !branch.remote {
                                    option { value: branch.name.clone(), "{branch.name}" }
                                }
                            }
                        }
                    } else {
                        TextInput {
                            value: name(),
                            autofocus: true,
                            disabled: pending,
                            oninput: move |event: FormEvent| name.set(event.value()),
                        }
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Working…" } else { confirm },
                        kind: if action == GitDialog::DeleteBranch { ButtonKind::Danger } else { ButtonKind::Primary },
                        disabled: pending || name().trim().is_empty(),
                        onclick: move |_| on_submit.call(name()),
                    }
                }
            }
        }
    }
}

#[component]
fn TagDialog(
    tags: Vec<TagInfo>,
    target: Option<String>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_create: EventHandler<TagRequest>,
    on_delete: EventHandler<String>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut message = use_signal(String::new);
    let mut delete_target = use_signal(|| None::<String>);
    rsx! {
        Modal {
            title: "Repository tags",
            description: target
                .as_ref()
                .map_or_else(
                    || "Create a tag at HEAD or remove an existing local tag.".to_owned(),
                    |target| format!("Create a tag at {target} or remove an existing local tag."),
                ),
            on_close,
            DialogForm {
                if !tags.is_empty() {
                    div { class: "max-h-44 space-y-1 overflow-y-auto rounded-md border border-border p-1",
                        for tag in tags {
                            div { class: "flex min-h-9 items-center justify-between gap-3 rounded px-2 text-xs hover:bg-accent/60",
                                span { class: "min-w-0",
                                    strong { class: "block truncate font-medium", "{tag.name}" }
                                    small { class: "block truncate font-mono text-[9px] text-muted-foreground",
                                        if tag.annotated {
                                            "annotated · "
                                        } else {
                                            "lightweight · "
                                        }
                                        "{short_oid(&tag.target_oid)}"
                                    }
                                }
                                if delete_target().as_deref() == Some(tag.name.as_str()) {
                                    div { class: "flex shrink-0 gap-1",
                                        Button {
                                            label: "Cancel",
                                            kind: ButtonKind::Ghost,
                                            size: ControlSize::Small,
                                            disabled: pending,
                                            onclick: move |_| delete_target.set(None),
                                        }
                                        Button {
                                            label: "Confirm delete",
                                            kind: ButtonKind::Danger,
                                            size: ControlSize::Small,
                                            disabled: pending,
                                            onclick: {
                                                let name = tag.name.clone();
                                                move |_| on_delete.call(name.clone())
                                            },
                                        }
                                    }
                                } else {
                                    Button {
                                        label: "Delete",
                                        kind: ButtonKind::Ghost,
                                        size: ControlSize::Small,
                                        disabled: pending,
                                        onclick: {
                                            let name = tag.name.clone();
                                            move |_| delete_target.set(Some(name.clone()))
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
                Field { control_id: "tag-name", label: "New tag name",
                    TextInput {
                        value: name(),
                        autofocus: true,
                        disabled: pending,
                        placeholder: "v1.0.0",
                        oninput: move |event: FormEvent| name.set(event.value()),
                    }
                }
                Field { control_id: "tag-message", label: "Annotation (optional)",
                    TextArea {
                        rows: 3,
                        value: message(),
                        disabled: pending,
                        placeholder: "Leave empty for a lightweight tag",
                        oninput: move |event: FormEvent| message.set(event.value()),
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Close",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Working…" } else { "Create tag" },
                        kind: ButtonKind::Primary,
                        disabled: pending || name().trim().is_empty(),
                        onclick: move |_| {
                            let annotation = message();
                            on_create
                                .call(TagRequest {
                                    name: name(),
                                    target: target.clone(),
                                    message: (!annotation.trim().is_empty()).then_some(annotation),
                                });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn CompareMergeDialog(
    current_branch: String,
    branches: Vec<BranchInfo>,
    initial_target: Option<String>,
    comparison: Option<BranchComparison>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_compare: EventHandler<String>,
    on_merge: EventHandler<String>,
) -> Element {
    let default_target = initial_target
        .filter(|name| branches.iter().any(|branch| branch.name == *name))
        .or_else(|| {
            branches
                .iter()
                .find(|branch| branch.name != current_branch)
                .map(|branch| branch.name.clone())
        })
        .unwrap_or_default();
    let mut target = use_signal(|| default_target);
    let comparison_matches = comparison
        .as_ref()
        .is_some_and(|value| value.base == current_branch && value.head == target());
    rsx! {
        Modal {
            title: "Compare and merge branch",
            description: "Review commits and the Git-generated three-dot diff before merging into the current branch.",
            on_close,
            DialogForm {
                div { class: "grid grid-cols-[minmax(0,1fr)_auto] items-end gap-2",
                    Field {
                        control_id: "compare-branch",
                        label: "Compare with {current_branch}",
                        select {
                            id: "compare-branch",
                            class: "h-9 w-full rounded-md border border-input bg-background px-2 text-xs",
                            value: target(),
                            disabled: pending,
                            onchange: move |event| target.set(event.value()),
                            for branch in branches {
                                if branch.name != current_branch {
                                    option { value: branch.name.clone(), "{branch.name}" }
                                }
                            }
                        }
                    }
                    Button {
                        label: if pending { "Loading…" } else { "Compare" },
                        kind: ButtonKind::Ghost,
                        disabled: pending || target().is_empty(),
                        onclick: move |_| on_compare.call(target()),
                    }
                }
                if let Some(value) = comparison {
                    div { class: "space-y-2 rounded-md border border-border",
                        div { class: "flex flex-wrap gap-3 border-b border-border px-3 py-2 text-[10px] text-muted-foreground",
                            span { "{value.base_only_commits} only on {value.base}" }
                            span { "{value.head_only_commits} only on {value.head}" }
                            span { "{value.files_changed} files" }
                            span { class: "text-success", "+{value.additions}" }
                            span { class: "text-destructive", "−{value.deletions}" }
                        }
                        if !value.commits.is_empty() {
                            div { class: "max-h-24 overflow-y-auto px-3 text-[10px]",
                                for commit in value.commits {
                                    p { class: "truncate py-1",
                                        code { class: "mr-2 text-primary", "{commit.short_oid}" }
                                        "{commit.subject}"
                                    }
                                }
                            }
                        }
                        if value.patch.is_empty() {
                            p { class: "px-3 pb-3 text-xs text-muted-foreground",
                                "No file differences to display."
                            }
                        } else {
                            div { class: "max-h-64 overflow-auto border-t border-border",
                                RawPatch { patch: value.patch }
                            }
                        }
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Working…" } else { "Merge into current branch" },
                        kind: ButtonKind::Primary,
                        disabled: pending || !comparison_matches,
                        onclick: move |_| on_merge.call(target()),
                    }
                }
            }
        }
    }
}

#[component]
fn AbortMergeDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Abort merge?",
            description: "Restore the index and working tree to their state before the current merge began.",
            on_close,
            DialogForm {
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Aborting…" } else { "Abort merge" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
fn DiscardAllDialog(
    changed_files: usize,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Discard all changes?",
            description: "Restore every staged and unstaged file to HEAD and remove untracked files. This cannot be undone.",
            on_close,
            DialogForm {
                p { class: "rounded-md border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive",
                    "{changed_files} changed file(s) will be discarded."
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Discarding…" } else { "Discard all changes" },
                        kind: ButtonKind::Danger,
                        disabled: pending || changed_files == 0,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
fn CommitHistoryActionDialog(
    action: GitDialog,
    commit: CommitInfo,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let checkout = action == GitDialog::CheckoutCommit;
    rsx! {
        Modal {
            title: if checkout { "Checkout commit?" } else { "Revert commit?" },
            description: if checkout { "Checkout switches to this snapshot in detached HEAD mode. Create a branch before committing new work." } else { "Revert creates a new commit that reverses this commit. Git may stop for conflict resolution." },
            on_close,
            DialogForm {
                div { class: "rounded-md border border-border bg-secondary/50 p-3",
                    p { class: "truncate text-xs font-medium", "{commit.subject}" }
                    p { class: "mt-1 font-mono text-[9px] text-muted-foreground",
                        "{commit.short_oid}"
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Working…" } else if checkout { "Checkout commit" } else { "Create revert commit" },
                        kind: if checkout { ButtonKind::Primary } else { ButtonKind::Danger },
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
fn ForcePushDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let mut acknowledged = use_signal(|| false);
    rsx! {
        Modal {
            title: "Force push with lease?",
            description: "The remote rejected the normal push as non-fast-forward. The lease prevents replacing commits you have not fetched.",
            on_close,
            DialogForm {
                label { class: "flex items-center gap-2.5 text-xs",
                    Checkbox {
                        checked: acknowledged(),
                        aria_label: "Confirm force push with lease",
                        disabled: pending,
                        on_checked_change: move |checked| acknowledged.set(checked),
                    }
                    span { "I understand that remote commits may be replaced." }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Pushing…" } else { "Force push with lease" },
                        kind: ButtonKind::Danger,
                        disabled: pending || !acknowledged(),
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
fn CommitDialog(
    workspace_slug: String,
    initial_message: String,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<CommitRequest>,
) -> Element {
    let mut message = use_signal(|| initial_message);
    let mut amend = use_signal(|| false);
    rsx! {
        Modal {
            title: if amend() { "Amend previous commit" } else { "Commit staged changes" },
            description: "Git will use the configured identity, hooks, and signing settings.",
            on_close,
            DialogForm {
                Field { control_id: "commit-message", label: "Commit message",
                    TextArea {
                        rows: 4,
                        value: message(),
                        placeholder: "Describe your changes",
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| message.set(event.value()),
                    }
                }
                label { class: "compact flex items-center gap-2.5 py-1.75",
                    Checkbox {
                        checked: amend(),
                        aria_label: "Amend previous commit",
                        disabled: pending,
                        on_checked_change: move |checked| {
                            amend.set(checked);
                            if checked && message().trim().is_empty() {
                                let slug = workspace_slug.clone();
                                spawn(async move {
                                    let previous_message =
                                        api::commit_message(slug, "HEAD".into()).await;
                                    if let Ok(previous_message) = previous_message {
                                        if amend() && message().trim().is_empty() {
                                            message.set(previous_message);
                                        }
                                    }
                                });
                            }
                        },
                    }
                    span { "Amend previous commit" }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Committing…" } else { "Commit" },
                        kind: ButtonKind::Primary,
                        disabled: pending || message().trim().is_empty(),
                        onclick: move |_| {
                            on_submit
                                .call(CommitRequest {
                                    message: message(),
                                    amend: amend(),
                                    signing_passphrase: None,
                                });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn SigningDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let mut passphrase = use_signal(String::new);
    rsx! {
        Modal {
            title: "Signing passphrase required",
            description: "The passphrase is sent only for this commit retry and is not stored.",
            on_close,
            DialogForm {
                Field {
                    control_id: "signing-passphrase",
                    label: "Signing passphrase",
                    TextInput {
                        input_type: TextInputType::Password,
                        value: passphrase(),
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| passphrase.set(event.value()),
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Signing…" } else { "Retry signed commit" },
                        kind: ButtonKind::Primary,
                        disabled: pending || passphrase().is_empty(),
                        onclick: move |_| {
                            let secret = std::mem::take(&mut *passphrase.write());
                            on_submit.call(secret);
                        },
                    }
                }
            }
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
