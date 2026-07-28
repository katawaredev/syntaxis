# Syntaxis `dioxus-code-editor` fork

This crate is the narrow, application-owned editor surface used by Syntaxis.
Editable documents and the Files route's read-only unified diff mode use a
bundled CodeMirror 6 view. Git-page diff fragments retain `dioxus-code`'s
Arborium/tree-sitter renderer so their patch-relative line-number offsets stay
intact.

## Capability spike

The CodeMirror bridge preserves the Rust-facing API required by Syntaxis:

- controlled external values and edit events while CodeMirror owns immediate
  browser editing state;
- cursor/selection reporting, caret-follow scrolling, focus, go-to-line, and
  select commands;
- Tab/Shift-Tab indentation, enter indentation, paired delimiters, and pair
  deletion/skip behavior;
- native multiple selections and vertical cursors;
- line-number and word-wrap modes with the Syntaxis theme integration point;
- viewport-only DOM rendering and incremental Lezer parsing for large files;
- native unified diffs with collapsed unchanged regions and bounded diff work;
- native language-aware and document-word completion;
- optional, lazy-loaded LSP completion and diagnostics through CodeMirror's
  official language-client package, including bounded supplementary clients
  such as Tailwind alongside a primary language server;
- filename-based language detection through CodeMirror's maintained language
  catalog, with plain text for unknown files;
- deterministic event-listener cleanup when the component is dropped.

CodeMirror owns history, input methods, selection painting, incremental syntax
state, and LSP document synchronization. Edit transactions cross the Rust
bridge as UTF-16-relative deltas instead of full document snapshots.
Imperative commands are consumed once, preventing mutations from replaying when
an editor remounts. Per-file search and completion remain inside CodeMirror;
workspace search remains application-owned. The product separately refuses
text files over 4 MiB and renders a clear large-file state.

Run `bun run build:editor` after changing
`assets/code-editor/bridge-source.js`, `package.json`, or `bun.lock`. Standard
`just` build, serve, QA, and pre-commit recipes build the cached bundle
automatically.

Validated targets are Linux native Cargo compilation and the Dioxus web client.
