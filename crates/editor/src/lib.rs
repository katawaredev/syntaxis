//! Platform-neutral file-editor state and configuration.

mod buffer;
mod config;
mod language;
mod language_service;
mod tree;

pub use buffer::{BufferStatus, EditorBuffer, ExternalChange};
pub use config::{
    apply_editor_config, resolve_editor_config, EditorConfig, EditorConfigSource, IndentStyle,
    LineEnding,
};
pub use language::{language_label_for_path, language_slug_for_path, lsp_language_id_for_path};
pub use language_service::{
    language_server_by_id, language_servers_for_language, profile_language_id,
    LanguageServerDefinition, ProjectLocalLanguageServer,
};
pub use tree::{ExplorerNode, ExplorerTree};
