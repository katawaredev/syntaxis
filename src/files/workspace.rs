use std::collections::BTreeSet;

use syntaxis_editor::EditorConfigSource;
use syntaxis_git::RepositoryStatus;
use syntaxis_workspace::{FileEntry, FileSession, WorkspaceRecord};

use crate::workspace::client as workspace_client;

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
    let bootstrap = workspace_client::workspace_files_bootstrap(workspace.clone()).await?;
    let editor_configs = bootstrap
        .root_editor_config
        .map(|contents| {
            vec![EditorConfigSource {
                directory: String::new(),
                contents,
            }]
        })
        .unwrap_or_default();
    Ok(InitialFiles {
        workspace,
        entries: bootstrap.entries,
        editor_configs,
        git_status: bootstrap.git_status,
        ignored_paths: bootstrap.ignored_paths.into_iter().collect(),
        session: bootstrap.session.files,
    })
}
