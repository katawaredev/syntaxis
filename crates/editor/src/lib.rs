//! Platform-neutral file-editor state and configuration.

mod buffer;
mod config;
mod language;
mod tree;

pub use buffer::{BufferStatus, EditorBuffer, ExternalChange};
pub use config::{
    apply_editor_config, resolve_editor_config, EditorConfig, EditorConfigSource, IndentStyle,
    LineEnding,
};
pub use language::{language_label_for_path, language_slug_for_path};
pub use tree::{ExplorerNode, ExplorerTree};
