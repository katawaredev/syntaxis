//! Canonical Files/Editor module contracts and, incrementally, its shared controller and UI.

use syntaxis_app_contracts::{AppError, ErrorSource};
use syntaxis_workspace::WorkspaceError;

mod clipboard;
mod controller;
mod directory;
mod document_io;
mod documents;
mod filesystem_search;
mod git;
mod initialization;
mod language_services;
mod loading;
mod mutations;
mod ports;
mod query;
mod reference;
mod search;
mod testing;
mod ui;
mod uploads;

pub use clipboard::FilesClipboardPort;
pub use controller::{
    FilesSessionWriter, FilesUiState, FilesWorkspaceEventBatch, FilesWorkspaceEvents,
    use_files_session_writer, use_files_ui_state, use_files_workspace_events,
};
pub use directory::{LoadedDirectory, load_files_directory};
pub use document_io::{
    MAX_TEXT_BYTES, reconcile_text_document, reload_text_document, save_text_document,
    save_text_documents,
};
pub use documents::{
    ActiveBufferMeta, ActiveDocumentView, ActivePathRepair, CloseRequest, FilesController,
    OpenDocument, OpenTab, RestoredDocuments, apply_document_edits, close_documents,
    merge_restored_documents, rename_documents, repaired_active_path, request_close,
    request_close_many, revert_text_document, use_files_controller,
};
pub use filesystem_search::{FilesystemWorkspaceSearch, SearchLimits};
pub use git::FileGitPort;
pub use initialization::{FilesInitialization, load_files_initialization};
pub use language_services::{LanguageServiceConnection, LanguageServicesPort};
pub use loading::{
    DocumentLoad, MAX_BINARY_PREVIEW_BYTES, image_mime, load_document_content,
    load_restored_document_content,
};
pub use mutations::{
    FileAction, FileActionDialog, FileMutationOutcome, execute_file_action, suggested_destination,
};
pub use ports::{FilesPorts, FilesSessionPort, WorkspaceSearchPort};
pub use query::FilesQuery;
pub use reference::format_file_reference;
pub use search::{
    ContentMatch, PathMatch, SearchMatcher, SearchOccurrence, SearchOptions, SearchRequest,
    SearchResult, SearchResults, SearchScope, TextRange,
};
pub use testing::{FixedWorkspaceSearch, MemoryFilesSession};
pub use ui::{
    FilesView, render_markdown, render_markdown_preserving_newlines, search_workspace_files,
};
pub use uploads::{
    PreparedUpload, UploadCollisionPolicy, UploadPolicy, execute_upload, prepare_upload,
};

fn files_error(error: WorkspaceError) -> AppError {
    let mut error = AppError::from(error);
    error.source = ErrorSource::Files;
    error
}
