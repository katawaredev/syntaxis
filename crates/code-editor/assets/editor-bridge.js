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
const byteToCodeUnit = (value, byteOffset) => {
    let bytes = 0;
    let codeUnits = 0;
    for (const character of value) {
        const width = encoder.encode(character).length;
        if (bytes + width > byteOffset) break;
        bytes += width;
        codeUnits += character.length;
    }
    return codeUnits;
};
const undoStack = [];
const redoStack = [];
const historyLimit = 200;
const historyByteLimit = 16 * 1024 * 1024;
let pendingHistory = null;
let caretScrollFrame = null;
let selectionFrame = null;
let composing = false;
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
const keepCaretLineVisible = lineIndex => {
    const editor = input.closest(".dxc-editor");
    const highlight = input.parentElement?.querySelector(":scope > .dxc-editor-highlight");
    if (!(editor instanceof HTMLElement) || !(highlight instanceof HTMLElement)) return;
    const lines = highlight.querySelectorAll(":scope > .dxc-editor-line");
    if (lines.length === 0) return;
    const lineHeight = Number.parseFloat(getComputedStyle(input).lineHeight) || 22;
    const target = lines[Math.min(lineIndex, lines.length - 1)];
    if (!(target instanceof HTMLElement)) return;

    const editorRect = editor.getBoundingClientRect();
    const viewport = window.visualViewport;
    const visibleTop = Math.max(editorRect.top, viewport?.offsetTop ?? 0);
    const visibleBottom = Math.min(
        editorRect.bottom,
        (viewport?.offsetTop ?? 0) + (viewport?.height ?? window.innerHeight),
    );
    if (visibleBottom <= visibleTop) return;

    const targetRect = target.getBoundingClientRect();
    const missingLines = Math.max(0, lineIndex - (lines.length - 1));
    const top = targetRect.top + (missingLines === 0 ? 0 : targetRect.height + (missingLines - 1) * lineHeight);
    const height = missingLines === 0 ? Math.max(targetRect.height, lineHeight) : lineHeight;
    // A wrapped logical line can be taller than the viewport. Without exact
    // textarea caret geometry, moving it wholesale would cause a jump.
    if (height > visibleBottom - visibleTop) return;
    const margin = lineHeight;
    if (top + height + margin > visibleBottom) {
        editor.scrollTop += top + height + margin - visibleBottom;
    } else if (top - margin < visibleTop) {
        editor.scrollTop -= visibleTop - (top - margin);
    }
};
const queueCaretVisibility = lineIndex => {
    keepCaretLineVisible(lineIndex);
    if (caretScrollFrame !== null) cancelAnimationFrame(caretScrollFrame);
    caretScrollFrame = requestAnimationFrame(() => {
        keepCaretLineVisible(lineIndex);
        caretScrollFrame = requestAnimationFrame(() => {
            keepCaretLineVisible(lineIndex);
            caretScrollFrame = null;
        });
    });
};
const emit = () => {
    const start = input.selectionStart ?? 0;
    const end = input.selectionEnd ?? start;
    const before = input.value.slice(0, start);
    const lineStart = before.lastIndexOf("\n") + 1;
    const line = before.split("\n").length;
    queueCaretVisibility(line - 1);
    dioxus.send({
        start: byteOffset(start),
        end: byteOffset(end),
        line,
        column: start - lineStart + 1,
        selection_count: Math.max(1, ranges.length),
        ranges: ranges.map(([rangeStart, rangeEnd]) => ({
            start: byteOffset(rangeStart),
            end: byteOffset(rangeEnd),
        })),
    });
};
const queueSelection = () => {
    if (selectionFrame !== null) cancelAnimationFrame(selectionFrame);
    selectionFrame = requestAnimationFrame(() => {
        selectionFrame = null;
        if (!composing) emit();
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
const applyCommand = command => {
    if (!command || typeof command.kind !== "string") return;
    if (command.kind === "focus") input.focus();
    if (command.kind === "select") {
        input.focus();
        input.setSelectionRange(
            byteToCodeUnit(input.value, command.start),
            byteToCodeUnit(input.value, command.end),
        );
        input.dispatchEvent(new Event("select", { bubbles: true }));
    }
    if (command.kind === "replace") {
        const value = command.value;
        if (typeof value !== "string") return;
        const start = byteToCodeUnit(value, command.start);
        const end = byteToCodeUnit(value, command.end);
        const before = captureHistory();
        ranges = [];
        input.focus();
        applyValue(value, start, end);
        commitHistory(before);
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
        input.dispatchEvent(new Event("select", { bubbles: true }));
    }
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
    if (event.isComposing || composing) return;
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
    if (event.isComposing || composing) return;
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
    if (composing) return;
    if (pendingHistory) {
        const before = pendingHistory;
        pendingHistory = null;
        commitHistory(before);
    }
    // Software keyboards do not reliably emit keyup. Report the updated caret
    // after the delegated Dioxus input handler has committed the controlled
    // value. Emitting synchronously here can re-render an old value on iOS.
    queueSelection();
};
const onCompositionStart = () => {
    composing = true;
    pendingHistory = captureHistory();
};
const onCompositionEnd = () => {
    composing = false;
    if (pendingHistory) {
        const before = pendingHistory;
        pendingHistory = null;
        commitHistory(before);
    }
    queueSelection();
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
input.addEventListener("compositionstart", onCompositionStart);
input.addEventListener("compositionend", onCompositionEnd);
input.addEventListener("select", onSelection);
input.addEventListener("keyup", onSelection);
input.addEventListener("click", onSelection);
emit();
while (true) {
    const command = await dioxus.recv();
    if (command === true) break;
    applyCommand(command);
}
input.removeEventListener("keydown", onKeyDown);
input.removeEventListener("beforeinput", onBeforeInput);
input.removeEventListener("input", onInput);
input.removeEventListener("compositionstart", onCompositionStart);
input.removeEventListener("compositionend", onCompositionEnd);
input.removeEventListener("select", onSelection);
input.removeEventListener("keyup", onSelection);
input.removeEventListener("click", onSelection);
if (caretScrollFrame !== null) cancelAnimationFrame(caretScrollFrame);
if (selectionFrame !== null) cancelAnimationFrame(selectionFrame);
