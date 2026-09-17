use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use syntaxis_app_contracts::AppError;
use syntaxis_editor::{BufferStatus, EditorConfig, ExternalChange, apply_editor_config};
use syntaxis_workspace::{FileVersion, RelativePath, WorkspaceRecord};

use crate::{FilesPorts, OpenDocument, files_error};

/// Maximum UTF-8 text document size accepted by the canonical Files controller.
pub const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;

struct TextSaveSnapshot {
    source: String,
    config: EditorConfig,
    version: FileVersion,
}

fn save_snapshot(path: &str, documents: Signal<Vec<OpenDocument>>) -> Option<TextSaveSnapshot> {
    documents.read().iter().find_map(|document| match document {
        OpenDocument::Text(buffer) if buffer.path == path => Some(TextSaveSnapshot {
            source: buffer.contents.clone(),
            config: buffer.config.clone(),
            version: buffer.version.clone(),
        }),
        _ => None,
    })
}

/// Reloads one open text document from its authoritative workspace source.
///
/// # Errors
///
/// Returns a typed path or workspace I/O error when the document cannot be read.
pub async fn reload_text_document(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    path: &str,
    mut documents: Signal<Vec<OpenDocument>>,
) -> Result<(), AppError> {
    let relative = RelativePath::try_from(path.to_owned()).map_err(files_error)?;
    let file = files
        .files()
        .read_text(workspace, &relative, MAX_TEXT_BYTES)
        .await
        .map_err(files_error)?;
    if let Some(OpenDocument::Text(buffer)) = documents
        .write()
        .iter_mut()
        .find(|document| document.path() == path)
    {
        buffer.mark_saved(file.content, file.version);
    }
    Ok(())
}

/// Reconciles one open text buffer with an external workspace change.
///
/// Pending save echoes are ignored. Other read failures mark an existing buffer conflicted.
///
/// # Errors
///
/// Returns a typed workspace error when an existing buffer can no longer be read.
pub async fn reconcile_text_document(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    path: &str,
    mut documents: Signal<Vec<OpenDocument>>,
) -> Result<ExternalChange, AppError> {
    let relative = RelativePath::try_from(path.to_owned()).map_err(files_error)?;
    match files
        .files()
        .read_text(workspace, &relative, MAX_TEXT_BYTES)
        .await
    {
        Ok(file) => {
            let outcome = match documents
                .write()
                .iter_mut()
                .find(|document| document.path() == path)
            {
                Some(OpenDocument::Text(buffer)) => {
                    buffer.reconcile_external(file.content, file.version)
                }
                Some(
                    OpenDocument::Image { .. }
                    | OpenDocument::Large { .. }
                    | OpenDocument::Unsupported { .. },
                )
                | None => ExternalChange::Unchanged,
            };
            Ok(outcome)
        }
        Err(error) => {
            let should_report = documents
                .write()
                .iter_mut()
                .find_map(|document| match document {
                    OpenDocument::Text(buffer) if buffer.path == path => {
                        if buffer.has_pending_save() {
                            Some(false)
                        } else {
                            buffer.status = BufferStatus::Conflict;
                            Some(true)
                        }
                    }
                    _ => None,
                })
                .unwrap_or(false);
            if should_report {
                Err(files_error(error))
            } else {
                Ok(ExternalChange::Unchanged)
            }
        }
    }
}

/// Saves one text document with its last known [`FileVersion`].
///
/// The buffer enters a pending-save state before I/O. A failed compare-and-set write clears that
/// state and marks the buffer conflicted.
///
/// # Errors
///
/// Returns a typed path, version-conflict, or workspace I/O error.
pub async fn save_text_document(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    path: &str,
    mut documents: Signal<Vec<OpenDocument>>,
) -> Result<(), AppError> {
    let Some(snapshot) = save_snapshot(path, documents) else {
        return Ok(());
    };
    let contents = apply_editor_config(&snapshot.source, &snapshot.config);
    let relative = RelativePath::try_from(path.to_owned()).map_err(files_error)?;
    if let Some(OpenDocument::Text(current)) = documents
        .write()
        .iter_mut()
        .find(|document| document.path() == path)
    {
        current.begin_save(contents.clone());
    }
    let result = files
        .files()
        .write_text(
            workspace,
            &relative,
            &contents,
            Some(&snapshot.version),
            MAX_TEXT_BYTES,
        )
        .await;
    match result {
        Ok(version) => {
            if let Some(OpenDocument::Text(current)) = documents
                .write()
                .iter_mut()
                .find(|document| document.path() == path)
            {
                current.finish_save(contents, version);
            }
            Ok(())
        }
        Err(error) => {
            if let Some(OpenDocument::Text(current)) = documents
                .write()
                .iter_mut()
                .find(|document| document.path() == path)
            {
                current.cancel_save();
                current.status = BufferStatus::Conflict;
            }
            Err(files_error(error))
        }
    }
}

/// Saves selected dirty text documents sequentially, stopping at the first failure.
///
/// # Errors
///
/// Returns the first typed save error. Documents saved before that error retain their successful
/// clean state.
pub async fn save_text_documents(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    paths: &[String],
    documents: Signal<Vec<OpenDocument>>,
) -> Result<(), AppError> {
    for path in paths {
        let is_dirty = documents.read().iter().any(
            |document| matches!(document, OpenDocument::Text(buffer) if buffer.path == path.as_str() && buffer.is_dirty()),
        );
        if is_dirty {
            save_text_document(files, workspace, path, documents).await?;
        }
    }
    Ok(())
}
