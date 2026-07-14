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

const EDITOR_COMMAND: &str = r#"
const [id, command] = await dioxus.recv();
const input = document.getElementById(id);
if (!(input instanceof HTMLTextAreaElement)) return;
const encoder = new TextEncoder();
const byteToCodeUnit = byteOffset => {
    let bytes = 0;
    let codeUnits = 0;
    for (const character of input.value) {
        const width = encoder.encode(character).length;
        if (bytes + width > byteOffset) break;
        bytes += width;
        codeUnits += character.length;
    }
    return codeUnits;
};
if (command.kind === "focus") input.focus();
if (command.kind === "select") {
    input.focus();
    input.setSelectionRange(byteToCodeUnit(command.start), byteToCodeUnit(command.end));
    input.dispatchEvent(new Event("select", { bubbles: true }));
}
if (command.kind === "go_to_line") {
    const line = Math.max(1, command.line);
    let offset = 0;
    for (let current = 1; current < line; current += 1) {
        const next = input.value.indexOf("\n", offset);
        if (next < 0) { offset = input.value.length; break; }
        offset = next + 1;
    }
    input.focus();
    input.setSelectionRange(offset, offset);
    const lineHeight = Number.parseFloat(getComputedStyle(input).lineHeight) || 22;
    input.scrollTop = Math.max(0, (line - 3) * lineHeight);
    input.dispatchEvent(new Event("select", { bubbles: true }));
}
"#;

const EDITOR_BRIDGE: &str = r#"
const [id, indent] = await dioxus.recv();
let input;
for (let attempt = 0; attempt < 100; attempt += 1) {
    input = document.getElementById(id);
    if (input instanceof HTMLTextAreaElement) break;
    await new Promise(resolve => setTimeout(resolve, 10));
}
if (!(input instanceof HTMLTextAreaElement)) return;
let ranges = [];
const pairs = { "(": ")", "[": "]", "{": "}", "\"": "\"", "'": "'", "`": "`" };
const bracketPairs = { "(": ")", "[": "]", "{": "}" };
const encoder = new TextEncoder();
const undoStack = [];
const redoStack = [];
const historyLimit = 200;
const historyByteLimit = 16 * 1024 * 1024;
let pendingHistory = null;
const cloneRanges = source => source.map(([start, end]) => [start, end]);
const captureHistory = () => ({
    value: input.value,
    start: input.selectionStart ?? 0,
    end: input.selectionEnd ?? input.selectionStart ?? 0,
    ranges: cloneRanges(ranges),
});
const historySize = entry => entry.oldText.length + entry.newText.length;
const pushHistory = (stack, entry) => {
    stack.push(entry);
    let bytes = stack.reduce((total, item) => total + historySize(item), 0);
    while (stack.length > historyLimit || bytes > historyByteLimit) {
        bytes -= historySize(stack.shift());
    }
};
const commitHistory = before => {
    const after = captureHistory();
    if (before.value === after.value) return;
    let start = 0;
    const shared = Math.min(before.value.length, after.value.length);
    while (start < shared && before.value[start] === after.value[start]) start += 1;
    let oldEnd = before.value.length;
    let newEnd = after.value.length;
    while (oldEnd > start && newEnd > start && before.value[oldEnd - 1] === after.value[newEnd - 1]) {
        oldEnd -= 1;
        newEnd -= 1;
    }
    pushHistory(undoStack, {
        start,
        oldText: before.value.slice(start, oldEnd),
        newText: after.value.slice(start, newEnd),
        before: { start: before.start, end: before.end, ranges: before.ranges },
        after: { start: after.start, end: after.end, ranges: after.ranges },
    });
    redoStack.length = 0;
};
const byteOffset = codeUnits => encoder.encode(input.value.slice(0, codeUnits)).length;
const emit = () => {
    const start = input.selectionStart ?? 0;
    const end = input.selectionEnd ?? start;
    const before = input.value.slice(0, start);
    const lineStart = before.lastIndexOf("\n") + 1;
    dioxus.send({
        start: byteOffset(start),
        end: byteOffset(end),
        line: before.split("\n").length,
        column: start - lineStart + 1,
        selection_count: Math.max(1, ranges.length),
        ranges: ranges.map(([rangeStart, rangeEnd]) => ({
            start: byteOffset(rangeStart),
            end: byteOffset(rangeEnd),
        })),
    });
};
const applyValue = (value, start, end = start) => {
    input.value = value;
    input.setSelectionRange(start, end);
    input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText" }));
    input.dispatchEvent(new Event("select", { bubbles: true }));
};
const setValue = (value, start, end = start) => {
    const before = captureHistory();
    applyValue(value, start, end);
    commitHistory(before);
};
const restoreHistory = (from, to, redo) => {
    const entry = from.pop();
    if (!entry) return false;
    pushHistory(to, entry);
    const removed = redo ? entry.oldText : entry.newText;
    const inserted = redo ? entry.newText : entry.oldText;
    const state = redo ? entry.after : entry.before;
    const value = input.value.slice(0, entry.start)
        + inserted
        + input.value.slice(entry.start + removed.length);
    ranges = cloneRanges(state.ranges);
    applyValue(value, state.start, state.end);
    return true;
};
const replaceRange = (start, end, text, selectStart = text.length, selectEnd = text.length) => {
    const value = input.value.slice(0, start) + text + input.value.slice(end);
    setValue(value, start + selectStart, start + selectEnd);
};
const wordAt = position => {
    let start = position;
    let end = position;
    while (start > 0 && /[\w$]/.test(input.value[start - 1])) start -= 1;
    while (end < input.value.length && /[\w$]/.test(input.value[end])) end += 1;
    return [start, end];
};
const selectNextOccurrence = all => {
    let start = input.selectionStart ?? 0;
    let end = input.selectionEnd ?? start;
    if (start === end) [start, end] = wordAt(start);
    const needle = input.value.slice(start, end);
    if (!needle) return;
    if (all) {
        ranges = [];
        let offset = 0;
        while ((offset = input.value.indexOf(needle, offset)) >= 0) {
            ranges.push([offset, offset + needle.length]);
            offset += Math.max(1, needle.length);
        }
    } else {
        if (ranges.length === 0) ranges.push([start, end]);
        const from = ranges.at(-1)[1];
        let next = input.value.indexOf(needle, from);
        if (next < 0) next = input.value.indexOf(needle);
        if (next >= 0 && !ranges.some(([rangeStart]) => rangeStart === next)) {
            ranges.push([next, next + needle.length]);
        }
    }
    const active = ranges.at(-1);
    input.setSelectionRange(active[0], active[1]);
    emit();
};
const addVerticalCursor = direction => {
    const start = input.selectionStart ?? 0;
    const lineStart = input.value.lastIndexOf("\n", start - 1) + 1;
    const column = start - lineStart;
    let targetLineStart;
    if (direction > 0) {
        const lineEnd = input.value.indexOf("\n", start);
        if (lineEnd < 0) return;
        targetLineStart = lineEnd + 1;
    } else {
        if (lineStart === 0) return;
        const previousEnd = lineStart - 1;
        targetLineStart = input.value.lastIndexOf("\n", previousEnd - 1) + 1;
    }
    const targetEnd = input.value.indexOf("\n", targetLineStart);
    const target = Math.min(targetLineStart + column, targetEnd < 0 ? input.value.length : targetEnd);
    if (ranges.length === 0) ranges.push([start, input.selectionEnd ?? start]);
    ranges.push([target, target]);
    input.setSelectionRange(target, target);
    emit();
};
const onKeyDown = event => {
    const mod = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();
    if (mod && !event.altKey && key === "z") {
        const stack = event.shiftKey ? redoStack : undoStack;
        const destination = event.shiftKey ? undoStack : redoStack;
        if (stack.length > 0) {
            event.preventDefault();
            restoreHistory(stack, destination, event.shiftKey);
        }
        return;
    }
    if (mod && !event.altKey && key === "y") {
        if (redoStack.length > 0) {
            event.preventDefault();
            restoreHistory(redoStack, undoStack, true);
        }
        return;
    }
    if (mod && key === "d") {
        event.preventDefault();
        selectNextOccurrence(false);
        return;
    }
    if (mod && event.shiftKey && event.key.toLowerCase() === "l") {
        event.preventDefault();
        selectNextOccurrence(true);
        return;
    }
    if (event.altKey && event.shiftKey && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
        event.preventDefault();
        addVerticalCursor(event.key === "ArrowDown" ? 1 : -1);
        return;
    }
    if (event.key === "Escape") ranges = [];
    if (event.key === "Tab" && !mod && !event.altKey) {
        event.preventDefault();
        const start = input.selectionStart ?? 0;
        const end = input.selectionEnd ?? start;
        const lineStart = input.value.lastIndexOf("\n", start - 1) + 1;
        if (start !== end && input.value.slice(start, end).includes("\n")) {
            const selected = input.value.slice(lineStart, end);
            const next = event.shiftKey
                ? selected.replace(new RegExp(`^${indent === "\t" ? "\\t" : ` {1,${indent.length}}`}`, "gm"), "")
                : selected.replace(/^/gm, indent);
            replaceRange(lineStart, end, next, start - lineStart, next.length);
        } else if (event.shiftKey) {
            const before = input.value.slice(lineStart, start);
            const removable = before.endsWith("\t") ? 1 : Math.min(indent.length, before.match(/ +$/)?.[0].length ?? 0);
            if (removable > 0) replaceRange(start - removable, start, "");
        } else {
            replaceRange(start, end, indent);
        }
        return;
    }
    if (event.key === "Enter" && !mod && !event.altKey && ranges.length <= 1) {
        event.preventDefault();
        const start = input.selectionStart ?? 0;
        const end = input.selectionEnd ?? start;
        const lineStart = input.value.lastIndexOf("\n", start - 1) + 1;
        const leading = input.value.slice(lineStart, start).match(/^\s*/)?.[0] ?? "";
        const extra = /[({\[]\s*$/.test(input.value.slice(lineStart, start)) ? indent : "";
        const closing = bracketPairs[input.value[start - 1]];
        if (start === end && closing && input.value[start] === closing) {
            const inner = `\n${leading}${indent}`;
            replaceRange(start, end, `${inner}\n${leading}`, inner.length);
            return;
        }
        replaceRange(start, end, `\n${leading}${extra}`);
        return;
    }
    const start = input.selectionStart ?? 0;
    const end = input.selectionEnd ?? start;
    if (!mod && !event.altKey && pairs[event.key]) {
        event.preventDefault();
        const selected = input.value.slice(start, end);
        replaceRange(start, end, event.key + selected + pairs[event.key], 1, 1 + selected.length);
        return;
    }
    if (!mod && start === end && Object.values(pairs).includes(event.key) && input.value[start] === event.key) {
        event.preventDefault();
        input.setSelectionRange(start + 1, start + 1);
        emit();
        return;
    }
    if (event.key === "Backspace" && start === end && pairs[input.value[start - 1]] === input.value[start]) {
        event.preventDefault();
        replaceRange(start - 1, start + 1, "");
    }
};
const onBeforeInput = event => {
    if (event.inputType === "historyUndo" || event.inputType === "historyRedo") return;
    pendingHistory = captureHistory();
    if (ranges.length < 2) {
        return;
    }
    const supported = event.inputType === "insertText"
        || event.inputType === "deleteContentBackward"
        || event.inputType === "deleteContentForward";
    if (!supported) {
        ranges = [];
        return;
    }
    event.preventDefault();
    const before = pendingHistory;
    pendingHistory = null;
    const insertion = event.inputType === "insertText" ? (event.data ?? "") : "";
    const adjusted = ranges.map(([start, end]) => {
        if (start !== end) return [start, end];
        if (event.inputType === "deleteContentBackward") return [Math.max(0, start - 1), end];
        if (event.inputType === "deleteContentForward") return [start, Math.min(input.value.length, end + 1)];
        return [start, end];
    });
    let value = input.value;
    for (const [start, end] of [...adjusted].sort((a, b) => b[0] - a[0])) {
        value = value.slice(0, start) + insertion + value.slice(end);
    }
    let delta = 0;
    ranges = adjusted.map(([start, end]) => {
        const caret = start + delta + insertion.length;
        delta += insertion.length - (end - start);
        return [caret, caret];
    });
    const active = ranges.at(-1);
    applyValue(value, active[0], active[1]);
    commitHistory(before);
};
const onInput = () => {
    if (!pendingHistory) return;
    const before = pendingHistory;
    pendingHistory = null;
    commitHistory(before);
};
const onSelection = () => {
    if (!input.matches(":focus")) return;
    const active = ranges.at(-1);
    if (!active || active[0] !== input.selectionStart || active[1] !== input.selectionEnd) ranges = [];
    emit();
};
input.addEventListener("keydown", onKeyDown);
input.addEventListener("beforeinput", onBeforeInput);
input.addEventListener("input", onInput);
input.addEventListener("select", onSelection);
input.addEventListener("keyup", onSelection);
input.addEventListener("click", onSelection);
emit();
await dioxus.recv();
input.removeEventListener("keydown", onKeyDown);
input.removeEventListener("beforeinput", onBeforeInput);
input.removeEventListener("input", onInput);
input.removeEventListener("select", onSelection);
input.removeEventListener("keyup", onSelection);
input.removeEventListener("click", onSelection);
"#;

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
