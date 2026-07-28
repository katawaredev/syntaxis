//! Syntaxis-maintained fork of `dioxus-code-editor`.
//!
//! Editable documents and editor diff mode are backed by a bundled `CodeMirror` 6
//! view with imperative commands and typed Dioxus events. Arborium/tree-sitter
//! remains available for the Git page's read-only unified diff renderer.

use std::{
    cell::RefCell,
    collections::BTreeSet,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use dioxus::prelude::*;
use dioxus_code::advanced::{Buffer, CodeThemeStyles, TokenSpan};
use dioxus_code::{CodeTheme, Language, Theme};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

pub const CODE_EDITOR_CSS: Asset = asset!("/assets/dioxus-code-editor.css");
const LSP_MODULE: Asset = asset!("/assets/lsp.bundle.js");
#[expect(
    clippy::large_include_file,
    reason = "the generated CodeMirror bundle must execute inside Dioxus' eval channel"
)]
const EDITOR_BRIDGE: &str = include_str!("../assets/editor.bundle.js");
static EDITOR_ID_NEXT: AtomicU64 = AtomicU64::new(0);

/// The syntax theme shared by editable and diff surfaces.
pub fn shared_code_theme() -> CodeTheme {
    CodeTheme::fixed(Theme::TOKYO_NIGHT)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiffLayout {
    #[default]
    Editor,
    Embedded,
    FullFile,
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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent editor compartments have independent boolean settings"
)]
struct EditorConfiguration {
    id: String,
    value: String,
    language: String,
    filename: String,
    line_numbers: bool,
    word_wrap: bool,
    tab_width: usize,
    indent_width: usize,
    indent_with_tabs: bool,
    read_only: bool,
    spellcheck: bool,
    autocomplete: bool,
    language_services: Vec<LanguageServiceConfig>,
    lsp_module: String,
    diff_original: Option<String>,
    aria_label: String,
    placeholder: String,
    search_matches: Vec<EditorRange>,
    active_search_match: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EditorBridgeCommand {
    Configure {
        config: Box<EditorConfiguration>,
    },
    ConfigureSearch {
        query: Option<EditorSearchQuery>,
    },
    Focus,
    GoToLine {
        line: usize,
    },
    Select {
        start: usize,
        end: usize,
    },
    Replace {
        value: String,
        start: usize,
        end: usize,
    },
    SearchNext,
    SearchPrevious,
    Destroy,
}

impl From<EditorCommandKind> for EditorBridgeCommand {
    fn from(command: EditorCommandKind) -> Self {
        match command {
            EditorCommandKind::Focus => Self::Focus,
            EditorCommandKind::GoToLine { line } => Self::GoToLine { line },
            EditorCommandKind::Select { start, end } => Self::Select { start, end },
            EditorCommandKind::Replace { value, start, end } => Self::Replace { value, start, end },
            EditorCommandKind::SearchNext => Self::SearchNext,
            EditorCommandKind::SearchPrevious => Self::SearchPrevious,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EditorBridgeEvent {
    Input {
        edits: Vec<EditorEdit>,
    },
    Selection {
        start: usize,
        end: usize,
        line: usize,
        column: usize,
        selection_count: usize,
        ranges: Vec<EditorRange>,
    },
    Search {
        count: usize,
        current: Option<usize>,
        valid: bool,
    },
    LanguageServiceStatus {
        server_id: String,
        server_name: String,
        status: LanguageServiceStatus,
        #[serde(default)]
        message: String,
    },
}

#[component]
pub fn CodeEditor(props: CodeEditorProps) -> Element {
    rsx! {
        InteractiveCodeEditor { editor_props: props }
    }
}

#[component]
fn InteractiveCodeEditor(editor_props: CodeEditorProps) -> Element {
    let props = editor_props;
    let generated_id = use_hook(|| {
        format!(
            "dxc-editor-{}",
            EDITOR_ID_NEXT.fetch_add(1, Ordering::Relaxed)
        )
    });
    let editor_id = if props.id.is_empty() {
        generated_id
    } else {
        props.id.clone()
    };
    let configuration = EditorConfiguration {
        id: editor_id.clone(),
        value: props.value.clone(),
        language: if props.language_name.is_empty() {
            props.language.slug().to_owned()
        } else {
            props.language_name.clone()
        },
        filename: props.filename.clone(),
        line_numbers: props.line_numbers,
        word_wrap: props.word_wrap,
        tab_width: props.tab_width,
        indent_width: props.indent_width,
        indent_with_tabs: props.indent_with_tabs,
        read_only: props.read_only,
        spellcheck: props.spellcheck,
        autocomplete: props.autocomplete,
        language_services: props.language_services.clone(),
        lsp_module: LSP_MODULE.to_string(),
        diff_original: props.diff_original.clone(),
        aria_label: props.aria_label.clone(),
        placeholder: props.placeholder.clone(),
        search_matches: props.search_matches.clone(),
        active_search_match: props.active_search_match,
    };
    let search_configuration = props.search_query.clone();
    let mut event_bridge = use_signal(|| None::<dioxus::document::Eval>);
    let last_configuration = use_hook(|| Rc::new(RefCell::new(None::<EditorConfiguration>)));
    let last_search_configuration =
        use_hook(|| Rc::new(RefCell::new(None::<Option<EditorSearchQuery>>)));

    use_effect({
        let configuration = configuration.clone();
        let last_configuration = Rc::clone(&last_configuration);
        move || {
            let mut events = document::eval(EDITOR_BRIDGE);
            drop(events.send(configuration.clone()));
            *last_configuration.borrow_mut() = Some(configuration.clone());
            event_bridge.set(Some(events));
            let event_configuration = Rc::clone(&last_configuration);
            spawn(async move {
                while let Ok(event) = events.recv::<EditorBridgeEvent>().await {
                    match event {
                        EditorBridgeEvent::Input { edits } => {
                            if let Some(configuration) = event_configuration.borrow_mut().as_mut() {
                                apply_editor_edits(&mut configuration.value, &edits);
                            }
                            props.oninput.call(edits);
                        }
                        EditorBridgeEvent::Selection {
                            start,
                            end,
                            line,
                            column,
                            selection_count,
                            ranges,
                        } => props.onselection.call(EditorSelection {
                            start,
                            end,
                            line,
                            column,
                            selection_count,
                            ranges,
                        }),
                        EditorBridgeEvent::Search {
                            count,
                            current,
                            valid,
                        } => props.onsearch.call(EditorSearchStatus {
                            count,
                            current,
                            valid,
                        }),
                        EditorBridgeEvent::LanguageServiceStatus {
                            server_id,
                            server_name,
                            status,
                            message,
                        } => {
                            props.on_language_service.call(LanguageServiceState {
                                server_id,
                                server_name,
                                status,
                                message,
                            });
                        }
                    }
                }
            });
        }
    });
    use_drop(move || {
        if let Some(events) = event_bridge() {
            drop(events.send(EditorBridgeCommand::Destroy));
        }
        if let Some(mut command) = props.command {
            command.set(None);
        }
    });
    use_effect(move || {
        let Some(mut command_signal) = props.command else {
            return;
        };
        let Some(command) = command_signal() else {
            return;
        };
        let Some(events) = event_bridge() else {
            return;
        };
        if events.send(EditorBridgeCommand::from(command.kind)).is_ok() {
            command_signal.set(None);
        }
    });

    if last_configuration.borrow().as_ref() != Some(&configuration) {
        if let Some(events) = event_bridge() {
            if events
                .send(EditorBridgeCommand::Configure {
                    config: Box::new(configuration.clone()),
                })
                .is_ok()
            {
                *last_configuration.borrow_mut() = Some(configuration);
            }
        }
    }
    if last_search_configuration.borrow().as_ref() != Some(&search_configuration) {
        if let Some(events) = event_bridge() {
            if events
                .send(EditorBridgeCommand::ConfigureSearch {
                    query: search_configuration.clone(),
                })
                .is_ok()
            {
                *last_search_configuration.borrow_mut() = Some(search_configuration);
            }
        }
    }
    let class = editor_class(
        props.theme,
        props.line_numbers,
        props.word_wrap,
        &props.class,
    );
    let diff_class = if props.diff_original.is_some() {
        "dxc-codemirror-diff"
    } else {
        Default::default()
    };
    rsx! {
        CodeThemeStyles { theme: props.theme }
        document::Stylesheet { href: CODE_EDITOR_CSS }
        div {
            id: editor_id,
            class: "{class} dxc-codemirror {diff_class}",
            style: "--dxc-editor-tab-width: {props.tab_width.max(1)}",
            onkeydown: move |event| props.onkeydown.call(event),
        }
    }
}

fn apply_editor_edits(value: &mut String, edits: &[EditorEdit]) -> bool {
    let mut normalized = Vec::with_capacity(edits.len());
    for edit in edits {
        let Some(start) = utf16_offset_to_byte(value, edit.start) else {
            return false;
        };
        let Some(end) = utf16_offset_to_byte(value, edit.end) else {
            return false;
        };
        if start > end {
            return false;
        }
        normalized.push((start, end, edit.text.as_str()));
    }
    normalized.sort_unstable_by(|left, right| right.0.cmp(&left.0).then(right.1.cmp(&left.1)));
    if !normalized.windows(2).all(|pair| pair[0].0 >= pair[1].1) {
        return false;
    }
    for (start, end, text) in normalized {
        value.replace_range(start..end, text);
    }
    true
}

fn utf16_offset_to_byte(value: &str, target: usize) -> Option<usize> {
    let mut utf16_offset = 0;
    for (byte_offset, character) in value.char_indices() {
        if utf16_offset == target {
            return Some(byte_offset);
        }
        utf16_offset += character.len_utf16();
        if utf16_offset > target {
            return None;
        }
    }
    (utf16_offset == target).then_some(value.len())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiffLineKind {
    Equal,
    Delete,
    Insert,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiffSegment {
    text: String,
    emphasized: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiffToken {
    text: String,
    tag: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiffLine {
    kind: DiffLineKind,
    old_line: Option<usize>,
    new_line: Option<usize>,
    segments: Vec<DiffSegment>,
    tokens: Vec<DiffToken>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DiffRow {
    Line(DiffLine),
    Fold { id: usize, lines: Vec<DiffLine> },
}

/// Syntax-aware, read-only unified diff used by both the editor and Git views.
#[component]
pub fn UnifiedDiffView(
    original: String,
    current: String,
    #[props(default = Language::Rust)] language: Language,
    #[props(default = shared_code_theme(), into)] theme: CodeTheme,
    #[props(default = true)] line_numbers: bool,
    #[props(default = false)] word_wrap: bool,
    #[props(default = 4)] tab_width: usize,
    #[props(default = true)] collapse_unchanged: bool,
    #[props(default)] layout: DiffLayout,
    #[props(default)] old_line_offset: usize,
    #[props(default)] new_line_offset: usize,
    #[props(into, default)] class: String,
) -> Element {
    let mut class = editor_class(theme, line_numbers, word_wrap, &class);
    class.push_str(match layout {
        DiffLayout::Editor => " dxc-diff-layout-editor",
        DiffLayout::Embedded => " dxc-diff-layout-embedded",
        DiffLayout::FullFile => " dxc-diff-layout-full-file",
    });
    let rows = unified_diff_rows(
        &original,
        &current,
        language,
        old_line_offset,
        new_line_offset,
        collapse_unchanged,
    );
    let mut expanded = use_signal(BTreeSet::<usize>::new);
    rsx! {
        CodeThemeStyles { theme }
        document::Stylesheet { href: CODE_EDITOR_CSS }
        div {
            class: "{class} dxc-diff-editor",
            style: "--dxc-editor-tab-width: {tab_width.max(1)}",
            role: "region",
            "aria-label": "Inline file changes",
            if rows.is_empty() {
                div { class: "dxc-diff-empty", "No changes" }
            }
            for row in rows {
                match row {
                    DiffRow::Line(line) => rsx! {
                        DiffLineView { line }
                    },
                    DiffRow::Fold { id, lines } => {
                        if expanded.read().contains(&id) {
                            rsx! {
                                for line in lines {
                                    DiffLineView { line }
                                }
                            }
                        } else {
                            let hidden_count = lines.len();
                            rsx! {
                                button {
                                    class: "dxc-diff-fold",
                                    "aria-label": "Expand {hidden_count} unchanged lines",
                                    onclick: move |_| {
                                        expanded.write().insert(id);
                                    },
                                    span { class: "dxc-diff-fold-meta", "⋯" }
                                    span { class: "dxc-diff-fold-label", "Expand {hidden_count} unchanged lines" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DiffLineView(line: DiffLine) -> Element {
    let (kind_class, marker) = match line.kind {
        DiffLineKind::Equal => ("dxc-diff-equal", ""),
        DiffLineKind::Delete => ("dxc-diff-delete", "−"),
        DiffLineKind::Insert => ("dxc-diff-insert", "+"),
    };
    rsx! {
        div { class: "dxc-diff-row {kind_class}",
            div { class: "dxc-diff-meta", aria_hidden: "true",
                span { class: "dxc-diff-marker", "{marker}" }
                span { class: "dxc-diff-old-line",
                    {line.old_line.map(|line| line.to_string()).unwrap_or_default()}
                }
                span { class: "dxc-diff-new-line",
                    {line.new_line.map(|line| line.to_string()).unwrap_or_default()}
                }
            }
            code { class: "dxc-diff-code",
                span { class: "dxc-diff-syntax",
                    for token in line.tokens {
                        if let Some(tag) = token.tag {
                            TokenSpan { text: token.text, tag }
                        } else {
                            span { "{token.text}" }
                        }
                    }
                }
                if line.segments.iter().any(|segment| segment.emphasized) {
                    span { class: "dxc-diff-inline-overlay", aria_hidden: "true",
                        for segment in line.segments {
                            if segment.emphasized {
                                mark { class: "dxc-diff-inline", "{segment.text}" }
                            } else {
                                span { "{segment.text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn unified_diff_rows(
    original: &str,
    current: &str,
    language: Language,
    old_line_offset: usize,
    new_line_offset: usize,
    collapse_unchanged: bool,
) -> Vec<DiffRow> {
    if original == current {
        return Vec::new();
    }
    let diff = TextDiff::from_lines(original, current);
    let old_tokens = highlighted_diff_lines(original, language);
    let new_tokens = highlighted_diff_lines(current, language);
    let lines = diff
        .iter_all_inline_changes()
        .map(|change| {
            let kind = match change.tag() {
                ChangeTag::Equal => DiffLineKind::Equal,
                ChangeTag::Delete => DiffLineKind::Delete,
                ChangeTag::Insert => DiffLineKind::Insert,
            };
            let mut segments = change
                .iter_strings_lossy()
                .map(|(emphasized, value)| DiffSegment {
                    text: value.into_owned(),
                    emphasized,
                })
                .collect::<Vec<_>>();
            trim_diff_line_ending(&mut segments);
            DiffLine {
                kind,
                old_line: change.old_index().map(|index| index + old_line_offset + 1),
                new_line: change.new_index().map(|index| index + new_line_offset + 1),
                segments,
                tokens: match kind {
                    DiffLineKind::Delete => change
                        .old_index()
                        .and_then(|index| old_tokens.get(index))
                        .cloned()
                        .unwrap_or_default(),
                    DiffLineKind::Equal | DiffLineKind::Insert => change
                        .new_index()
                        .and_then(|index| new_tokens.get(index))
                        .cloned()
                        .unwrap_or_default(),
                },
            }
        })
        .collect::<Vec<_>>();
    if collapse_unchanged {
        collapse_unchanged_lines(&lines)
    } else {
        lines.into_iter().map(DiffRow::Line).collect()
    }
}

fn highlighted_diff_lines(source: &str, language: Language) -> Vec<Vec<DiffToken>> {
    Buffer::new(language, source.to_owned()).map_or_else(
        |_| {
            source
                .split('\n')
                .map(|line| {
                    vec![DiffToken {
                        text: line.to_owned(),
                        tag: None,
                    }]
                })
                .collect()
        },
        |buffer| {
            buffer
                .lines()
                .into_iter()
                .map(|line| {
                    line.into_iter()
                        .map(|segment| DiffToken {
                            text: segment.text().to_owned(),
                            tag: segment.tag(),
                        })
                        .collect()
                })
                .collect()
        },
    )
}

fn trim_diff_line_ending(segments: &mut Vec<DiffSegment>) {
    let Some(last) = segments.last_mut() else {
        return;
    };
    if last.text.ends_with('\n') {
        last.text.pop();
        if last.text.ends_with('\r') {
            last.text.pop();
        }
    }
    if last.text.is_empty() && segments.len() > 1 {
        segments.pop();
    }
}

fn collapse_unchanged_lines(lines: &[DiffLine]) -> Vec<DiffRow> {
    const MARGIN: usize = 4;
    const MIN_FOLD: usize = 12;
    let mut rows = Vec::new();
    let mut index = 0;
    let mut fold_id = 0;
    while index < lines.len() {
        if lines[index].kind != DiffLineKind::Equal {
            rows.push(DiffRow::Line(lines[index].clone()));
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && lines[index].kind == DiffLineKind::Equal {
            index += 1;
        }
        let count = index - start;
        if count < MIN_FOLD {
            rows.extend(lines[start..index].iter().cloned().map(DiffRow::Line));
            continue;
        }
        rows.extend(
            lines[start..start + MARGIN]
                .iter()
                .cloned()
                .map(DiffRow::Line),
        );
        rows.push(DiffRow::Fold {
            id: fold_id,
            lines: lines[start + MARGIN..index - MARGIN].to_vec(),
        });
        fold_id += 1;
        rows.extend(
            lines[index - MARGIN..index]
                .iter()
                .cloned()
                .map(DiffRow::Line),
        );
    }
    rows
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
    use std::fmt::Write as _;

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

    #[test]
    fn bridge_edits_advance_the_cached_value_with_utf16_offsets() {
        let mut value = "a😀bc".to_owned();
        assert!(apply_editor_edits(
            &mut value,
            &[
                EditorEdit {
                    start: 1,
                    end: 3,
                    text: "🙂".into(),
                },
                EditorEdit {
                    start: 4,
                    end: 5,
                    text: "d".into(),
                },
            ],
        ));
        assert_eq!(value, "a🙂bd");
    }

    #[test]
    fn unified_diff_marks_replacements_and_inline_changes() {
        let rows = unified_diff_rows(
            "same\nold value\nend\n",
            "same\nnew value\nend\n",
            Language::Rust,
            0,
            0,
            true,
        );
        let lines = rows
            .iter()
            .filter_map(|row| match row {
                DiffRow::Line(line) => Some(line),
                DiffRow::Fold { .. } => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.kind == DiffLineKind::Delete));
        assert!(lines.iter().any(|line| line.kind == DiffLineKind::Insert));
        assert!(lines.iter().any(|line| {
            line.kind != DiffLineKind::Equal
                && line.segments.iter().any(|segment| segment.emphasized)
        }));
    }

    #[test]
    fn unified_diff_applies_fragment_line_offsets() {
        let rows = unified_diff_rows("old", "new", Language::Rust, 9, 19, false);
        let lines = rows
            .iter()
            .filter_map(|row| match row {
                DiffRow::Line(line) => Some(line),
                DiffRow::Fold { .. } => None,
            })
            .collect::<Vec<_>>();

        assert!(lines
            .iter()
            .any(|line| line.kind == DiffLineKind::Delete && line.old_line == Some(10)));
        assert!(lines
            .iter()
            .any(|line| line.kind == DiffLineKind::Insert && line.new_line == Some(20)));
    }

    #[test]
    fn unified_diff_collapses_large_unchanged_regions() {
        let mut original = String::new();
        for line in 1..=30 {
            writeln!(original, "line {line}").expect("writing to a String cannot fail");
        }
        let current = original.replace("line 15\n", "changed 15\n");
        let rows = unified_diff_rows(&original, &current, Language::Rust, 0, 0, true);

        assert!(rows.iter().any(|row| matches!(row, DiffRow::Fold { .. })));
        assert!(rows
            .iter()
            .any(|row| matches!(row, DiffRow::Line(line) if line.kind == DiffLineKind::Delete)));
        assert!(rows
            .iter()
            .any(|row| matches!(row, DiffRow::Line(line) if line.kind == DiffLineKind::Insert)));
    }
}
