# Syntaxis Guest maintenance notes

## Product boundary

Syntaxis Guest is the static, browser-only sibling of Syntaxis. It has no
application server and keeps browser-only dependencies out of the main binary.
The browser workspace is authoritative.

The guest application now covers the browser-compatible product surface:

- OPFS and optional File System Access API workspaces.
- File browsing, bounded search, create, upload, download, rename/move, copy,
  recursive delete, and ZIP import/export.
- Text editing with conflict-aware saves, multiple tabs with independent dirty
  buffers, image/binary previews, and dirty-navigation protection.
- A bounded local `just-bash` command console with cancellation, history,
  workspace reconciliation, and open-buffer conflict handling.
- Sandboxed static HTML preview with bounded local asset inlining.
- Offline source-history snapshots with change status, local commits, and
  explicit two-step restore.
- Optional provider-neutral BYOK AI chat through an OpenAI-compatible HTTPS
  endpoint, with an opt-in active-file attachment and an in-memory-only key.
- Static deployment configuration for Vercel or any equivalent host.

## Intentional browser limitations

These are platform boundaries rather than unfinished guest features:

- The terminal is a bounded command console, not a PTY. Interactive processes,
  native executables, package installation, and network commands are absent.
- Browser source history is not an interoperable `.git` repository. Branches,
  remotes, signing, rebase, worktrees, and provider authentication require a
  native/server Git runtime. The UI labels this distinction explicitly.
- Static preview does not run framework development servers. It renders a saved
  HTML document in a restrictive sandbox; scripts and network access remain
  disabled.
- AI is disabled until the user enters an API key and submits a request. Calls
  go directly to the chosen provider and therefore depend on that provider's
  CORS policy. No static deployment can safely hide a shared secret.
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
- Require an explicit user action before sending file contents to an AI
  provider.
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
8. Browser-history initialize/status/commit/restore, empty workspaces, and
   history-size eviction.
9. AI without a key, invalid endpoints, provider errors, CORS rejection,
   context opt-in, and context limits.
10. Mobile explorer overlay, keyboard navigation, notices, and responsive
    module navigation.

## Generated assets and release workflow

The ZIP and terminal bridges are generated and ignored by Git. From the
repository root:

```sh
just build-guest-archive build-guest-terminal
dx serve --package syntaxis-guest --platform web
```

Release artifact:

```sh
dx build \
  --package syntaxis-guest \
  --platform web \
  --release \
  --locked \
  --debug-symbols false
```

The output is `target/dx/syntaxis-guest/release/web/public`. The included
`build-vercel.sh` builds both generated bridges and copies that static artifact
to `apps/guest/dist`.

Repository policy requires explicit user confirmation before running the full
Rust/build validation workflow after code or build-configuration changes.
