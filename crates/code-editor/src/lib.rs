//! Syntaxis-maintained fork of `dioxus-code-editor`.
//!
//! The upstream controlled textarea/highlighter is retained while this fork adds
//! imperative commands, selection reporting, wrapping, indentation, paired
//! delimiters, and textarea-backed multiple-selection editing.

use std::{cell::RefCell, rc::Rc};

use dioxus::prelude::*;
use dioxus_code::advanced::{Buffer, CodeThemeStyles, HighlightSegment, SourceEdit, TokenSpan};
use dioxus_code::{CodeTheme, Language};
use serde::{Deserialize, Serialize};

pub const CODE_EDITOR_CSS: Asset = asset!("/assets/dioxus-code-editor.css");

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
    GoToLine { line: usize },
    Select { start: usize, end: usize },
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
    #[props(default, into)]
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
    pub command: Option<EditorCommand>,
    #[props(default)]
    pub search_matches: Vec<EditorRange>,
    #[props(default)]
    pub active_search_match: Option<usize>,
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

    use_effect({
        let editor_id = editor_id.clone();
        let indent = if props.indent_with_tabs {
            "\t".to_owned()
        } else {
            " ".repeat(props.indent_width.max(1))
        };
        move || {
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
    });
    use_effect({
        let editor_id = editor_id.clone();
        move || {
            let Some(command) = props.command.clone() else {
                return;
            };
            let command_eval = document::eval(EDITOR_COMMAND);
            let _ = command_eval.send((editor_id.clone(), command));
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

const EDITOR_COMMAND: &str = include_str!("../assets/editor-command.js");
const EDITOR_BRIDGE: &str = include_str!("../assets/editor-bridge.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_diff_stays_on_utf8_boundaries() {
        let edit = source_edit_from_diff("aéz", "aèz").unwrap();
        assert_eq!(edit.start_byte, 1);
        assert_eq!(edit.old_end_byte, 3);
        assert_eq!(edit.new_end_byte, 3);
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
}
