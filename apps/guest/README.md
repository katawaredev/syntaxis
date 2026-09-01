# Syntaxis Guest

Syntaxis Guest is the backend-free, static sibling of the self-hosted
application. It builds as a separate WebAssembly artifact, so its browser-only
dependencies do not increase the main Syntaxis bundle.

It provides the browser-compatible Syntaxis workspace experience over the
Origin Private File System (OPFS), with optional direct access to a
user-selected local folder in browsers that support the File System Access API.
No server functions or handwritten JavaScript filesystem bridge are used.

## Run locally

From the repository root:

```sh
just build-guest-archive build-guest-terminal
dx serve --package syntaxis-guest --platform web
```

## Build static files

```sh
dx build --package syntaxis-guest --platform web --release --locked --debug-symbols false
```

The static deployment output is generated under
`target/dx/syntaxis-guest/release/web/public`.

## Deploy on Vercel

Create a Vercel project with `apps/guest` as its Root Directory and enable
access to source files outside that directory. The included `vercel.json` runs
`build-vercel.sh`, which builds from the Cargo workspace and copies only the
static public artifact into `apps/guest/dist` for deployment. The Vercel build
image must provide the pinned Rust toolchain and Dioxus CLI; alternatively run
the same script in CI and deploy the resulting static directory.

## Current scope

- Browse OPFS directories.
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
- Reuse the existing Rust-facing CodeMirror editor component.
- Track offline source-history snapshots with status, commits, and confirmed restore.
- Use optional BYOK AI chat through a user-configured OpenAI-compatible HTTPS endpoint.
- Access the file explorer on desktop and mobile layouts.

The browser terminal snapshots at most 32 MiB total and 8 MiB per file. It is a
command console rather than a PTY: shell variables and working-directory state
reset between commands, interactive processes are unavailable, and network
access is disabled.

Browser source history intentionally does not claim `.git` compatibility:
branches, remotes, signing, rebase, and worktrees require the native/server Git
runtime. AI keys remain in memory and requests go directly to the selected
provider, so that provider must allow browser CORS. ZIP import/export is
intentionally merge-only: existing workspace paths are never overwritten by an
import.
