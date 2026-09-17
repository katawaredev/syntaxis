use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_workspace::{FileEntry, RelativePath, WorkspaceRecord};

use crate::{FilesPorts, files_error};

/// Workspace entry operation requested by the Files UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileAction {
    CreateFile,
    CreateFolder,
    Move,
    Duplicate,
    Delete,
}

/// UI-independent file action request.
#[derive(Clone, Debug, PartialEq)]
pub struct FileActionDialog {
    pub action: FileAction,
    pub source: Option<String>,
    pub destination_parent: Option<String>,
}

/// Suggests an initial destination for a file mutation dialog.
pub fn suggested_destination(dialog: &FileActionDialog) -> String {
    match dialog.action {
        FileAction::CreateFile => suggested_child_path(dialog, "new_file.txt"),
        FileAction::CreateFolder => suggested_child_path(dialog, "new_folder"),
        FileAction::Move => dialog.source.clone().unwrap_or_default(),
        FileAction::Duplicate => dialog
            .source
            .as_deref()
            .map_or_else(|| "copy".to_owned(), suggested_copy_path),
        FileAction::Delete => String::new(),
    }
}

fn suggested_copy_path(source: &str) -> String {
    let (parent, name) = source.rsplit_once('/').unwrap_or(("", source));
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            (stem, Some(extension))
        }
        _ => (name, None),
    };
    let copy_name = extension.map_or_else(
        || format!("{stem}-copy"),
        |extension| format!("{stem}-copy.{extension}"),
    );
    if parent.is_empty() {
        copy_name
    } else {
        format!("{parent}/{copy_name}")
    }
}

fn suggested_child_path(dialog: &FileActionDialog, name: &str) -> String {
    dialog
        .destination_parent
        .as_deref()
        .map_or_else(|| name.to_owned(), |parent| format!("{parent}/{name}"))
}

/// Authoritative result of a workspace entry mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileMutationOutcome {
    FileCreated(FileEntry),
    DirectoryCreated,
    Copied,
    Moved {
        source: RelativePath,
        destination: RelativePath,
    },
    Deleted {
        source: RelativePath,
    },
}

fn invalid_input(message: &'static str) -> AppError {
    AppError::new(
        AppErrorCode::InvalidInput,
        message,
        RetryAdvice::Never,
        ErrorSource::Files,
    )
}

fn destination_path(
    action: FileAction,
    destination: &str,
) -> Result<Option<RelativePath>, AppError> {
    if action == FileAction::Delete {
        return Ok(None);
    }
    match RelativePath::try_from(destination.trim().to_owned()).map_err(files_error)? {
        path if !path.is_root() => Ok(Some(path)),
        _ => Err(invalid_input("Choose a non-root path.")),
    }
}

fn source_path(source: Option<&str>) -> Result<Option<RelativePath>, AppError> {
    source
        .map(|source| {
            match RelativePath::try_from(source.trim().to_owned()).map_err(files_error)? {
                path if !path.is_root() => Ok(path),
                _ => Err(invalid_input("Choose a non-root workspace item.")),
            }
        })
        .transpose()
}

fn validate_relocation(source: &RelativePath, destination: &RelativePath) -> Result<(), AppError> {
    if destination == source
        || destination
            .as_str()
            .strip_prefix(source.as_str())
            .is_some_and(|suffix| suffix.starts_with('/'))
    {
        return Err(invalid_input(
            "Choose a different destination outside the source.",
        ));
    }
    Ok(())
}

/// Executes a validated workspace entry mutation through the runtime-selected Files port.
///
/// # Errors
///
/// Returns typed validation or workspace I/O errors. Paths are normalized as [`RelativePath`]
/// values before any adapter is called.
pub async fn execute_file_action(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    action: FileAction,
    source: Option<&str>,
    destination: &str,
) -> Result<FileMutationOutcome, AppError> {
    let destination = destination_path(action, destination)?;
    let source = source_path(source)?;
    match action {
        FileAction::CreateFile => {
            let destination = destination.ok_or_else(|| invalid_input("Choose a file path."))?;
            files
                .files()
                .create_file(workspace, &destination)
                .await
                .map(FileMutationOutcome::FileCreated)
                .map_err(files_error)
        }
        FileAction::CreateFolder => {
            let destination = destination.ok_or_else(|| invalid_input("Choose a folder path."))?;
            files
                .files()
                .create_directory(workspace, &destination)
                .await
                .map(|_| FileMutationOutcome::DirectoryCreated)
                .map_err(files_error)
        }
        FileAction::Move | FileAction::Duplicate => {
            let source =
                source.ok_or_else(|| invalid_input("Choose an existing workspace item."))?;
            let destination =
                destination.ok_or_else(|| invalid_input("Choose a destination path."))?;
            validate_relocation(&source, &destination)?;
            if action == FileAction::Move {
                files
                    .files()
                    .move_entry(workspace, &source, &destination)
                    .await
                    .map_err(files_error)?;
                Ok(FileMutationOutcome::Moved {
                    source,
                    destination,
                })
            } else {
                files
                    .files()
                    .copy(workspace, &source, &destination)
                    .await
                    .map_err(files_error)?;
                Ok(FileMutationOutcome::Copied)
            }
        }
        FileAction::Delete => {
            let source =
                source.ok_or_else(|| invalid_input("Choose an existing workspace item."))?;
            files
                .files()
                .delete(workspace, &source)
                .await
                .map_err(files_error)?;
            Ok(FileMutationOutcome::Deleted { source })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_lite::future::block_on;
    use syntaxis_workspace::{
        MockWorkspaceFiles, WorkspaceAvailability, WorkspaceIcon, WorkspaceIconSymbol, WorkspaceId,
        WorkspaceProfile, WorkspaceSection,
    };

    use super::*;

    fn workspace() -> WorkspaceRecord {
        WorkspaceRecord {
            id: WorkspaceId::new("file-mutations"),
            slug: "file-mutations".into(),
            name: "File mutations".into(),
            root: "/file-mutations".into(),
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

    fn ports(files: Arc<MockWorkspaceFiles>) -> FilesPorts {
        FilesPorts::new(
            files,
            Arc::new(crate::FixedWorkspaceSearch::default()),
            Arc::new(crate::MemoryFilesSession::default()),
        )
    }

    #[test]
    fn mutation_destinations_are_trimmed_and_reject_the_root() {
        assert_eq!(
            destination_path(FileAction::CreateFile, " src/main.rs ")
                .expect("path should be valid")
                .expect("create needs a destination")
                .as_str(),
            "src/main.rs"
        );
        let error =
            destination_path(FileAction::Move, "").expect_err("workspace root should be rejected");
        assert_eq!(error.code, AppErrorCode::InvalidInput);
        let error = source_path(Some("  ")).expect_err("workspace root should be rejected");
        assert_eq!(error.code, AppErrorCode::InvalidInput);

        let source = RelativePath::try_from("src").unwrap();
        for destination in ["src", "src/generated/file.rs"] {
            let destination = RelativePath::try_from(destination).unwrap();
            let error = validate_relocation(&source, &destination)
                .expect_err("a source cannot be relocated into itself");
            assert_eq!(error.code, AppErrorCode::InvalidInput);
        }
    }

    #[test]
    fn duplicate_suggestions_preserve_extensions() {
        assert_eq!(
            suggested_destination(&FileActionDialog {
                action: FileAction::Duplicate,
                source: Some("src/archive.tar.gz".to_owned()),
                destination_parent: None,
            }),
            "src/archive.tar-copy.gz"
        );
        assert_eq!(
            suggested_destination(&FileActionDialog {
                action: FileAction::Duplicate,
                source: Some("folder.with-dot/README".to_owned()),
                destination_parent: None,
            }),
            "folder.with-dot/README-copy"
        );
        assert_eq!(
            suggested_destination(&FileActionDialog {
                action: FileAction::Duplicate,
                source: Some(".env".to_owned()),
                destination_parent: None,
            }),
            ".env-copy"
        );
    }

    #[test]
    fn mutations_execute_through_the_files_port() {
        let workspace = workspace();
        let adapter = Arc::new(MockWorkspaceFiles::default());
        let ports = ports(Arc::clone(&adapter));

        block_on(execute_file_action(
            &ports,
            &workspace,
            FileAction::CreateFolder,
            None,
            "src",
        ))
        .expect("directory should be created");
        let created = block_on(execute_file_action(
            &ports,
            &workspace,
            FileAction::CreateFile,
            None,
            "src/main.rs",
        ))
        .expect("file should be created");
        assert!(matches!(created, FileMutationOutcome::FileCreated(_)));

        block_on(execute_file_action(
            &ports,
            &workspace,
            FileAction::Duplicate,
            Some("src/main.rs"),
            "src/copy.rs",
        ))
        .expect("file should be copied");
        let moved = block_on(execute_file_action(
            &ports,
            &workspace,
            FileAction::Move,
            Some("src/copy.rs"),
            "src/moved.rs",
        ))
        .expect("file should be moved");
        assert!(matches!(
            moved,
            FileMutationOutcome::Moved { source, destination }
                if source.as_str() == "src/copy.rs" && destination.as_str() == "src/moved.rs"
        ));

        let deleted = block_on(execute_file_action(
            &ports,
            &workspace,
            FileAction::Delete,
            Some("src"),
            "",
        ))
        .expect("tree should be deleted");
        assert!(matches!(
            deleted,
            FileMutationOutcome::Deleted { source } if source.as_str() == "src"
        ));

        let missing = block_on(execute_file_action(
            &ports,
            &workspace,
            FileAction::Delete,
            Some("src"),
            "",
        ))
        .expect_err("deleting a missing tree should fail");
        assert_eq!(missing.code, AppErrorCode::NotFound);
        assert_eq!(missing.source, ErrorSource::Files);
    }
}
