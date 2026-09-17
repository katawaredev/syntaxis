# Syntaxis Browser maintenance notes

This is a maintenance reference and recurring release checklist, not a feature
backlog or a record of completed verification. See the [runtime guide](runtimes.md),
[Browser AI](browser-ai.md), and [browser app README](../apps/browser/README.md).

## Product boundary

Syntaxis Browser is the static, browser-only sibling of Syntaxis. It has no
application server and keeps browser-only dependencies out of the server product.
The browser workspace is authoritative.

The browser application now covers the browser-compatible product surface:

- OPFS and optional File System Access API workspaces.
- File browsing, bounded search, create, upload, download, rename/move, copy,
  recursive delete, and ZIP import/export.
- Text editing with conflict-aware saves, multiple tabs with independent dirty
  buffers, image/binary previews, and dirty-navigation protection.
- A bounded local `just-bash` command console with cancellation, history,
  workspace reconciliation, open-buffer conflict handling, and protected
  omission of generated dependency/build directory contents.
- Sandboxed static HTML preview with bounded local asset inlining.
- An interoperable local `.git` repository with status, staging, commits,
  history, diffs, branch checkout, HTTPS clone, remote management, fetch, pull,
  publish, and push.
- Optional BYOK AI chat using Pi browser libraries, multiple providers, model
  selection, streaming, and sandboxed workspace tools. Provider keys and chat
  state are held in memory; see [Browser AI](browser-ai.md).
- Static deployment configuration for Vercel or any equivalent host.

## Intentional browser limitations

These are platform boundaries rather than unfinished browser features:

- The terminal is a bounded command console, not a PTY. Interactive processes,
  native executables, package installation, and network commands are absent.
  Generated directories remain visible as protected empty directories so large
  projects do not make ordinary browser-shell commands unusable.
- Browser Git uses ordinary local `.git` metadata. HTTPS networking depends on
  the host's CORS support or an explicitly configured trusted CORS proxy.
  Connection settings and credentials are separate from the server runtime.
  SSH, credential helpers, signing, rebase, hooks, partial-hunk staging,
  worktrees, and force pushes are unsupported in the browser.
- Static preview does not run framework development servers. It renders a saved
  HTML document in a restrictive sandbox; scripts and network access remain
  disabled.
- AI requests use user-supplied provider keys and go directly to the provider,
  so they depend on its CORS policy. Model discovery may run when a model
  selector opens; submitting a prompt starts the agent and its workspace tools.
  No static deployment can safely hide a shared secret.
- OPFS is origin-scoped and subject to browser eviction. ZIP export remains the
  portability and backup mechanism.
- Direct local-folder access depends on the File System Access API and browser
  permission rules. OPFS remains available where the picker is unsupported.

## Correctness and security invariants

- Keep filesystem operations behind `WorkspaceFiles` and validate every path
  through `RelativePath`.
- Never expose directory handles to shell or AI code.
- Keep file, workspace, archive, command-output, execution-time, history, and AI
  context limits in place.
- Keep shell network access disabled.
- Never persist an AI API key or place it in workspace files, terminal state,
  logs, URLs, or error telemetry.
- Start AI runs only through an explicit user request. A run may read workspace
  instructions and files through its tools and send that context to the selected
  provider; preserve the documented tool boundaries and limits.
- Preserve `FileVersion` conflict checks and reconcile terminal mutations with
  open editor buffers.
- Keep the preview iframe sandbox restrictive unless a capability has a
  documented browser threat model.
- Source-history restore is destructive; retain its two-step confirmation and
  block it while any editor buffer is dirty.

## Browser interoperability checks

Before a release, exercise these paths in a real Chromium browser and, where
possible, Firefox/Safari:

1. OPFS create/read/edit/save/rename/copy/delete and reload persistence.
2. Selected-folder permission grant, reload, reconnect, revocation, and fallback
   to OPFS.
3. Empty directories, binary and Unicode filenames, quota errors, and all size
   limits.
4. ZIP round-trip, malformed archives, duplicate paths, and archive limits.
5. Terminal readiness, mutation reconciliation, cancellation, timeout, output
   cap, and command-history navigation.
6. Multiple dirty tabs, save conflicts, terminal edits to active/inactive tabs,
   and navigation protection.
7. HTML preview reload, local assets, unsupported references, and sandbox
   isolation.
8. Browser Git initialize/status/stage/unstage/commit/diff/history and branch
   switching; HTTPS clone, remote management, fetch, pull, publish, push,
   authentication failures, CORS rejection, and explicitly configured proxy use.
   Confirm unsupported operations remain unavailable.
9. AI without a key, invalid endpoints, provider errors, CORS rejection,
   model selection/discovery, streaming cancellation, workspace tools,
   instructions, skills, context limits, and loss of keys/chat state on reload.
10. Mobile explorer overlay, keyboard navigation, notices, and responsive
    module navigation.

## Generated assets and release workflow

The ZIP, Git, terminal, and AI bridges are generated and ignored by Git. From the
repository root:

```sh
just serve-browser
```

Release artifact:

```sh
just build-assets
dx build \
  --package syntaxis-browser \
  --platform web \
  --release \
  --locked \
  --debug-symbols false
```

The output is `target/dx/syntaxis-browser/release/web/public`. The Vercel-specific
[`build-vercel.sh`](../apps/browser/build-vercel.sh) provisions its build tools,
builds the editor and all four browser bridges, and copies that static artifact
to `apps/browser/dist`. It injects analytics into the staged artifact only when
`VERCEL=1`; ordinary static builds contain no analytics integration. See the
[Vercel deployment instructions](../apps/browser/README.md#deploy-on-vercel).

Repository policy requires explicit user confirmation before running the full
Rust/build validation workflow after code or build-configuration changes.
