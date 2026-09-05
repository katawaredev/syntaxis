use syntaxis_app_contracts::AppError;
use syntaxis_editor::EditorConfigSource;
use syntaxis_workspace::{EntryKind, FileEntry, RelativePath, WorkspaceRecord};

use crate::{FilesPorts, MAX_TEXT_BYTES, files_error};

/// One authoritative directory listing and its optional scoped editor configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedDirectory {
    pub path: RelativePath,
    pub entries: Vec<FileEntry>,
    pub editor_config: Option<EditorConfigSource>,
}

/// Loads a directory and discovers a `.editorconfig` located directly inside it.
///
/// An unreadable editor configuration is ignored without hiding an otherwise valid listing.
///
/// # Errors
///
/// Returns a typed Files error when the directory cannot be listed.
pub async fn load_files_directory(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    path: RelativePath,
) -> Result<LoadedDirectory, AppError> {
    let entries = files
        .files()
        .list(workspace, &path)
        .await
        .map_err(files_error)?;
    let editor_config = if entries
        .iter()
        .any(|entry| entry.name == ".editorconfig" && entry.kind == EntryKind::File)
    {
        let config_path = if path.is_root() {
            ".editorconfig".to_owned()
        } else {
            format!("{}/.editorconfig", path.as_str())
        };
        let config_path = RelativePath::try_from(config_path).map_err(files_error)?;
        files
            .files()
            .read_text(workspace, &config_path, MAX_TEXT_BYTES)
            .await
            .ok()
            .map(|file| EditorConfigSource {
                directory: path.as_str().to_owned(),
                contents: file.content,
            })
    } else {
        None
    };
    Ok(LoadedDirectory {
        path,
        entries,
        editor_config,
    })
}
