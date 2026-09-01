# Syntaxis Guest: Agent Handoff and Next Steps

## Objective and constraints

Syntaxis Guest is a backend-free, statically deployable sibling application for
visitors who cannot or do not want to run the Syntaxis server. It must remain a
separate WebAssembly artifact so browser-only dependencies do not increase the
main application bundle. This is important because Dioxus does not currently
provide the route-level code splitting needed to make a large guest mode cheap
inside the main binary.

Continue to follow these constraints:

- Use Dioxus 0.7.10 APIs and Rust implementations wherever practical.
- Keep handwritten JavaScript to small, reviewed interoperability bridges.
- Do not add guest-only dependencies to the main `syntaxis` application.
- Preserve a static deployment model suitable for Vercel or another free host.
- Treat browser storage as authoritative; no Syntaxis backend is available.
- Keep network access disabled by default, especially in the terminal.
- Preserve the existing `WorkspaceFiles` boundary instead of adding filesystem
  operations directly to UI components.

## Current implementation

The implementation is present in the working tree and may not be committed yet.
Do not reset or replace unrelated working-tree changes.

### Application

`apps/guest` is a standalone Dioxus web application containing:

- A file browser with directory navigation and refresh.
- File and directory creation.
- Two-click recursive deletion.
- Rename/move and recursive copy/duplicate controls with dirty-buffer guards.
- Bounded binary previews for images and hex previews for other binary files.
- Browser upload and download actions with per-file size limits.
- Whole-workspace ZIP import/export for portable browser storage.
- Bounded Rust workspace search across paths and small text files.
- Text editing through the existing Rust-facing CodeMirror component.
- Conflict-aware saves using `FileVersion`.
- Unsaved-buffer protection during navigation and workspace switching.
- Editor and Terminal panels.
- Multiple text editor tabs with close/reopen behavior and move-path remapping.
- Sandboxed HTML preview with bounded local asset inlining and explicit reload.
- Static Vercel deployment configuration.

The main UI is currently concentrated in `apps/guest/src/app.rs`. Splitting it
into `files`, `terminal`, and shared-state modules is advisable before adding
substantially more UI.

### Browser filesystem

`crates/workspace-browser` implements `WorkspaceFiles` using browser APIs:

- OPFS is the default private workspace.
- The File System Access API can activate a user-selected local folder.
- Selected directory handles are stored in IndexedDB using structured cloning.
- On reload, granted handles reopen automatically. Handles requiring a new user
  gesture appear as a **Reopen folder** action.
- Text and binary reads/writes, listing, stat, creation, recursive copy/move,
  and recursive deletion are implemented.
- All paths pass through the shared `RelativePath` validation boundary.

No handwritten JavaScript filesystem adapter is used. Browser APIs are called
through `web-sys` and small `wasm-bindgen` declarations.

### Browser terminal

`crates/terminal-browser` and the guest terminal UI provide a bounded command
console backed by pinned `just-bash` 3.4.2:

- The terminal is not a PTY. Each submission is one isolated command execution.
- Before execution, Rust creates a binary-safe snapshot of the active workspace.
- After execution, Rust validates and reconciles the resulting snapshot into
  OPFS or the selected folder.
- Current limits are 8 MiB per file and 32 MiB for the workspace snapshot.
- Execution is limited to 10 seconds and 512 KiB combined command output.
- Network, Python, JavaScript execution, SQLite, and interactive processes are
  not enabled.
- Terminal output history is capped at 50 records.

The necessary JavaScript is isolated to
`assets/guest-terminal/bridge-source.js`. The generated 1.2 MiB bundle is
guest-only and ignored by Git. It is rebuilt by
`scripts/build-guest-terminal.mjs`. A `node:zlib` shim intentionally makes gzip
commands unavailable because the published browser entry still imports that
Node module.

## Validation status

The following passed on 2026-09-01:

```sh
just qa
```

This included formatting, JavaScript lint, workspace Clippy, 205 tests, and
doctests.

The browser-only paths also passed:

```sh
cargo clippy \
  -p syntaxis-guest \
  -p syntaxis-workspace-browser \
  -p syntaxis-terminal-browser \
  --target wasm32-unknown-unknown \
  --no-default-features \
  --features syntaxis-guest/web \
  -- -D warnings

dx build --package syntaxis-guest --platform web
```

The debug static output was generated at
`target/dx/syntaxis-guest/debug/web/public`.

The repository instructions require asking the user before running the full
validation workflow after Rust or build-configuration changes.

The browser implementation has since gained additional Rust and UI changes;
the validation results above are the last completed baseline, not a result for
the current working tree.

## Recommended next work

### 1. Stabilize the current browser implementation

Do this before adding another major subsystem.

- Add browser integration tests using a real Chromium instance. Cover OPFS
  creation/read/write/delete, a terminal command that modifies files, page
  reload, and persisted-handle permission states.
- Test the guest app in browsers without `showDirectoryPicker`. OPFS and the
  terminal should remain usable, and unsupported controls should explain why
  they are disabled.
- Test empty directories, binary files, unusual Unicode filenames, large-file
  rejection, permission revocation, and storage quota errors.
- Exercise the IndexedDB callback ownership changes under repeated operations;
  handlers now unregister and release themselves after success or failure.
- Exercise the selected-handle persistence warning and permission-reopen paths;
  selection remains usable for the session when persistence fails.
- Exercise the just-bash loading/ready state and cancellation path in a real
  browser; submissions now wait for the bridge before running.
- Verify the release artifact size and load time. The guest-only separation is
  intentional, but the `just-bash` bundle should still be measured and cached.
- Perform an actual Vercel deployment. Confirm that the build image supplies
  Rust 1.98.0, Dioxus CLI 0.7.10, Bun, and access to workspace sources outside
  `apps/guest`. If this is unreliable, build in CI and deploy the static output.

### 2. Complete the workspace/editor experience

- Add rename/move and copy/duplicate controls. The backend operations already
  exist; the guest UI now provides safe, dirty-buffer-aware controls.
- Add multiple editor tabs and close/reopen behavior using the existing editor
  session models where possible.
- Improve tab state by moving from the current active-buffer controller to the
  shared editor session models, so dirty buffers can remain open independently.
- Exercise whole-workspace ZIP import/export with empty directories, malformed
  archives, duplicate paths, and archive bomb limits.
- Extend workspace search with cancellation while preserving its current byte
  and result-count limits.
- Refresh or invalidate an open editor after a terminal command changes its
  file. The current implementation now reconciles the active buffer and
  refreshes an active HTML preview; browser integration coverage is still
  needed.
- Break `apps/guest/src/app.rs` into focused modules and introduce a small
  shared guest-workspace controller rather than passing many independent
  signals through future components.

### 3. Improve terminal semantics without pretending it is a PTY

- Add command history navigation with Arrow Up and Arrow Down.
- Add cancellation using `AbortController` through the narrow bridge.
- Scroll to the latest output after a command finishes.
- Show which files changed and whether reconciliation succeeded.
- Make reconciliation safer. It is currently a validated but non-transactional
  sequence of browser filesystem operations. A mid-apply failure can leave a
  partial result. At minimum, calculate and display the planned changes first;
  ideally stage writes and provide best-effort rollback.
- Detect external changes between the pre-command snapshot and reconciliation,
  especially for selected local folders. Do not silently overwrite a file that
  changed outside the app while the command was running.
- Consider retaining a `just-bash` filesystem between commands only if shell
  behavior requires it. The current recreation is deliberate: the browser
  workspace stays authoritative and `just-bash` documents shell state as
  isolated between `exec` calls.
- Do not add xterm merely for appearance. It becomes worthwhile only if the
  runtime gains streaming input/output or genuinely interactive semantics.

### 4. Add static preview

Start with browser-native web preview rather than framework dev servers:

- Render a selected HTML file in a sandboxed iframe.
- Resolve bounded relative CSS, JavaScript, image, and module paths from the
  browser workspace. A service worker or controlled virtual-origin layer is
  likely more robust than rewriting every URL into a blob URL.
- Explicit reload and auto-reload after saved file changes are implemented;
  browser coverage and richer preview error reporting remain.
- Use a restrictive iframe sandbox initially. Do not grant same-origin,
  navigation, pop-up, or network capabilities without a documented reason.
- Capture preview errors and display them in the guest UI.
- Keep preview state and assets inside the guest app; do not reuse server
  preview endpoints.

Framework preview (`npm run dev`, Vite, Next.js, and similar) is a separate and
much larger problem. `just-bash` cannot execute arbitrary native Node packages,
so do not imply that these commands work. Evaluate browser-specific runtimes
such as WebContainers only as an optional, separately loaded experiment because
they are large, JavaScript-heavy, browser-restricted, and may conflict with the
project's minimal-JavaScript goal.

### 5. Research Git as an isolated spike

Do not begin by porting the server Git UI. First create a tiny proof of concept
that can initialize a repository, inspect status, stage, commit, and produce a
diff against the same browser filesystem.

Evaluate these constraints explicitly:

- A pure Rust Git implementation compiled to WASM is preferred, but WASM size,
  async filesystem integration, and cryptography support must be measured.
- A JavaScript Git implementation may be more mature in browsers but increases
  handwritten/third-party JavaScript and needs an adapter to OPFS/FSA.
- Clone, fetch, and push are commonly blocked by Git server CORS behavior and
  authentication flows. Local-only Git may be the honest first feature.
- Never store access tokens in workspace files or persistent terminal state.
- Keep any chosen Git implementation in a guest-only crate/bundle.

Record bundle size, browser compatibility, supported operations, and remote
limitations before choosing a library.

### 6. Treat AI as optional BYOK functionality

AI is not required for the guest app to be useful. If implemented:

- Start with a provider-neutral Rust interface and one small proof of concept.
- Use a user-supplied API key held in memory by default. Persistent key storage
  requires an explicit user choice and a clear warning.
- Verify provider CORS support; a static host cannot safely hide a secret or act
  as an API proxy.
- Reuse editor/workspace context construction, but cap file count and bytes
  before sending anything off-device.
- Require a visible confirmation showing which files will be transmitted.
- Make the local-only privacy guarantee change visibly when AI is enabled.
- Consider local inference only as a later optional download because model size,
  WebGPU support, and memory requirements conflict with a lightweight guest.

## Important correctness and security notes

- File System Access API handles are capabilities. Do not serialize them outside
  IndexedDB or expose them to the terminal bridge.
- Browser permission prompts must be triggered by a user gesture. Startup may
  query permission but must not request it.
- Keep root modification rejected and validate every bridge-returned path with
  `RelativePath` before touching storage.
- The terminal result is untrusted even though it runs locally. Preserve file,
  workspace, output, execution-time, and history limits.
- Avoid exposing general network access to shell commands. If `curl` is ever
  enabled, use an explicit origin/path/method allowlist with no credentials by
  default.
- Do not place API tokens or local directory handles into `just-bash` files,
  environment variables, output history, or error messages.
- Selected local folders can change outside the app. Continue using file
  versions and add equivalent conflict checks to terminal reconciliation.
- OPFS data is origin-scoped and can be deleted by browser storage eviction or
  user settings. Import/export is necessary before presenting it as durable.

## Build and generated-asset workflow

The `just-bash` browser bundle is generated and ignored by Git:

```sh
just build-guest-terminal
```

The source and build inputs that must be committed are:

- `assets/guest-terminal/bridge-source.js`
- `assets/guest-terminal/zlib-shim.js`
- `scripts/build-guest-terminal.mjs`
- `package.json`
- `bun.lock`
- `Justfile`

Generated bundles are excluded from Oxlint and Oxfmt in `.oxlintrc.json` and
`.oxfmtrc.json`. Do not lint or hand-edit the generated bundle.

Run locally from the repository root:

```sh
just build-guest-terminal
dx serve --package syntaxis-guest --platform web
```

Build a release artifact with:

```sh
dx build \
  --package syntaxis-guest \
  --platform web \
  --release \
  --locked \
  --debug-symbols false
```

## Suggested definition of done for the next agent

The next increment should preferably complete stabilization and one small
workspace feature rather than starting Git or AI. A strong handoff point would
include:

- Chromium integration tests for OPFS and terminal file mutation.
- No IndexedDB callback leaks.
- Explicit terminal bridge readiness and cancellation.
- Immediate open-buffer refresh/conflict UI after terminal changes.
- Rename/duplicate controls or ZIP import/export.
- A successful release build and a tested static deployment.
- `just qa`, WASM-target Clippy, and the Dioxus guest build all passing.
