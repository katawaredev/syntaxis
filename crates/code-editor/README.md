# Syntaxis `dioxus-code-editor` fork

This crate is the narrow, application-owned editor surface used by phase 5. It
retains `dioxus-code`'s incremental Arborium/tree-sitter `Buffer` and pins the
reviewed upstream revision in `Cargo.toml`.

## Capability spike

The upstream textarea component supplied controlled text editing and syntax
highlighting, but not the imperative and selection APIs required by Syntaxis.
This fork adds:

- controlled values and edit events with incremental highlighting;
- cursor/selection reporting, caret-follow scrolling, focus, go-to-line, and
  select commands;
- Tab/Shift-Tab indentation, enter indentation, paired delimiters, and pair
  deletion/skip behavior;
- textarea-backed next/all-occurrence cursors and vertical rectangular cursors;
- line-number and word-wrap modes with the Syntaxis theme integration point;
- deterministic event-listener cleanup when the component is dropped.

History is maintained beside the browser textarea so editor commands such as
search replacement join typing in the same undo/redo stack. Imperative commands
are consumed once, preventing mutations from replaying when an editor remounts.
Search, selection
matching, and completion UI are application-owned because they need workspace
state. Completion candidates combine cached, word-like terminals from the same
enabled Arborium grammar with identifiers near the cursor, so language updates
do not require hand-maintained keyword tables. Suggestions open directly from
the input/selection bridge, including software-keyboard input, while
`Ctrl`/`Cmd`+`Space` remains available. Semantic, project-aware completion
remains an LSP concern. A focused test exercises an incremental edit in a roughly
500 KiB highlighted Rust buffer; the product separately refuses text files over
4 MiB and renders a clear large-file state.

Validated targets are Linux native Cargo compilation and a complete Dioxus web
client/server build. `arborium-tree-sitter` references a C `stderr` symbol that
`wasm32-unknown-unknown` does not export, so the root build supplies a minimal
WASM-only compatibility symbol. This is only reachable on tree-sitter's
terminal allocation-failure diagnostic path.

The browser textarea cannot paint every non-primary selection. Multiple-cursor
edits and counts work, while richer selection decoration is the main reason to
revisit a contenteditable or canvas-backed editor upstream later.
