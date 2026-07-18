//! Syntaxis-maintained fork of `dioxus-code-editor`.
//!
//! The upstream controlled textarea/highlighter is retained while this fork adds
//! imperative commands, selection reporting, wrapping, indentation, paired
//! delimiters, and textarea-backed multiple-selection editing.

use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use dioxus::prelude::*;
use dioxus_code::advanced::{Buffer, CodeThemeStyles, HighlightSegment, SourceEdit, TokenSpan};
use dioxus_code::{CodeTheme, Language, Theme};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

pub const CODE_EDITOR_CSS: Asset = asset!("/assets/dioxus-code-editor.css");

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
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EditorCommand {
    pub revision: u64,
    #[serde(flatten)]
    pub kind: EditorCommandKind,
}

#[derive(Props, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct CodeEditorProps {
    #[props(into)]
    pub value: String,
    #[props(default = Language::Rust)]
    pub language: Language,
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
    /// Original contents used to render an inline unified diff. Diff mode is read-only.
    #[props(default)]
    pub diff_original: Option<String>,
    #[props(default = EventHandler::new(|_: String| {}))]
    pub oninput: EventHandler<String>,
    #[props(default = EventHandler::new(|_: EditorSelection| {}))]
    pub onselection: EventHandler<EditorSelection>,
    #[props(default = EventHandler::new(|_: KeyboardEvent| {}))]
    pub onkeydown: EventHandler<KeyboardEvent>,
}

struct EditorBuffer {
    buffer: Option<Buffer>,
    language: Language,
    value: String,
}

#[component]
pub fn CodeEditor(props: CodeEditorProps) -> Element {
    let editor_id = if props.id.is_empty() {
        format!("dxc-editor-{}", use_hook(|| 1_u64))
    } else {
        props.id.clone()
    };
    let state = use_hook({
        let value = props.value.clone();
        let language = props.language;
        move || {
            Rc::new(RefCell::new(EditorBuffer {
                buffer: Buffer::new(language, value.clone()).ok(),
                language,
                value,
            }))
        }
    });
    {
        let mut state = state.borrow_mut();
        if state.language != props.language {
            state.buffer = Buffer::new(props.language, props.value.clone()).ok();
            state.language = props.language;
            state.value.clone_from(&props.value);
        } else if state.value != props.value {
            let edit = source_edit_from_diff(&state.value, &props.value);
            let updated = state.buffer.as_mut().is_some_and(|buffer| {
                edit.is_some_and(|edit| buffer.edit(edit, props.value.clone()).is_ok())
            });
            if !updated {
                state.buffer = Buffer::new(props.language, props.value.clone()).ok();
            }
            state.value.clone_from(&props.value);
        }
    }
    let mut event_bridge = use_signal(|| None::<dioxus::document::Eval>);
    let mut multi_selections = use_signal(Vec::<EditorRange>::new);
    let diff_mode = props.diff_original.is_some();

    use_effect({
        let editor_id = editor_id.clone();
        let indent = if props.indent_with_tabs {
            "\t".to_owned()
        } else {
            " ".repeat(props.indent_width.max(1))
        };
        move || {
            if diff_mode {
                return;
            }
            let mut events = document::eval(EDITOR_BRIDGE);
            let _ = events.send((editor_id.clone(), indent.clone()));
            event_bridge.set(Some(events));
            spawn(async move {
                while let Ok(selection) = events.recv::<EditorSelection>().await {
                    multi_selections.set(selection.ranges.clone());
                    props.onselection.call(selection);
                }
            });
        }
    });
    use_drop(move || {
        if let Some(events) = event_bridge() {
            let _ = events.send(true);
        }
        if let Some(mut command) = props.command {
            command.set(None);
        }
    });
    use_effect(move || {
        if diff_mode {
            return;
        }
        let Some(mut command_signal) = props.command else {
            return;
        };
        let Some(command) = command_signal() else {
            return;
        };
        let Some(events) = event_bridge() else {
            return;
        };
        if events.send(command).is_ok() {
            command_signal.set(None);
        }
    });

    let class = editor_class(
        props.theme,
        props.line_numbers,
        props.word_wrap,
        &props.class,
    );
    let readonly = props.read_only.then_some("true");
    // Render directly from the incremental buffer. `Buffer::highlighted()`
    // creates a full source-and-span snapshot, which is unnecessary while the
    // component already owns the buffer for this render.
    let render_state = state.borrow();
    let lines = render_state.buffer.as_ref().map_or_else(
        || {
            props
                .value
                .split('\n')
                .map(|line| vec![HighlightSegment::new(line, None)])
                .collect()
        },
        Buffer::lines,
    );
    let line_count = lines.len();
    let search_lines = overlay_lines(&props.value, &props.search_matches);
    let multi_selection_lines = overlay_lines(&props.value, &multi_selections());
    if let Some(original) = props.diff_original.as_deref() {
        return rsx! {
            UnifiedDiffView {
                original: original.to_owned(),
                current: props.value.clone(),
                language: props.language,
                theme: props.theme,
                line_numbers: props.line_numbers,
                word_wrap: props.word_wrap,
                tab_width: props.tab_width,
                class: props.class.clone(),
            }
        };
    }
    rsx! {
        CodeThemeStyles { theme: props.theme }
        document::Stylesheet { href: CODE_EDITOR_CSS }
        div {
            class,
            style: "--dxc-editor-tab-width: {props.tab_width.max(1)}",
            if props.line_numbers {
                div { class: "dxc-editor-gutter", aria_hidden: "true",
                    for index in 0..line_count {
                        div { class: "dxc-editor-gutter-line", "{index + 1}" }
                    }
                }
            }
            div { class: "dxc-editor-viewport",
                div { class: "dxc-editor-highlight", aria_hidden: "true",
                    for line in lines {
                        div { class: "dxc-editor-line",
                            for segment in line {
                                if let Some(tag) = segment.tag() {
                                    TokenSpan { text: segment.text(), tag }
                                } else {
                                    span { "{segment.text()}" }
                                }
                            }
                        }
                    }
                }
                if !props.search_matches.is_empty() {
                    RangeOverlay {
                        class: "dxc-editor-search-highlights",
                        lines: search_lines,
                        range_class: "dxc-editor-search-match",
                        active_range: props.active_search_match,
                    }
                }
                if multi_selections.read().iter().any(|range| range.start != range.end) {
                    RangeOverlay {
                        class: "dxc-editor-multi-selections",
                        lines: multi_selection_lines,
                        range_class: "dxc-editor-multi-selection",
                    }
                }
                textarea {
                    id: editor_id,
                    class: "dxc-editor-input",
                    value: props.value,
                    readonly: props.read_only,
                    spellcheck: props.spellcheck,
                    autocomplete: "off",
                    autocapitalize: "off",
                    autocorrect: "off",
                    role: "textbox",
                    "aria-label": props.aria_label,
                    "aria-multiline": "true",
                    "aria-readonly": readonly,
                    placeholder: props.placeholder,
                    wrap: if props.word_wrap { "soft" } else { "off" },
                    onkeydown: move |event| props.onkeydown.call(event),
                    oninput: move |event| props.oninput.call(event.value()),
                }
            }
        }
    }
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

#[derive(Clone, Debug, PartialEq)]
struct OverlaySegment {
    text: String,
    range_index: Option<usize>,
}

fn overlay_lines(source: &str, ranges: &[EditorRange]) -> Vec<Vec<OverlaySegment>> {
    let mut ranges = ranges
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, range)| {
            range.start < range.end
                && range.end <= source.len()
                && source.is_char_boundary(range.start)
                && source.is_char_boundary(range.end)
        })
        .collect::<Vec<_>>();
    ranges.sort_by_key(|(_, range)| range.start);

    let mut line_start = 0;
    source
        .split('\n')
        .map(|line| {
            let line_end = line_start + line.len();
            let mut cursor = 0;
            let mut segments = Vec::new();
            for (index, range) in ranges.iter().copied() {
                if range.end <= line_start || range.start >= line_end {
                    continue;
                }
                let start = range.start.saturating_sub(line_start).max(cursor);
                let end = range.end.saturating_sub(line_start).min(line.len());
                if start >= end {
                    continue;
                }
                if cursor < start {
                    segments.push(OverlaySegment {
                        text: line[cursor..start].to_owned(),
                        range_index: None,
                    });
                }
                segments.push(OverlaySegment {
                    text: line[start..end].to_owned(),
                    range_index: Some(index),
                });
                cursor = end;
            }
            if cursor < line.len() {
                segments.push(OverlaySegment {
                    text: line[cursor..].to_owned(),
                    range_index: None,
                });
            }
            line_start = line_end + 1;
            segments
        })
        .collect()
}

#[component]
fn RangeOverlay(
    class: String,
    lines: Vec<Vec<OverlaySegment>>,
    range_class: String,
    active_range: Option<usize>,
) -> Element {
    rsx! {
        div { class: "dxc-editor-range-overlay {class}", aria_hidden: "true",
            for line in lines {
                div { class: "dxc-editor-line",
                    for segment in line {
                        if let Some(index) = segment.range_index {
                            mark { class: if active_range == Some(index) { "{range_class} dxc-editor-range-active" } else { "{range_class}" },
                                "{segment.text}"
                            }
                        } else {
                            span { "{segment.text}" }
                        }
                    }
                }
            }
        }
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

fn source_edit_from_diff(old: &str, new: &str) -> Option<SourceEdit> {
    if old == new {
        return None;
    }
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while start > 0 && (!old.is_char_boundary(start) || !new.is_char_boundary(start)) {
        start -= 1;
    }
    let mut old_end = old.len();
    let mut new_end = new.len();
    while old_end > start
        && new_end > start
        && old.as_bytes()[old_end - 1] == new.as_bytes()[new_end - 1]
    {
        old_end -= 1;
        new_end -= 1;
    }
    while old_end < old.len() && !old.is_char_boundary(old_end) {
        old_end += 1;
    }
    while new_end < new.len() && !new.is_char_boundary(new_end) {
        new_end += 1;
    }
    Some(SourceEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
    })
}

const EDITOR_BRIDGE: &str = include_str!("../assets/editor-bridge.js");

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    #[test]
    fn source_diff_stays_on_utf8_boundaries() {
        let edit = source_edit_from_diff("aéz", "aèz").unwrap();
        assert_eq!(edit.start_byte, 1);
        assert_eq!(edit.old_end_byte, 3);
        assert_eq!(edit.new_end_byte, 3);
    }

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
        let serialized = serde_json::to_value(command).unwrap();

        assert_eq!(serialized["revision"], 7);
        assert_eq!(serialized["kind"], "replace");
        assert_eq!(serialized["value"], "new value");
        assert_eq!(serialized["start"], 3);
        assert_eq!(serialized["end"], 6);
    }

    #[test]
    fn overlay_lines_preserve_text_and_mark_every_range() {
        let source = "root value\nroot";
        let lines = overlay_lines(
            source,
            &[
                EditorRange { start: 0, end: 4 },
                EditorRange { start: 11, end: 15 },
            ],
        );

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0][0].range_index, Some(0));
        assert_eq!(lines[0][0].text, "root");
        assert_eq!(lines[1][0].range_index, Some(1));
        assert_eq!(lines[1][0].text, "root");
    }

    #[test]
    fn overlay_lines_accept_utf8_byte_ranges() {
        let lines = overlay_lines("é root", &[EditorRange { start: 3, end: 7 }]);

        assert_eq!(lines[0][1].text, "root");
        assert_eq!(lines[0][1].range_index, Some(0));
    }

    #[test]
    fn incremental_edit_handles_a_bounded_large_buffer() {
        let original = "fn value() -> usize { 42 }\n".repeat(18_000);
        let updated = format!("{original}// edited\n");
        let mut buffer = Buffer::new(Language::Rust, original.clone()).unwrap();
        let edit = source_edit_from_diff(&original, &updated).unwrap();

        buffer.edit(edit, updated).unwrap();

        assert!(buffer.highlighted().lines().len() > 18_000);
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
            writeln!(original, "line {line}").unwrap();
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
