use std::collections::BTreeSet;

use syntaxis_editor::EditorConfigSource;
use syntaxis_git::RepositoryStatus;
use syntaxis_workspace::{EntryKind, FileEntry, FileSession, RelativePath, WorkspaceRecord};

use crate::{git::api as git_api, workspace::client as workspace_client};

const MAX_EDITOR_CONFIG_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct InitialFiles {
    pub workspace: WorkspaceRecord,
    pub entries: Vec<FileEntry>,
    pub editor_configs: Vec<EditorConfigSource>,
    pub git_status: Option<RepositoryStatus>,
    pub ignored_paths: BTreeSet<String>,
    pub session: FileSession,
}

pub(super) async fn load_initial(workspace: WorkspaceRecord) -> Result<InitialFiles, String> {
    let entries = workspace_client::list_files(workspace.clone(), RelativePath::root()).await?;
    let mut editor_configs = Vec::new();
    if entries
        .iter()
        .any(|entry| entry.name == ".editorconfig" && entry.kind == EntryKind::File)
    {
        if let Ok(config) = workspace_client::read_text(
            workspace.clone(),
            RelativePath::try_from(".editorconfig").map_err(|error| error.message)?,
            MAX_EDITOR_CONFIG_BYTES,
        )
        .await
        {
            editor_configs.push(EditorConfigSource {
                directory: String::new(),
                contents: config.content,
            });
        }
    }
    let (git_status, ignored_paths, session) = futures_util::join!(
        git_api::repository_status(workspace.id.0.clone()),
        git_api::ignored_paths(workspace.id.0.clone()),
        workspace_client::load_workspace_session(workspace.id.0.clone()),
    );
    Ok(InitialFiles {
        workspace,
        entries,
        editor_configs,
        git_status: git_status.ok(),
        ignored_paths: ignored_paths.unwrap_or_default().into_iter().collect(),
        session: session.unwrap_or_default().files,
    })
}
