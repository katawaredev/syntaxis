use super::Notice;
use dioxus::prelude::*;
use dioxus_code_editor::{DiffLayout, UnifiedDiffView};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use syntaxis_editor::language_slug_for_path;
use syntaxis_git::ChangeKind;
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Field, Icon, Modal,
    RepositoryChangeRow, RepositoryChangeSection, RepositoryEmptyDetail, RepositoryPanelHeader,
    RepositoryShell, RepositorySidebarTabs, RepositorySidebarView, TextArea, TextInput,
    TextInputType,
};
use syntaxis_workspace::WorkspaceRecord;

/// Kept hidden when opening a workspace created by the old snapshot implementation.
pub(super) const LEGACY_HISTORY_PATH: &str = ".syntaxis-guest-history.json";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum BrowserChangeKind {
    Added,
    Modified,
    Deleted,
}

impl From<BrowserChangeKind> for ChangeKind {
    fn from(value: BrowserChangeKind) -> Self {
        match value {
            BrowserChangeKind::Added => Self::Added,
            BrowserChangeKind::Modified => Self::Modified,
            BrowserChangeKind::Deleted => Self::Deleted,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct BrowserChange {
    path: String,
    staged: Option<BrowserChangeKind>,
    unstaged: Option<BrowserChangeKind>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct BrowserCommit {
    oid: String,
    short_oid: String,
    subject: String,
    message: String,
    author_name: String,
    author_email: String,
    date: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct BrowserRemote {
    remote: String,
    url: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct BrowserRepository {
    initialized: bool,
    branch: Option<String>,
    branches: Vec<String>,
    remotes: Vec<BrowserRemote>,
    changes: Vec<BrowserChange>,
    commits: Vec<BrowserCommit>,
    author_name: Option<String>,
    author_email: Option<String>,
}

impl BrowserRepository {
    fn staged(&self) -> Vec<BrowserChange> {
        self.changes
            .iter()
            .filter(|change| change.staged.is_some())
            .cloned()
            .collect()
    }

    fn unstaged(&self) -> Vec<BrowserChange> {
        self.changes
            .iter()
            .filter(|change| change.unstaged.is_some())
            .cloned()
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangeArea {
    Staged,
    Worktree,
}

impl ChangeArea {
    const fn bridge_name(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Worktree => "worktree",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SelectedChange {
    path: String,
    area: ChangeArea,
}

#[derive(Clone, Debug, Deserialize)]
struct BrowserDiff {
    path: String,
    binary: bool,
    before: String,
    after: String,
}

#[derive(Serialize)]
struct GitBridgeRequest {
    method: String,
    payload: Value,
}

#[derive(Deserialize)]
struct GitBridgeResponse<T> {
    ok: bool,
    value: Option<T>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct CommitResult {
    oid: String,
    repository: BrowserRepository,
}

#[component]
pub(super) fn GuestGit(
    workspace: WorkspaceRecord,
    revision: Signal<u64>,
    dirty: bool,
    mut notice: Signal<Option<Notice>>,
    on_workspace_changed: EventHandler<()>,
) -> Element {
    let _ = workspace;
    let mut refresh = use_signal(|| 0_u64);
    let mut busy = use_signal(|| false);
    let mut sidebar_view = use_signal(RepositorySidebarView::default);
    let mut sidebar_open = use_signal(|| true);
    let selected_change = use_signal(|| None::<SelectedChange>);
    let selected_commit_id = use_signal(|| None::<String>);
    let commit_open = use_signal(|| false);
    let commit_message = use_signal(String::new);
    let mut author_name = use_signal(|| "Syntaxis Guest".to_owned());
    let mut author_email = use_signal(|| "guest@syntaxis.local".to_owned());
    let sync_open = use_signal(|| false);
    let branch_open = use_signal(|| false);
    let branch_name = use_signal(String::new);

    let repository = use_resource(move || {
        let _workspace_revision = revision();
        let _git_refresh = refresh();
        async move { git_request::<BrowserRepository>("repository", Value::Null).await }
    });
    use_effect(move || {
        let Some(Ok(repository)) = repository() else {
            return;
        };
        if let Some(name) = repository.author_name
            && author_name() == "Syntaxis Guest"
        {
            author_name.set(name);
        }
        if let Some(email) = repository.author_email
            && author_email() == "guest@syntaxis.local"
        {
            author_email.set(email);
        }
    });
    let diff = use_resource(move || {
        let selection = selected_change();
        let _git_refresh = refresh();
        async move {
            let Some(selection) = selection else {
                return Ok(None);
            };
            git_request::<BrowserDiff>(
                "diff",
                json!([selection.path, selection.area.bridge_name()]),
            )
            .await
            .map(Some)
        }
    });

    rsx! {
        match repository() {
            None => rsx! {
                div { class: "flex size-full items-center justify-center gap-2 text-xs text-muted-foreground", role: "status",
                    span { class: "size-4 animate-spin rounded-full border-2 border-border border-t-primary", aria_hidden: "true" }
                    "Reading Git repository…"
                }
            },
            Some(Err(error)) => rsx! {
                div { class: "flex size-full flex-col items-center justify-center gap-3 p-6 text-center",
                    Icon { icon: AppIcon::GitBranch, size: 26 }
                    strong { class: "text-sm", "Git could not read this workspace" }
                    p { class: "max-w-lg text-xs text-destructive", "{error}" }
                    Button { label: "Try again", disabled: busy(), onclick: move |_| refresh += 1 }
                }
            },
            Some(Ok(repository)) => if repository.initialized {
                rsx! {
                    GitRepositoryContent {
                    repository,
                    dirty,
                    busy,
                    refresh,
                    notice,
                    sidebar_view,
                    sidebar_open,
                    selected_change,
                    selected_commit_id,
                    diff,
                    commit_open,
                    commit_message,
                    author_name,
                    author_email,
                    sync_open,
                    branch_open,
                    branch_name,
                    on_workspace_changed,
                    }
                }
            } else {
                rsx! {
                    RepositoryShell {
                    sidebar_open: sidebar_open(),
                    sidebar: rsx! {
                        RepositorySidebarTabs {
                            active: RepositorySidebarView::Changes,
                            changes: 0,
                            on_change: move |view| sidebar_view.set(view),
                        }
                        div { class: "flex min-h-52 flex-1 flex-col items-center justify-center p-5 text-center",
                            Icon { icon: AppIcon::GitBranch, size: 26 }
                            strong { class: "mt-3 text-sm", "Initialize repository" }
                            p { class: "mt-1 max-w-60 text-[11px] leading-relaxed text-muted-foreground",
                                "Create a real .git repository here. Git files, objects, refs, and commits stay in the active browser workspace."
                            }
                            Button {
                                label: if busy() { "Initializing…" } else { "Initialize" },
                                kind: ButtonKind::Primary,
                                disabled: busy() || dirty,
                                onclick: move |_| {
                                    busy.set(true);
                                    spawn(async move {
                                        match git_request::<BrowserRepository>("init", json!("main")).await {
                                            Ok(_) => {
                                                refresh += 1;
                                                notice.set(Some(Notice::success("Initialized Git repository on main.")));
                                            }
                                            Err(error) => notice.set(Some(Notice::error(error))),
                                        }
                                        busy.set(false);
                                    });
                                },
                            }
                        }
                    },
                    header: rsx! {
                        RepositoryPanelHeader {
                            title: "No repository",
                            subtitle: None,
                            sidebar_open: sidebar_open(),
                            on_toggle_sidebar: move |()| sidebar_open.toggle(),
                            actions: rsx! {
                                Button { label: "Commit", kind: ButtonKind::Primary, disabled: true, onclick: move |_| {} }
                                Button { label: "Fetch", kind: ButtonKind::Ghost, disabled: true, onclick: move |_| {} }
                            },
                        }
                    },
                    detail: rsx! { RepositoryEmptyDetail { message: "Initialize this workspace to use browser Git." } },
                    }
                }
            },
        }
    }
}

#[component]
fn GitRepositoryContent(
    repository: BrowserRepository,
    dirty: bool,
    mut busy: Signal<bool>,
    mut refresh: Signal<u64>,
    mut notice: Signal<Option<Notice>>,
    mut sidebar_view: Signal<RepositorySidebarView>,
    mut sidebar_open: Signal<bool>,
    mut selected_change: Signal<Option<SelectedChange>>,
    mut selected_commit_id: Signal<Option<String>>,
    diff: Resource<Result<Option<BrowserDiff>, String>>,
    mut commit_open: Signal<bool>,
    mut commit_message: Signal<String>,
    mut author_name: Signal<String>,
    mut author_email: Signal<String>,
    mut sync_open: Signal<bool>,
    mut branch_open: Signal<bool>,
    mut branch_name: Signal<String>,
    on_workspace_changed: EventHandler<()>,
) -> Element {
    let staged = repository.staged();
    let unstaged = repository.unstaged();
    let changed_count = repository.changes.len();
    let branch = repository
        .branch
        .clone()
        .unwrap_or_else(|| "HEAD".to_owned());
    let selected_commit = selected_commit_id().and_then(|oid| {
        repository
            .commits
            .iter()
            .find(|commit| commit.oid == oid)
            .cloned()
    });
    rsx! {
        RepositoryShell {
            sidebar_open: sidebar_open(),
            sidebar: rsx! {
                RepositorySidebarTabs {
                    active: sidebar_view(),
                    changes: changed_count,
                    on_change: move |view| {
                        sidebar_view.set(view);
                        if view == RepositorySidebarView::Changes { selected_commit_id.set(None); }
                        else { selected_change.set(None); }
                    },
                }
                div { class: "min-h-0 flex-1 overflow-y-auto p-2",
                    if sidebar_view() == RepositorySidebarView::Changes {
                        ChangeList {
                            staged: staged.clone(),
                            unstaged: unstaged.clone(),
                            dirty,
                            busy,
                            refresh,
                            notice,
                            selected_change,
                        }
                    } else if repository.commits.is_empty() {
                        div { class: "grid h-full min-h-40 place-items-center p-4 text-center text-xs text-muted-foreground", "No commits yet." }
                    } else {
                        div { class: "space-y-1",
                            for commit in repository.commits.clone() {
                                button {
                                    key: "{commit.oid}",
                                    class: if selected_commit_id().as_deref() == Some(commit.oid.as_str()) { "flex w-full min-w-0 gap-2 rounded-md bg-muted p-2 text-left text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring" } else { "flex w-full min-w-0 gap-2 rounded-md p-2 text-left text-muted-foreground outline-none hover:bg-muted/60 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" },
                                    onclick: {
                                        let oid = commit.oid.clone();
                                        move |_| selected_commit_id.set(Some(oid.clone()))
                                    },
                                    span { class: "mt-1.5 size-2 shrink-0 rounded-full border-2 border-primary" }
                                    span { class: "min-w-0",
                                        strong { class: "block truncate text-xs font-medium", "{commit.subject}" }
                                        small { class: "mt-1 block truncate font-mono text-[10px] text-muted-foreground", "{commit.short_oid} · {commit.author_name}" }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            header: rsx! {
                RepositoryPanelHeader {
                    title: branch,
                    subtitle: Some(if changed_count == 1 { "1 file changed".to_owned() } else { format!("{changed_count} files changed") }),
                    sidebar_open: sidebar_open(),
                    on_toggle_sidebar: move |()| sidebar_open.toggle(),
                    actions: rsx! {
                        Button {
                            label: "Commit",
                            kind: ButtonKind::Primary,
                            disabled: busy() || dirty || staged.is_empty(),
                            onclick: move |_| commit_open.set(true),
                        }
                        select {
                            class: "touch-input max-w-32 rounded-md border border-border bg-background px-2 text-xs text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
                            aria_label: "Current Git branch",
                            disabled: busy() || dirty,
                            value: repository.branch.clone().unwrap_or_default(),
                            onchange: move |event| {
                                let branch = event.value();
                                busy.set(true);
                                spawn(async move {
                                    match git_request::<BrowserRepository>("checkout", json!(branch)).await {
                                        Ok(_) => {
                                            refresh += 1;
                                            on_workspace_changed.call(());
                                        }
                                        Err(error) => notice.set(Some(Notice::error(error))),
                                    }
                                    busy.set(false);
                                });
                            },
                            for branch_name in repository.branches.clone() {
                                option { value: branch_name.clone(), "{branch_name}" }
                            }
                        }
                        Button {
                            label: "Branch",
                            kind: ButtonKind::Ghost,
                            disabled: busy() || dirty || repository.commits.is_empty(),
                            onclick: move |_| branch_open.set(true),
                        }
                        button {
                            class: "touch-target px-3 text-xs font-semibold text-muted-foreground hover:text-foreground disabled:opacity-50",
                            disabled: busy(),
                            title: "Configure HTTPS remote access",
                            onclick: move |_| sync_open.set(true),
                            "Fetch"
                        }
                    },
                }
            },
            detail: rsx! {
                RepositoryDetail {
                    sidebar_view: sidebar_view(),
                    selection: selected_change(),
                    selected_commit,
                    diff,
                    dirty,
                    busy,
                    refresh,
                    notice,
                }
            },
        }
        if commit_open() {
            CommitDialog {
                busy,
                refresh,
                notice,
                commit_open,
                commit_message,
                author_name,
                author_email,
                selected_change,
            }
        }
        if sync_open() {
            SyncDialog {
                initial_url: repository.remotes.iter().find(|remote| remote.remote == "origin").map(|remote| remote.url.clone()).unwrap_or_default(),
                busy,
                refresh,
                notice,
                sync_open,
                on_workspace_changed,
            }
        }
        if branch_open() {
            BranchDialog {
                busy,
                refresh,
                notice,
                branch_open,
                branch_name,
                on_workspace_changed,
            }
        }
    }
}

#[component]
fn ChangeList(
    staged: Vec<BrowserChange>,
    unstaged: Vec<BrowserChange>,
    dirty: bool,
    busy: Signal<bool>,
    refresh: Signal<u64>,
    notice: Signal<Option<Notice>>,
    mut selected_change: Signal<Option<SelectedChange>>,
) -> Element {
    if staged.is_empty() && unstaged.is_empty() {
        return rsx! { div { class: "grid h-full min-h-40 place-items-center p-4 text-center text-xs text-muted-foreground", "Working tree clean." } };
    }
    rsx! {
        div { class: "space-y-3",
            ChangeSection { title: "Staged", area: ChangeArea::Staged, changes: staged, dirty, busy, refresh, notice, selected_change }
            ChangeSection { title: "Changes", area: ChangeArea::Worktree, changes: unstaged, dirty, busy, refresh, notice, selected_change }
        }
    }
}

#[component]
fn ChangeSection(
    title: String,
    area: ChangeArea,
    changes: Vec<BrowserChange>,
    dirty: bool,
    busy: Signal<bool>,
    refresh: Signal<u64>,
    notice: Signal<Option<Notice>>,
    mut selected_change: Signal<Option<SelectedChange>>,
) -> Element {
    let method = if area == ChangeArea::Staged {
        "unstage"
    } else {
        "stage"
    };
    let label = if area == ChangeArea::Staged {
        "Unstage"
    } else {
        "Stage"
    };
    let paths = changes
        .iter()
        .map(|change| change.path.clone())
        .collect::<Vec<_>>();
    rsx! {
        RepositoryChangeSection {
            title,
            count: changes.len(),
            batch_label: Some(label.to_owned()),
            pending: busy() || (dirty && area == ChangeArea::Worktree),
            on_batch: move |()| run_change_action(method, paths.clone(), busy, refresh, notice),
            for change in changes {
                RepositoryChangeRow {
                    key: "{area:?}-{change.path}",
                    path: change.path.clone(),
                    kind: if area == ChangeArea::Staged { change.staged.map(Into::into) } else { change.unstaged.map(Into::into) },
                    active: selected_change().as_ref().is_some_and(|selected| selected.path == change.path && selected.area == area),
                    onclick: {
                        let path = change.path.clone();
                        move |()| selected_change.set(Some(SelectedChange { path: path.clone(), area }))
                    },
                }
            }
        }
    }
}

#[component]
fn RepositoryDetail(
    sidebar_view: RepositorySidebarView,
    selection: Option<SelectedChange>,
    selected_commit: Option<BrowserCommit>,
    diff: Resource<Result<Option<BrowserDiff>, String>>,
    dirty: bool,
    busy: Signal<bool>,
    refresh: Signal<u64>,
    notice: Signal<Option<Notice>>,
) -> Element {
    if sidebar_view == RepositorySidebarView::History {
        return if let Some(commit) = selected_commit {
            let subject = commit.subject.clone();
            let message = commit.message.clone();
            let oid = commit.oid.clone();
            let author_name = commit.author_name.clone();
            let author_email = commit.author_email.clone();
            let author = format!("{author_name} <{author_email}>");
            let date = commit.date.clone();
            rsx! {
                div { class: "p-5",
                    div { class: "flex items-center gap-2",
                        Icon { icon: AppIcon::Commit, size: 17 }
                        h2 { class: "text-sm font-medium", {subject} }
                    }
                    p { class: "mt-3 whitespace-pre-wrap text-xs", {message} }
                    dl { class: "mt-5 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-[11px]",
                        dt { class: "text-muted-foreground", "Commit" }
                        dd { class: "break-all font-mono", {oid} }
                        dt { class: "text-muted-foreground", "Author" }
                        dd { {author} }
                        dt { class: "text-muted-foreground", "Date" }
                        dd { {date} }
                    }
                }
            }
        } else {
            rsx! { RepositoryEmptyDetail { message: "Select a commit to inspect it." } }
        };
    }
    let Some(selection) = selection else {
        return rsx! { RepositoryEmptyDetail { message: "Select a changed file to inspect its Git diff." } };
    };
    let method = if selection.area == ChangeArea::Staged {
        "unstage"
    } else {
        "stage"
    };
    rsx! {
        div { class: "flex size-full min-w-0 flex-col",
            div { class: "flex min-h-12 items-center gap-2 border-b border-border px-3",
                strong { class: "min-w-0 flex-1 truncate text-xs", "{selection.path}" }
                Button {
                    label: if selection.area == ChangeArea::Staged { "Unstage" } else { "Stage" },
                    disabled: busy() || (dirty && selection.area == ChangeArea::Worktree),
                    onclick: {
                        let path = selection.path.clone();
                        move |_| run_change_action(method, vec![path.clone()], busy, refresh, notice)
                    },
                }
            }
            div { class: "min-h-0 min-w-0 flex-1 overflow-auto",
                match diff() {
                    None => rsx! { div { class: "grid size-full place-items-center text-xs text-muted-foreground", "Loading diff…" } },
                    Some(Err(error)) => rsx! { div { class: "grid size-full place-items-center p-5 text-xs text-destructive", "{error}" } },
                    Some(Ok(None)) => rsx! { RepositoryEmptyDetail { message: "Select a changed file to inspect it." } },
                    Some(Ok(Some(diff))) => rsx! {
                        if diff.binary {
                            RepositoryEmptyDetail { message: "Binary file changed. A text diff is unavailable." }
                        } else {
                            UnifiedDiffView {
                                original: diff.before,
                                current: diff.after,
                                language: language_slug_for_path(&diff.path),
                                filename: diff.path,
                                collapse_unchanged: false,
                                layout: DiffLayout::FullFile,
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn CommitDialog(
    mut busy: Signal<bool>,
    mut refresh: Signal<u64>,
    mut notice: Signal<Option<Notice>>,
    mut commit_open: Signal<bool>,
    mut commit_message: Signal<String>,
    mut author_name: Signal<String>,
    mut author_email: Signal<String>,
    mut selected_change: Signal<Option<SelectedChange>>,
) -> Element {
    let invalid = commit_message().trim().is_empty()
        || author_name().trim().is_empty()
        || author_email().trim().is_empty();
    rsx! {
        Modal {
            title: "Commit staged changes",
            description: "Create a real local Git commit in this browser workspace.",
            on_close: move |()| commit_open.set(false),
            form {
                onsubmit: move |event| {
                    event.prevent_default();
                    if invalid || busy() { return; }
                    let payload = json!({
                        "message": commit_message().trim(),
                        "name": author_name().trim(),
                        "email": author_email().trim(),
                    });
                    busy.set(true);
                    spawn(async move {
                        match git_request::<CommitResult>("commit", payload).await {
                            Ok(result) => {
                                let short_oid = result.oid.chars().take(7).collect::<String>();
                                let _repository = result.repository;
                                commit_message.set(String::new());
                                commit_open.set(false);
                                selected_change.set(None);
                                refresh += 1;
                                notice.set(Some(Notice::success(format!("Created commit {short_oid}."))));
                            }
                            Err(error) => notice.set(Some(Notice::error(error))),
                        }
                        busy.set(false);
                    });
                },
                DialogForm {
                    Field { control_id: "guest-git-message", label: "Commit message", required: true,
                        TextArea {
                            value: commit_message(),
                            name: "message",
                            placeholder: "Describe these changes",
                            rows: 4,
                            autofocus: true,
                            oninput: move |event: FormEvent| commit_message.set(event.value()),
                        }
                    }
                    div { class: "grid grid-cols-2 gap-3 max-sm:grid-cols-1",
                        Field { control_id: "guest-git-author-name", label: "Author name", required: true,
                            TextInput { value: author_name(), name: "author-name", autocomplete: "name", oninput: move |event: FormEvent| author_name.set(event.value()) }
                        }
                        Field { control_id: "guest-git-author-email", label: "Author email", required: true,
                            TextInput { value: author_email(), name: "author-email", autocomplete: "email", oninput: move |event: FormEvent| author_email.set(event.value()) }
                        }
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |event: MouseEvent| { event.prevent_default(); commit_open.set(false); },
                        }
                        Button { label: if busy() { "Committing…" } else { "Commit" }, kind: ButtonKind::Primary, disabled: busy() || invalid, onclick: move |_| {} }
                    }
                }
            }
        }
    }
}

#[component]
fn SyncDialog(
    initial_url: String,
    mut busy: Signal<bool>,
    mut refresh: Signal<u64>,
    mut notice: Signal<Option<Notice>>,
    mut sync_open: Signal<bool>,
    on_workspace_changed: EventHandler<()>,
) -> Element {
    let mut action = use_signal(|| "fetch".to_owned());
    let mut url = use_signal(|| initial_url);
    let mut cors_proxy = use_signal(String::new);
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    rsx! {
        Modal {
            title: "Remote Git",
            description: "Fetch, fast-forward pull, or push through Git Smart HTTP.",
            on_close: move |()| sync_open.set(false),
            form {
                onsubmit: move |event| {
                    event.prevent_default();
                    if busy() || url().trim().is_empty() { return; }
                    let requested_action = action();
                    let payload = json!({
                        "action": requested_action,
                        "url": url().trim(),
                        "corsProxy": cors_proxy().trim(),
                        "username": username(),
                        "password": password(),
                    });
                    busy.set(true);
                    spawn(async move {
                        match git_request::<BrowserRepository>("sync", payload).await {
                            Ok(_) => {
                                if action() == "pull" { on_workspace_changed.call(()); }
                                password.set(String::new());
                                sync_open.set(false);
                                refresh += 1;
                                notice.set(Some(Notice::success(format!("Git {} completed.", action()))));
                            }
                            Err(error) => notice.set(Some(Notice::error(error))),
                        }
                        busy.set(false);
                    });
                },
                DialogForm {
                    Field {
                        control_id: "guest-git-remote-url",
                        label: "Origin URL",
                        description: "HTTPS only. The origin is saved in .git/config.",
                        required: true,
                        TextInput {
                            input_type: TextInputType::Url,
                            value: url(),
                            name: "remote-url",
                            autocomplete: "url",
                            placeholder: "https://github.com/owner/repository.git",
                            oninput: move |event: FormEvent| url.set(event.value()),
                        }
                    }
                    div { class: "grid grid-cols-2 gap-3 max-sm:grid-cols-1",
                        Field { control_id: "guest-git-operation", label: "Operation",
                            select {
                                id: "guest-git-operation",
                                class: "touch-input w-full rounded-md border border-input bg-background px-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                value: action(),
                                onchange: move |event| action.set(event.value()),
                                option { value: "fetch", "Fetch" }
                                option { value: "pull", "Pull (fast-forward only)" }
                                option { value: "push", "Push" }
                            }
                        }
                        Field {
                            control_id: "guest-git-cors-proxy",
                            label: "CORS proxy",
                            description: "Usually required by Git hosts. Use a proxy you trust.",
                            TextInput {
                                input_type: TextInputType::Url,
                                value: cors_proxy(),
                                name: "cors-proxy",
                                autocomplete: "url",
                                placeholder: "https://your-git-proxy.example",
                                oninput: move |event: FormEvent| cors_proxy.set(event.value()),
                            }
                        }
                    }
                    div { class: "grid grid-cols-2 gap-3 max-sm:grid-cols-1",
                        Field { control_id: "guest-git-username", label: "Username",
                            TextInput { value: username(), name: "username", autocomplete: "username", oninput: move |event: FormEvent| username.set(event.value()) }
                        }
                        Field {
                            control_id: "guest-git-password",
                            label: "Password or token",
                            description: "Used for this request only and never persisted.",
                            TextInput {
                                input_type: TextInputType::Password,
                                value: password(),
                                name: "password",
                                autocomplete: "current-password",
                                oninput: move |event: FormEvent| password.set(event.value()),
                            }
                        }
                    }
                    p { class: "rounded-md border border-border bg-muted/40 p-2 text-[11px] leading-relaxed text-muted-foreground",
                        "Browser security blocks most Git hosts unless they enable CORS. A configured proxy can relay Smart HTTP; SSH, credential helpers, and GPG signing are unavailable."
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |event: MouseEvent| { event.prevent_default(); sync_open.set(false); },
                        }
                        Button {
                            label: if busy() { "Working…" } else { "Run operation" },
                            kind: ButtonKind::Primary,
                            disabled: busy() || url().trim().is_empty(),
                            onclick: move |_| {},
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BranchDialog(
    mut busy: Signal<bool>,
    mut refresh: Signal<u64>,
    mut notice: Signal<Option<Notice>>,
    mut branch_open: Signal<bool>,
    mut branch_name: Signal<String>,
    on_workspace_changed: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Create branch",
            description: "Create a local branch at HEAD and switch the workspace to it.",
            on_close: move |()| branch_open.set(false),
            form {
                onsubmit: move |event| {
                    event.prevent_default();
                    let name = branch_name().trim().to_owned();
                    if busy() || name.is_empty() { return; }
                    busy.set(true);
                    spawn(async move {
                        match git_request::<BrowserRepository>("createBranch", json!({ "ref": name, "checkout": true })).await {
                            Ok(_) => {
                                branch_name.set(String::new());
                                branch_open.set(false);
                                refresh += 1;
                                on_workspace_changed.call(());
                                notice.set(Some(Notice::success("Created and checked out the branch.")));
                            }
                            Err(error) => notice.set(Some(Notice::error(error))),
                        }
                        busy.set(false);
                    });
                },
                DialogForm {
                    Field {
                        control_id: "guest-git-branch-name",
                        label: "Branch name",
                        description: "Use a Git ref name such as feature/browser-git.",
                        required: true,
                        TextInput {
                            value: branch_name(),
                            name: "branch-name",
                            autocomplete: "off",
                            autofocus: true,
                            placeholder: "feature/browser-git",
                            oninput: move |event: FormEvent| branch_name.set(event.value()),
                        }
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |event: MouseEvent| { event.prevent_default(); branch_open.set(false); },
                        }
                        Button {
                            label: if busy() { "Creating…" } else { "Create branch" },
                            kind: ButtonKind::Primary,
                            disabled: busy() || branch_name().trim().is_empty(),
                            onclick: move |_| {},
                        }
                    }
                }
            }
        }
    }
}

fn run_change_action(
    method: &'static str,
    paths: Vec<String>,
    mut busy: Signal<bool>,
    mut refresh: Signal<u64>,
    mut notice: Signal<Option<Notice>>,
) {
    if paths.is_empty() || busy() {
        return;
    }
    busy.set(true);
    spawn(async move {
        match git_request::<BrowserRepository>(method, json!(paths)).await {
            Ok(_) => refresh += 1,
            Err(error) => notice.set(Some(Notice::error(error))),
        }
        busy.set(false);
    });
}

async fn git_request<T>(method: &str, payload: Value) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let mut eval = document::eval(
        r#"
        const request = await dioxus.recv();
        let bridge = globalThis.SyntaxisGuestGit;
        for (let attempt = 0; !bridge && attempt < 200; attempt += 1) {
          await new Promise((resolve) => setTimeout(resolve, 25));
          bridge = globalThis.SyntaxisGuestGit;
        }
        if (!bridge) {
          await dioxus.send({ ok: false, error: "The browser Git bridge is unavailable." });
        } else if (typeof bridge[request.method] !== "function") {
          await dioxus.send({ ok: false, error: `Unknown browser Git operation: ${request.method}` });
        } else {
          try {
            const value = request.method === "diff"
              ? await bridge.diff(request.payload[0], request.payload[1])
              : await bridge[request.method](request.payload);
            await dioxus.send({ ok: true, value });
          } catch (error) {
            const message = error?.message ?? String(error);
            console.error(`Syntaxis browser Git operation failed: ${message}`);
            await dioxus.send({ ok: false, error: message });
          }
        }
        "#,
    );
    eval.send(GitBridgeRequest {
        method: method.to_owned(),
        payload,
    })
    .map_err(|error| format!("Could not start browser Git: {error}"))?;
    let response = eval
        .recv::<GitBridgeResponse<T>>()
        .await
        .map_err(|error| format!("Browser Git did not return a valid response: {error}"))?;
    if response.ok {
        response
            .value
            .ok_or_else(|| "Browser Git returned no result.".to_owned())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "Browser Git operation failed.".to_owned()))
    }
}
