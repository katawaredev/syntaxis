# Architecture

Read the [runtime guide](runtimes.md) before changing runtime behavior. Both apps
mount the same UI; capabilities supplied by their adapters determine what works.

## Ownership

| Location | Owns |
| --- | --- |
| `apps/server` | Server composition, authentication startup, document assets |
| `apps/browser` | Static browser composition and document assets |
| `apps/Android` | WebView connection/pairing shell for the server app |
| `crates/app-shell` | Shared routes, Home, workspace chrome, service composition |
| `crates/module-*` | Feature UI, controllers, normalized models, and ports |
| `crates/ui` | Visual components without feature state or runtime dependencies |
| `crates/runtime-remote` | Client transports, Dioxus server functions, host-service selection |
| `crates/runtime-browser` | Browser adapters, bridge DTOs, generated bridge loading |
| Domain crates (`workspace`, `editor`, `git`, `agent`, `terminal`) | Shared contracts and state |
| `*-host` / `*-browser` crates | Platform implementations of domain contracts |

Remote adapters have both a WASM client half and feature-gated server handlers.
Native filesystem, processes, Git, LSP and Pi operations belong in host crates.
Shared modules must not call server functions, load JavaScript bridges, or select
implementations by URL or runtime name. Use absent optional ports or typed
capabilities. `AppServices::validate` rejects incomplete service graphs before
mounting the UI. `scripts/check-architecture.sh` enforces the dependency direction.

The shared route enum is in `crates/app-shell/src/route.rs`. Add routes there,
then supply runtime behavior through ports rather than parallel app components.

## Files and editor state

- `WorkspaceFiles` and `RelativePath` are the file boundary. Browser directory
  handles and server paths stay inside their adapters.
- Saves compare `FileVersion`; external changes must not overwrite dirty buffers
  or edits made while a save is pending.
- Filesystem mutations publish through `WorkspaceEventBus`. A lagged subscriber
  must resync from authoritative state rather than assume it saw every event.
- Shell/AI mutations use the same conflict checks and editor notifications.
- `code-editor` owns CodeMirror and its UTF-16 transaction bridge. Shared Files UI
  owns workspace search, buffers, tabs and save/navigation decisions.

## Browser bridges

Authored sources live in each owner's `bridge-src`; generated bundles live in
that crate's ignored `assets` directory. `just build-assets` refreshes them.
Adapters own idempotent loading, version checks, timeouts and cleanup. Shared UI
receives ports and typed errors, never script URLs or filesystem handles.

Load editor, LSP, terminal and AI code only when needed. Diff highlighting reuses
the CodeMirror module; adding a second parser stack to Rust can inflate initial
WASM even when its UI is route-local. Measure raw and compressed artifacts with
`just bundle-check`; limits live in `scripts/bundle-budgets.json`.

Browser-specific bounds and credentials are documented in
[Browser AI](browser-ai.md) and [Wasmer execution](browser-wasmer.md).
[Pi maintenance](pi-maintenance.md) records the upstream compatibility seams.

## Git rebase interaction

Server Git's pull-with-rebase workflow is deliberately non-interactive. Preserve
stopped progress across navigation/reconnect, disable conflicting repository
actions, and keep Abort available. Continue requires resolved conflicts; Skip is
a guarded secondary action. Label conflict sides as upstream and rebased commit
versions: Git's ours/theirs meaning is reversed during rebase. Unsupported binary,
submodule or rename conflicts need an explanation and terminal fallback.

The shared flow lives in `module-git/src/sync.rs` and `dialogs/rebase.rs`; real Git
regressions are in `git-host/src/operations/tests.rs`. Browser Git lacks a rebase
port. A todo editor, commit reordering and general history editing require a
separate interaction design.
