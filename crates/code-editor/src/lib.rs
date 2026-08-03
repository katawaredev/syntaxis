//! Syntaxis-maintained fork of `dioxus-code-editor`.
//!
//! Editable documents and editor diff mode are backed by a bundled `CodeMirror` 6
//! view with imperative commands and typed Dioxus events. Arborium/tree-sitter
//! remains available for the Git page's read-only unified diff renderer.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use dioxus::prelude::*;
use dioxus_code::advanced::CodeThemeStyles;
use dioxus_code::{CodeTheme, Language, Theme};
use serde::{Deserialize, Serialize};

pub const CODE_EDITOR_CSS: Asset = asset!("/assets/dioxus-code-editor.css");
const LSP_MODULE: Asset = asset!("/assets/lsp.bundle.js");
#[expect(
    clippy::large_include_file,
    reason = "the generated CodeMirror bundle must execute inside Dioxus' eval channel"
)]
const EDITOR_BRIDGE: &str = include_str!("../assets/editor.bundle.js");
static EDITOR_ID_NEXT: AtomicU64 = AtomicU64::new(0);

mod bridge;
mod diff;

pub use diff::{DiffLayout, UnifiedDiffView};

use bridge::InteractiveCodeEditor;

/// The syntax theme shared by editable and diff surfaces.
pub fn shared_code_theme() -> CodeTheme {
    CodeTheme::fixed(Theme::TOKYO_NIGHT)
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EditorRange {
    pub start: usize,
    pub end: usize,
}

/// One `CodeMirror` document change, expressed in UTF-16 code-unit offsets
/// against the document before the transaction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EditorEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EditorSelection {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
    pub selection_count: usize,
    #[serde(default)]
    pub ranges: Vec<EditorRange>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct EditorSearchQuery {
    pub query: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct EditorSearchStatus {
    pub count: usize,
    pub current: Option<usize>,
    pub valid: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LanguageServiceConfig {
    pub server_id: String,
    pub server_name: String,
    pub language_id: String,
    pub session_key: String,
    pub endpoint: String,
    pub root_uri: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LanguageServiceStatus {
    Starting,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct LanguageServiceState {
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub server_name: String,
    pub status: LanguageServiceStatus,
    #[serde(default)]
    pub message: String,
}

impl Default for EditorSearchStatus {
    fn default() -> Self {
        Self {
            count: 0,
            current: None,
            valid: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EditorCommandKind {
    Focus,
    GoToLine {
        line: usize,
    },
    Select {
        start: usize,
        end: usize,
    },
    /// Replace the controlled value as one editor-history transaction.
    Replace {
        value: String,
        start: usize,
        end: usize,
    },
    SearchNext,
    SearchPrevious,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EditorCommand {
    pub revision: u64,
    #[serde(flatten)]
    pub kind: EditorCommandKind,
}

#[derive(Props, Clone, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent editor toggles are clearer than an artificial nested configuration"
)]
pub struct CodeEditorProps {
    #[props(into)]
    pub value: String,
    #[props(default = Language::Rust)]
    pub language: Language,
    #[props(into, default)]
    pub language_name: String,
    #[props(into, default)]
    pub filename: String,
    #[props(default = shared_code_theme(), into)]
    pub theme: CodeTheme,
    #[props(default = true)]
    pub line_numbers: bool,
    #[props(default = false)]
    pub word_wrap: bool,
    #[props(default = 4)]
    pub tab_width: usize,
    #[props(default = 4)]
    pub indent_width: usize,
    #[props(default = false)]
    pub indent_with_tabs: bool,
    #[props(default = false)]
    pub read_only: bool,
    #[props(default = false)]
    pub spellcheck: bool,
    #[props(default = false)]
    pub autocomplete: bool,
    #[props(default)]
    pub language_services: Vec<LanguageServiceConfig>,
    #[props(into, default = "Code editor")]
    pub aria_label: String,
    #[props(into, default)]
    pub placeholder: String,
    #[props(into, default)]
    pub class: String,
    #[props(into, default)]
    pub id: String,
    #[props(default)]
    /// One-shot command channel. Commands are cleared after they are delivered
    /// so a remounted editor cannot replay stale mutations.
    pub command: Option<Signal<Option<EditorCommand>>>,
    #[props(default)]
    pub search_matches: Vec<EditorRange>,
    #[props(default)]
    pub active_search_match: Option<usize>,
    #[props(default)]
    pub search_query: Option<EditorSearchQuery>,
    /// Original contents used to render an inline unified diff. Diff mode is read-only.
    #[props(default)]
    pub diff_original: Option<String>,
    #[props(default = EventHandler::new(|_: Vec<EditorEdit>| {}))]
    pub oninput: EventHandler<Vec<EditorEdit>>,
    #[props(default = EventHandler::new(|_: EditorSelection| {}))]
    pub onselection: EventHandler<EditorSelection>,
    #[props(default = EventHandler::new(|_: EditorSearchStatus| {}))]
    pub onsearch: EventHandler<EditorSearchStatus>,
    #[props(default = EventHandler::new(|_: KeyboardEvent| {}))]
    pub onkeydown: EventHandler<KeyboardEvent>,
    #[props(default = EventHandler::new(|_: LanguageServiceState| {}))]
    pub on_language_service: EventHandler<LanguageServiceState>,
}

#[component]
pub fn CodeEditor(props: CodeEditorProps) -> Element {
    rsx! {
        InteractiveCodeEditor { editor_props: props }
    }
}

fn editor_class(theme: CodeTheme, line_numbers: bool, word_wrap: bool, extra: &str) -> String {
    let mut class = format!("dxc-editor {}", theme.classes());
    if !line_numbers {
        class.push_str(" dxc-editor-no-gutter");
    }
    if word_wrap {
        class.push_str(" dxc-editor-wrap");
    }
    if !extra.is_empty() {
        class.push(' ');
        class.push_str(extra);
    }
    class
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_command_has_a_stable_browser_contract() {
        let command = EditorCommand {
            revision: 7,
            kind: EditorCommandKind::Replace {
                value: "new value".into(),
                start: 3,
                end: 6,
            },
        };
        let serialized = serde_json::to_value(command).expect("editor command should serialize");

        assert_eq!(serialized["revision"], 7);
        assert_eq!(serialized["kind"], "replace");
        assert_eq!(serialized["value"], "new value");
        assert_eq!(serialized["start"], 3);
        assert_eq!(serialized["end"], 6);
    }
}
