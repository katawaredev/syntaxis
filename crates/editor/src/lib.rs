//! Platform-neutral file-editor state and configuration.

mod buffer;
mod completion;
mod config;
mod generated_completions;
mod language;
mod tree;

pub use buffer::{BufferStatus, EditorBuffer, ExternalChange};
pub use completion::{complete_any_word, complete_with_words, WordCompletions};
pub use config::{
    apply_editor_config, resolve_editor_config, EditorConfig, EditorConfigSource, IndentStyle,
    LineEnding,
};
pub use generated_completions::generated_completion_words;
pub use language::{language_label_for_path, language_slug_for_path};
pub use tree::{ExplorerNode, ExplorerTree};
