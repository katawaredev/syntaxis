use syntaxis_app_contracts::AppError;
use syntaxis_editor::EditorConfigSource;
use syntaxis_workspace::{FileEntry, FileSession, RelativePath, WorkspaceRecord};

use crate::{FilesPorts, load_files_directory};

/// Files state loaded before the canonical controller restores its reactive state.
#[derive(Clone, Debug, PartialEq)]
pub struct FilesInitialization {
    pub workspace: WorkspaceRecord,
    pub entries: Vec<FileEntry>,
    pub editor_configs: Vec<EditorConfigSource>,
    pub session: FileSession,
}

/// Loads the runtime-neutral portion of Files startup through the injected port bundle.
///
/// A root `.editorconfig` is optional: unreadable configuration does not prevent opening the
/// workspace, matching the canonical application behavior.
///
/// # Errors
///
/// Returns a typed Files error when the root listing or persisted session cannot be loaded.
pub async fn load_files_initialization(
    files: &FilesPorts,
    workspace: WorkspaceRecord,
) -> Result<FilesInitialization, AppError> {
    let loaded = load_files_directory(files, &workspace, RelativePath::root()).await?;
    let session = files
        .session()
        .load(&workspace.id)
        .await
        .map_err(|mut error| {
            error.source = syntaxis_app_contracts::ErrorSource::Files;
            error
        })?;
    let editor_configs = loaded.editor_config.into_iter().collect();
    Ok(FilesInitialization {
        workspace,
        entries: loaded.entries,
        editor_configs,
        session,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_lite::future::block_on;
    use syntaxis_workspace::{
        MockWorkspaceFiles, WorkspaceAvailability, WorkspaceIcon, WorkspaceIconSymbol, WorkspaceId,
        WorkspaceProfile, WorkspaceSection,
    };

    use crate::{FixedWorkspaceSearch, MemoryFilesSession};

    use super::*;

    fn workspace() -> WorkspaceRecord {
        WorkspaceRecord {
            id: WorkspaceId::new("files-initialization"),
            slug: "files-initialization".into(),
            name: "Files initialization".into(),
            root: "/files-initialization".into(),
            icon: WorkspaceIcon::Symbol {
                name: WorkspaceIconSymbol::Folder,
            },
            profile: WorkspaceProfile::default(),
            registered_at_unix_ms: 0,
            last_opened_unix_ms: 0,
            last_section: WorkspaceSection::Files,
            availability: WorkspaceAvailability::Available,
        }
    }

    #[test]
    fn startup_loads_root_entries_editor_config_and_session() {
        let workspace = workspace();
        let adapter = Arc::new(MockWorkspaceFiles::default());
        adapter
            .insert_text(
                &workspace,
                &RelativePath::try_from(".editorconfig").unwrap(),
                "root = true",
            )
            .unwrap();
        adapter
            .insert_text(
                &workspace,
                &RelativePath::try_from("src/main.rs").unwrap(),
                "fn main() {}",
            )
            .unwrap();
        let files = FilesPorts::new(
            adapter,
            Arc::new(FixedWorkspaceSearch::default()),
            Arc::new(MemoryFilesSession::default()),
        );

        let initialized = block_on(load_files_initialization(&files, workspace))
            .expect("startup state should load");
        assert_eq!(initialized.entries.len(), 2);
        assert_eq!(initialized.editor_configs.len(), 1);
        assert_eq!(initialized.editor_configs[0].contents, "root = true");
        assert_eq!(initialized.session, FileSession::default());
    }
}
