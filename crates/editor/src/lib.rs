//! Platform-neutral file-editor state and configuration.

mod buffer;
mod config;
mod language;
mod language_service;
mod tree;

pub use buffer::{BufferStatus, EditorBuffer, ExternalChange};
pub use config::{
    EditorConfig, EditorConfigSource, IndentStyle, LineEnding, apply_editor_config,
    resolve_editor_config,
};
pub use language::{language_label_for_path, language_slug_for_path, lsp_language_id_for_path};
pub use language_service::{
    LanguageServerDefinition, ProjectLocalLanguageServer, language_server_by_id,
    language_servers_for_language, profile_language_id,
};
pub use tree::{ExplorerNode, ExplorerTree};
