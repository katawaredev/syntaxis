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

