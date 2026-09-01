use std::collections::BTreeMap;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use syntaxis_workspace::{
    EntryKind, RelativePath, WorkspaceFiles, WorkspaceRecord,
    is_bulky_generated_directory_name,
};
use syntaxis_workspace_browser::OpfsWorkspaceFiles;

use super::Notice;

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
        section { class: "guest-git", "aria-label": "Browser source history",
            header { class: "guest-module-header",
                div {
                    h2 { "Source control" }
                    p { "Offline browser snapshots for this workspace." }
                }
                button {
                    r#type: "button",
                    disabled: busy(),
                    onclick: move |_| refresh += 1,
                    "Refresh"
                }
            }
            p { class: "guest-module-note",
                "Browser history provides local commits and restore points. Git branches, remotes, signing, rebase, and interoperable .git data require a server-side Git runtime and are intentionally unavailable here."
            }
            match state() {
                None => rsx! {
                    p { class: "guest-module-note", "Reading workspace history…" }
                },
                Some(Err(error)) => rsx! {
                    p { class: "guest-ai-error", role: "alert", "{error}" }
                },
                Some(Ok((history, status, git_metadata_present))) => {
                    let commit_disabled = busy()
                        || dirty
                        || message().trim().is_empty()
                        || (status.is_clean() && !history.commits.is_empty());

                    rsx! {
                        div { class: "guest-git-summary",
                            strong {
                                if git_metadata_present {
                                    "Git metadata detected"
                                } else if history.commits.is_empty() {
                                    "Browser history not initialized"
                                } else if status.is_clean() {
                                    "Working tree clean"
                                } else {
                                    "{status.count()} changed path(s)"
                                }
                            }
                            span { "{history.commits.len()} local commit(s)" }
                        }
                        if git_metadata_present {
                            p { class: "guest-module-note",
                                "This is a Git working tree. Browser snapshots remain available, but branches, remotes, and .git mutations are disabled because the static guest has no Git runtime."
                            }
                        }
                        if !status.is_clean() {
                            div { class: "guest-git-changes",
                                for path in status.added.iter() {
                                    p { key: "a-{path}",
                                        span { class: "guest-git-added", "A" }
                                        "{path}"
                                    }
                                }
                                for path in status.modified.iter() {
                                    p { key: "m-{path}",
                                        span { class: "guest-git-modified", "M" }
                                        "{path}"
                                    }
                                }
                                for path in status.deleted.iter() {
                                    p { key: "d-{path}",
                                        span { class: "guest-git-deleted", "D" }
                                        "{path}"
                                    }
                                }
                            }
                        }
                        form {
                            class: "guest-git-commit",
                            onsubmit: {
                                let workspace = workspace.clone();
                                move |event| {
                                    event.prevent_default();
                                    let commit_message = message().trim().to_owned();
                                    if commit_message.is_empty() || busy() || dirty {
                                        return;
                                    }
                                    busy.set(true);
                                    let workspace = workspace.clone();
                                    spawn(async move {
                                        match create_commit(&files, &workspace, commit_message).await {
                                            Ok(()) => {
                                                message.set(String::new());
                                                refresh += 1;
                                                notice
                                                    .set(
                                                        Some(Notice::success("Created a local browser commit.")),
                                                    );
                                            }
                                            Err(error) => notice.set(Some(Notice::error(error))),
                                        }
                                        busy.set(false);
                                    });
                                }
                            },
                            input {
                                value: message,
                                maxlength: 200,
                                placeholder: "Commit message",
                                aria_label: "Browser commit message",
                                oninput: move |event| message.set(event.value()),
                            }
                            button { disabled: commit_disabled,
                                if history.commits.is_empty() {
                                    "Initialize history"
                                } else {
                                    "Commit all"
                                }
                            }
                        }
                        if dirty {
                            p { class: "guest-ai-error", "Save the active editor buffer before committing or restoring." }
                        }
                        div { class: "guest-git-history",
                            h3 { "Local commits" }
                            if history.commits.is_empty() {
                                p { class: "guest-module-note",
                                    "No commits yet. Add a message to capture the current workspace."
                                }
                            }
                            for commit in history.commits.iter().rev() {
                                article { key: "{commit.id}",
                                    div {
                                        strong { "{commit.message}" }
                                        small {
                                            "Snapshot {commit.id} · {commit.files.iter().filter(|entry| !entry.directory).count()} file(s)"
                                        }
                                    }
                                    button {
                                        r#type: "button",
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
                                                            notice
                                                                .set(Some(Notice::success("Restored the browser commit.")));
                                                        }
                                                        Err(error) => notice.set(Some(Notice::error(error))),
                                                    }
                                                    busy.set(false);
                                                });
                                            }
                                        },
                                        if confirm_restore().as_deref() == Some(commit.id.as_str()) {
                                            "Confirm restore"
                                        } else {
                                            "Restore"
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

async fn has_git_metadata(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
) -> bool {
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
                    && is_bulky_generated_directory_name(&entry.name))
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
                "Restore failed and the previous workspace was recovered: {error}"
            )),
            Err(rollback_error) => Err(format!(
                "Restore failed ({error}) and rollback was incomplete ({rollback_error}). Import a recent ZIP backup before continuing."
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
                && is_bulky_generated_directory_name(&entry.name))
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
