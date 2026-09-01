use super::Notice;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, Icon, RepositoryEmptyDetail, RepositoryPanelHeader,
    RepositoryPathRow, RepositoryShell, RepositorySidebarTabs, RepositorySidebarView,
};
use syntaxis_workspace::{
    EntryKind, RelativePath, WorkspaceFiles, WorkspaceRecord, is_bulky_generated_directory_name,
};
use syntaxis_workspace_browser::OpfsWorkspaceFiles;
pub(super) const HISTORY_PATH: &str = ".syntaxis-guest-history.json";
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_HISTORY_BYTES: usize = 24 * 1024 * 1024;
const MAX_COMMITS: usize = 8;
mod base64_bytes {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use serde::{Deserialize, Deserializer, Serializer};
    pub(super) fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&BASE64.encode(bytes))
    }
    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        BASE64.decode(encoded).map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SnapshotEntry {
    path: String,
    directory: bool,
    #[serde(with = "base64_bytes")]
    content: Vec<u8>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct BrowserCommit {
    id: String,
    message: String,
    files: Vec<SnapshotEntry>,
}
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct BrowserHistory {
    commits: Vec<BrowserCommit>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BrowserStatus {
    added: Vec<String>,
    modified: Vec<String>,
    deleted: Vec<String>,
}
impl BrowserStatus {
    fn is_clean(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }
    fn count(&self) -> usize {
        self.added.len() + self.modified.len() + self.deleted.len()
    }
}
#[component]
pub(super) fn GuestGit(
    workspace: WorkspaceRecord,
    revision: Signal<u64>,
    dirty: bool,
    mut notice: Signal<Option<Notice>>,
    on_workspace_changed: EventHandler<()>,
) -> Element {
    let files = OpfsWorkspaceFiles;
    let mut refresh = use_signal(|| 0_u64);
    let mut message = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut confirm_restore = use_signal(|| None::<String>);
    let mut sidebar_view = use_signal(RepositorySidebarView::default);
    let mut selected_path = use_signal(|| None::<String>);
    let mut selected_commit_id = use_signal(|| None::<String>);
    let state_workspace = workspace.clone();
    let state = use_resource(move || {
        let workspace = state_workspace.clone();
        let _workspace_revision = revision();
        let _refresh = refresh();
        async move {
            let history = load_history(&files, &workspace).await?;
            let git_metadata_present = has_git_metadata(&files, &workspace).await;
            let current = collect_snapshot(&files, &workspace).await?;
            let status = compare_snapshot(
                history.commits.last().map(|commit| commit.files.as_slice()),
                &current,
            );
            Ok::<_, String>((history, status, git_metadata_present))
        }
    });
    rsx! {
        match state() {
            None => rsx! {
                div { class: "flex size-full items-center justify-center text-xs text-muted-foreground",
                    "Reading workspace history…"
                }
            },
            Some(Err(error)) => rsx! {
                div { class: "flex size-full items-center justify-center p-6 text-xs text-destructive",
                    "{error}"
                }
            },
            Some(Ok((history, status, git_metadata_present))) => {
                let commit_disabled = busy() || dirty || message().trim().is_empty()
                    || (status.is_clean() && !history.commits.is_empty());
                let selected_commit = selected_commit_id().and_then(|id| {
                    history.commits.iter().find(|commit| commit.id == id).cloned()
                });
                rsx! {
                    RepositoryShell {
                        sidebar: rsx! {
                            RepositorySidebarTabs {
                                active: sidebar_view(),
                                changes: status.count(),
                                on_change: move |view| sidebar_view.set(view),
                            }
                            div { class: "min-h-0 flex-1 overflow-y-auto p-1.25",
                                if sidebar_view() == RepositorySidebarView::Changes {
                                    if status.is_clean() {
                                        div { class: "flex h-full min-h-40 items-center justify-center p-4 text-center text-xs text-muted-foreground",
                                            "Working tree clean."
                                        }
                                    }
                                    for (path, badge, tone) in status.added.iter().map(|path| (path, "A", "text-success"))
                                        .chain(status.modified.iter().map(|path| (path, "M", "text-warning")))
                                        .chain(status.deleted.iter().map(|path| (path, "D", "text-destructive")))
                                    {
                                        RepositoryPathRow {
                                            key: "{badge}-{path}",
                                            path: path.clone(),
                                            status: badge,
                                            tone_class: tone,
                                            active: selected_path().as_deref() == Some(path.as_str()),
                                            onclick: {
                                                let path = path.clone();
                                                move |()| selected_path.set(Some(path.clone()))
                                            },
                                        }
                                    }
                                } else if history.commits.is_empty() {
                                    div { class: "flex h-full min-h-40 items-center justify-center p-4 text-center text-xs text-muted-foreground",
                                        "No browser snapshots yet."
                                    }
                                } else {
                                    for commit in history.commits.iter().rev() {
                                        button {
                                            key: "{commit.id}",
                                            class: if selected_commit_id().as_deref() == Some(commit.id.as_str()) {
                                                "flex w-full items-center gap-2 rounded-sm bg-accent px-2 py-2 text-left text-xs"
                                            } else {
                                                "flex w-full items-center gap-2 rounded-sm px-2 py-2 text-left text-xs hover:bg-accent/65"
                                            },
                                            onclick: {
                                                let id = commit.id.clone();
                                                move |_| selected_commit_id.set(Some(id.clone()))
                                            },
                                                Icon { icon: AppIcon::Commit, size: 14 }
                                            span { class: "min-w-0 flex-1",
                                                strong { class: "block truncate", "{commit.message}" }
                                                small { class: "text-[9px] text-muted-foreground", "Snapshot {commit.id}" }
                                            }
                                        }
                                    }
                                }
                            }
                            if sidebar_view() == RepositorySidebarView::Changes {
                                form {
                                    class: "grid grid-cols-[minmax(0,1fr)_auto] gap-1.5 border-t border-border p-2",
                                    onsubmit: {
                                        let workspace = workspace.clone();
                                        move |event| {
                                            event.prevent_default();
                                            let commit_message = message().trim().to_owned();
                                            if commit_message.is_empty() || commit_disabled { return; }
                                            busy.set(true);
                                            let workspace = workspace.clone();
                                            spawn(async move {
                                                match create_commit(&files, &workspace, commit_message).await {
                                                    Ok(()) => {
                                                        message.set(String::new());
                                                        refresh += 1;
                                                        notice.set(Some(Notice::success("Created a browser snapshot.")));
                                                    }
                                                    Err(error) => notice.set(Some(Notice::error(error))),
                                                }
                                                busy.set(false);
                                            });
                                        }
                                    },
                                    input {
                                        class: "h-8 min-w-0 rounded-md border border-input bg-background px-2 text-xs outline-none",
                                        value: message,
                                        placeholder: "Commit message",
                                        oninput: move |event| message.set(event.value()),
                                    }
                                    Button {
                                        label: if history.commits.is_empty() { "Initialize" } else { "Commit" },
                                        kind: ButtonKind::Primary,
                                        disabled: commit_disabled,
                                        onclick: move |_| {},
                                    }
                                }
                            }
                        },
                        header: rsx! {
                            RepositoryPanelHeader {
                                title: if git_metadata_present { "browser/git" } else { "browser/history" },
                                subtitle: Some(format!("{} snapshot(s)", history.commits.len())),
                                actions: rsx! {
                                    Button {
                                        label: "Refresh",
                                        kind: ButtonKind::Ghost,
                                        disabled: busy(),
                                        onclick: move |_| refresh += 1,
                                    }
                                },
                            }
                        },
                        detail: rsx! {
                            if sidebar_view() == RepositorySidebarView::Changes {
                                if let Some(path) = selected_path() {
                                    div { class: "p-5",
                                        h3 { class: "text-sm font-medium", "{path}" }
                                        p { class: "mt-2 text-xs text-muted-foreground",
                                            "Browser snapshots track this path. Git-generated diffs require the native Git runtime."
                                        }
                                    }
                                } else {
                                    RepositoryEmptyDetail { message: "Select a changed file to inspect its browser snapshot status." }
                                }
                            } else if let Some(commit) = selected_commit {
                                div { class: "flex h-full flex-col",
                                    div { class: "flex items-center gap-3 border-b border-border p-3",
                                        div { class: "min-w-0 flex-1",
                                            h3 { class: "truncate text-sm font-medium", "{commit.message}" }
                                            p { class: "text-[10px] text-muted-foreground",
                                                "Snapshot {commit.id} · {commit.files.iter().filter(|entry| !entry.directory).count()} files"
                                            }
                                        }
                                        Button {
                                            label: if confirm_restore().as_deref() == Some(commit.id.as_str()) { "Confirm restore" } else { "Restore" },
                                            kind: ButtonKind::Danger,
                                            disabled: busy() || dirty,
                                            onclick: {
                                                let workspace = workspace.clone();
                                                let commit = commit.clone();
                                                move |_| {
                                                    if confirm_restore().as_deref() != Some(commit.id.as_str()) {
                                                        confirm_restore.set(Some(commit.id.clone()));
                                                        return;
                                                    }
                                                    busy.set(true);
                                                    confirm_restore.set(None);
                                                    let workspace = workspace.clone();
                                                    let commit = commit.clone();
                                                    spawn(async move {
                                                        match restore_snapshot(&files, &workspace, &commit.files).await {
                                                            Ok(()) => {
                                                                refresh += 1;
                                                                on_workspace_changed.call(());
                                                                notice.set(Some(Notice::success("Restored the browser snapshot.")));
                                                            }
                                                            Err(error) => notice.set(Some(Notice::error(error))),
                                                        }
                                                        busy.set(false);
                                                    });
                                                }
                                            },
                                        }
                                    }
                                    RepositoryEmptyDetail { message: "Browser snapshot selected." }
                                }
                            } else {
                                RepositoryEmptyDetail { message: "Select a browser snapshot to inspect it." }
                            }
                        },
                    }
                }
            },
        }
    }
}
async fn create_commit(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
    message: String,
) -> Result<(), String> {
    let snapshot = collect_snapshot(files, workspace).await?;
    let mut history = load_history(files, workspace).await?;
    if history.commits.is_empty()
        || !compare_snapshot(
            history.commits.last().map(|commit| commit.files.as_slice()),
            &snapshot,
        )
        .is_clean()
    {
        history.commits.push(BrowserCommit {
            id: format!("{:.0}", js_sys::Date::now()),
            message,
            files: snapshot,
        });
    } else {
        return Err("There are no workspace changes to commit.".to_owned());
    }
    if history.commits.len() > MAX_COMMITS {
        let remove = history.commits.len() - MAX_COMMITS;
        history.commits.drain(0..remove);
    }
    loop {
        let encoded = serde_json::to_vec(&history)
            .map_err(|error| format!("Could not encode browser history: {error}"))?;
        if encoded.len() <= MAX_HISTORY_BYTES {
            let path =
                RelativePath::try_from(HISTORY_PATH.to_owned()).map_err(|error| error.message)?;
            files
                .write_binary(
                    workspace,
                    &path,
                    &encoded,
                    u64::try_from(MAX_HISTORY_BYTES).unwrap_or(u64::MAX),
                )
                .await
                .map_err(|error| error.message)?;
            return Ok(());
        }
        if history.commits.len() <= 1 {
            return Err("The workspace snapshot is too large for browser history.".to_owned());
        }
        history.commits.remove(0);
    }
}
async fn has_git_metadata(files: &OpfsWorkspaceFiles, workspace: &WorkspaceRecord) -> bool {
    let Ok(path) = RelativePath::try_from(".git".to_owned()) else {
        return false;
    };
    files
        .stat(workspace, &path)
        .await
        .is_ok_and(|entry| entry.kind == EntryKind::Directory || entry.kind == EntryKind::File)
}
async fn load_history(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
) -> Result<BrowserHistory, String> {
    let path = RelativePath::try_from(HISTORY_PATH.to_owned()).map_err(|error| error.message)?;
    match files
        .read_binary(
            workspace,
            &path,
            u64::try_from(MAX_HISTORY_BYTES).unwrap_or(u64::MAX),
        )
        .await
    {
        Ok(file) => serde_json::from_slice(&file.content)
            .map_err(|error| format!("Browser history is damaged: {error}")),
        Err(error) if history_is_missing(&error) => Ok(BrowserHistory::default()),
        Err(error) => Err(error.message),
    }
}
fn history_is_missing(error: &syntaxis_workspace::WorkspaceError) -> bool {
    if error.code == syntaxis_workspace::ErrorCode::NotFound {
        return true;
    }
    let message = error.message.to_ascii_lowercase();
    message.contains("not found")
        || message.contains("not be found")
        || message.contains("could not open the file entry")
}
async fn collect_snapshot(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
) -> Result<Vec<SnapshotEntry>, String> {
    let mut pending = vec![RelativePath::root()];
    let mut snapshot = Vec::new();
    let mut total = 0_u64;
    while let Some(directory) = pending.pop() {
        for entry in files
            .list(workspace, &directory)
            .await
            .map_err(|error| error.message)?
        {
            if entry.path.as_str() == HISTORY_PATH
                || (entry.kind == EntryKind::Directory
                    && (entry.name == ".git" || is_bulky_generated_directory_name(&entry.name)))
            {
                continue;
            }
            match entry.kind {
                EntryKind::Directory => {
                    snapshot.push(SnapshotEntry {
                        path: entry.path.as_str().to_owned(),
                        directory: true,
                        content: Vec::new(),
                    });
                    pending.push(entry.path);
                }
                EntryKind::File => {
                    let file = files
                        .read_binary(workspace, &entry.path, MAX_FILE_BYTES)
                        .await
                        .map_err(|error| error.message)?;
                    total =
                        total.saturating_add(u64::try_from(file.content.len()).unwrap_or(u64::MAX));
                    if total > MAX_SNAPSHOT_BYTES {
                        return Err(
                            "The workspace exceeds the 16 MiB browser-history limit.".to_owned()
                        );
                    }
                    snapshot.push(SnapshotEntry {
                        path: entry.path.as_str().to_owned(),
                        directory: false,
                        content: file.content,
                    });
                }
                EntryKind::Symlink => {}
            }
        }
    }
    snapshot.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(snapshot)
}
fn compare_snapshot(
    previous: Option<&[SnapshotEntry]>,
    current: &[SnapshotEntry],
) -> BrowserStatus {
    let previous = previous
        .into_iter()
        .flatten()
        .filter(|entry| !entry.directory)
        .map(|entry| (entry.path.as_str(), entry.content.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let current = current
        .iter()
        .filter(|entry| !entry.directory)
        .map(|entry| (entry.path.as_str(), entry.content.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let mut status = BrowserStatus::default();
    for (path, content) in &current {
        match previous.get(path) {
            None => status.added.push((*path).to_owned()),
            Some(old) if old != content => status.modified.push((*path).to_owned()),
            Some(_) => {}
        }
    }
    for path in previous.keys() {
        if !current.contains_key(path) {
            status.deleted.push((*path).to_owned());
        }
    }
    status
}
async fn restore_snapshot(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
    snapshot: &[SnapshotEntry],
) -> Result<(), String> {
    let rollback = collect_snapshot(files, workspace).await?;
    if let Err(error) = apply_snapshot(files, workspace, snapshot).await {
        return match apply_snapshot(files, workspace, &rollback).await {
            Ok(()) => Err(format!(
                "Restore failed and the previous workspace was recovered: {error}",
            )),
            Err(rollback_error) => Err(format!(
                "Restore failed ({error}) and rollback was incomplete ({rollback_error}). Import a recent ZIP backup before continuing.",
            )),
        };
    }
    Ok(())
}
async fn apply_snapshot(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
    snapshot: &[SnapshotEntry],
) -> Result<(), String> {
    for entry in files
        .list(workspace, &RelativePath::root())
        .await
        .map_err(|error| error.message)?
    {
        if entry.path.as_str() == HISTORY_PATH
            || (entry.kind == EntryKind::Directory
                && (entry.name == ".git" || is_bulky_generated_directory_name(&entry.name)))
        {
            continue;
        }
        files
            .delete(workspace, &entry.path)
            .await
            .map_err(|error| error.message)?;
    }
    let mut directories = snapshot
        .iter()
        .filter(|entry| entry.directory)
        .collect::<Vec<_>>();
    directories.sort_by_key(|entry| entry.path.matches('/').count());
    for entry in directories {
        let path = RelativePath::try_from(entry.path.clone()).map_err(|error| error.message)?;
        files
            .create_directory(workspace, &path)
            .await
            .map_err(|error| error.message)?;
    }
    for entry in snapshot.iter().filter(|entry| !entry.directory) {
        let path = RelativePath::try_from(entry.path.clone()).map_err(|error| error.message)?;
        files
            .write_binary(workspace, &path, &entry.content, MAX_FILE_BYTES)
            .await
            .map_err(|error| error.message)?;
    }
    Ok(())
}
