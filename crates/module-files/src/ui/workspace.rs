//! Shared Files initialization projection.

use std::collections::BTreeSet;

use syntaxis_editor::EditorConfigSource;
use syntaxis_git::RepositoryStatus;
use crate::FilesPorts;
use syntaxis_workspace::{FileEntry, FileSession, WorkspaceRecord};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct InitialFiles {
    pub workspace: WorkspaceRecord,
    pub entries: Vec<FileEntry>,
    pub editor_configs: Vec<EditorConfigSource>,
    pub git_status: Option<RepositoryStatus>,
    pub ignored_paths: BTreeSet<String>,
    pub session: FileSession,
}

pub(super) async fn load_initial(
    ports: &FilesPorts,
    workspace: WorkspaceRecord,
) -> Result<InitialFiles, String> {
    let initialization_workspace = workspace.clone();
    let git = ports.git().cloned();
    let git_state = async move {
        let Some(git) = git else {
            return (None, BTreeSet::new());
        };
        let (status, ignored) = futures_util::join!(
            git.status(&workspace),
            git.ignored_paths(&workspace),
        );
        (
            status.ok(),
            ignored
                .unwrap_or_default()
                .into_iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
        )
    };
    let (initialized, (git_status, ignored_paths)) = futures_util::join!(
        crate::load_files_initialization(ports, initialization_workspace),
        git_state,
    );
    let initialized = initialized.map_err(|error| error.message)?;
    Ok(InitialFiles {
        workspace: initialized.workspace,
        entries: initialized.entries,
        editor_configs: initialized.editor_configs,
        git_status,
        ignored_paths,
        session: initialized.session,
    })
}
