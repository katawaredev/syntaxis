# Syntaxis code editor

Application-owned CodeMirror 6 integration for editable files and unified diffs.
Git diff fragments preserve patch-relative line numbers and load highlighting from
the same route-local JavaScript module; plain diff text renders while it loads.

CodeMirror owns immediate editing state, history, IME, selection, incremental
parsing, completion and LSP document synchronization. Rust owns the workspace
buffer/save lifecycle. Transactions cross the bridge as UTF-16 deltas; keep
surrogate boundaries and edits made during pending saves covered by regressions.
Imperative commands are consumed once so remounting cannot replay an edit.

The LSP bridge loads separately and supports a primary server plus supplementary
clients such as Tailwind. Filename-based language selection uses CodeMirror's
catalog and falls back to plain text. Workspace search remains application-owned.
The Files module enforces its text-size limit before mounting the editor.

`bun run build:editor` rebuilds the cached bundles from `bridge-src` and the locked
JavaScript dependencies. Build, serve and pre-commit recipes invoke it automatically.
Keep parser/highlighter dependencies out of initial client WASM and measure both
first-mount latency and compressed asset size when changing this boundary.
