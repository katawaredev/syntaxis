# Syntaxis Browser

Syntaxis Browser is the backend-free, static sibling of the self-hosted
application. It builds as a separate WebAssembly artifact, so its browser-only
dependencies do not increase the server-backed Syntaxis bundle.

It provides the browser-compatible Syntaxis workspace experience over the
Origin Private File System (OPFS), with optional direct access to a
user-selected local folder in browsers that support the File System Access API.
It uses no server functions; small generated JavaScript bridges provide the
browser-only ZIP, Git, and command runtimes.

## Run locally

From the repository root:

```sh
just serve-browser
```

## Build static files

```sh
just build-assets
dx build --package syntaxis-browser --platform web --release --locked --debug-symbols false
```

The static deployment output is generated under
`target/dx/syntaxis-browser/release/web/public`.

## Deploy on Vercel

Create a Vercel project with `apps/browser` as its Root Directory and enable
access to source files outside that directory. The included `vercel.json` runs
`build-vercel.sh`, which builds from the Cargo workspace and copies only the
static public artifact into `apps/browser/dist` for deployment. Select **Other**
as the framework preset, keep **Build Command** as `sh build-vercel.sh` and
**Output Directory** as `dist`, and leave **Install Command** empty (the build
script installs the locked JavaScript dependencies).

The script provisions Rust using `rust-toolchain.toml`, installs the WASM target,
and downloads the pinned Dioxus CLI 0.7.10 Linux binary with SHA-256 verification
when a matching CLI is unavailable. It also installs Bun 1.3.14 if missing and
builds all browser bridges, including AI. Node.js must be available in the build
image for the JavaScript build scripts. Tool downloads live under `target/vercel-tools`;
the first build also downloads Rust and Cargo dependencies. No server runtime or
provider API keys are needed for this static build.

## Current scope

- Browse a lazy, expandable file tree backed by the shared editor tree model.
- Hide bulky generated folders by default with an explicit explorer toggle.
- Open a real local directory in supporting browsers.
- Remember selected directory handles in IndexedDB and restore access when the
  browser permits it.
- Create and recursively delete files and directories.
- Copy and move files or directory trees through the shared workspace API.
- Open, edit, and save text files.
- Keep multiple text files open with independent dirty buffers and protected close/navigation.
- Preview HTML files in a sandboxed iframe with bounded local asset support.
- Upload/download files and import/export the workspace as a bounded ZIP archive.
- Run bounded `just-bash` commands locally and reconcile their file changes
  back to the active browser workspace.
- Detect conflicting writes using the shared Syntaxis file-version model.
- Reuse the shared CodeMirror editor, file-tree viewport, AI chrome, and terminal run menu.
- Use a real local Git repository for status, staging, commits, history, branches,
  diffs, and checkout operations.
- Detect package.json, Justfile, and Makefile commands; native-runtime commands remain visibly disabled.
- Use optional BYOK AI chat with provider credentials configured under AI Settings.
- Access the shared, scrollable file explorer on desktop and mobile layouts.

The browser terminal snapshots at most 32 MiB total and 8 MiB per file. Common
generated directories (`node_modules`, `target`, `dist`, `.git`, and similar)
remain visible but their contents are protected and omitted from the bounded
snapshot. It is a command console rather than a PTY: shell variables and
working-directory state reset between commands, interactive processes are
unavailable, and network access is disabled.

The browser product stores and mutates ordinary `.git` metadata through isomorphic-git.
The browser runtime does not advertise a Git network capability, so remote
management, fetch, pull, publish, and push controls are absent. SSH, credential
helpers, GPG signing, rebase, worktrees, hooks, and partial-hunk staging still
require the native/server Git runtime. AI keys
remain in memory, are configured at `/workspaces/<slug>/ai/settings/provider-accounts`, and go
directly to the selected provider, so that provider must allow browser CORS. ZIP import/export is
intentionally merge-only: existing workspace paths are never overwritten by an
import.
