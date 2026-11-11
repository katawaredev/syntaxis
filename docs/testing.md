# Testing

Use the smallest check that exercises the changed behavior. Test observable
contracts, failure paths and regressions; avoid repeating derived defaults,
upstream catalog snapshots, exact documentation prose or CSS class choices.

## Automated checks

| Area | Command or location |
| --- | --- |
| Authored JavaScript, browser bridges, Pi compatibility, benchmark helpers | `bun run test:web` |
| Browser AI loop/catalog | `bun test crates/runtime-browser/bridge-src/ai` |
| Browser terminal/Wasmer adapter | `bun test crates/runtime-browser/bridge-src/terminal` |
| Browser Git against a local Git remote | `bun test crates/runtime-browser/bridge-src/git` |
| Public Pi SDK and Node RPC startup | `bun test tests/pi-integration.test.js` |
| Benchmark helpers only | `bun run autoresearch:test` |
| Dependency direction and ownership | `just architecture` |
| Rust tests, checks, Clippy, formatting and doctests | `just qa` / `just qa server` |
| Release WASM budgets for both apps | `just bundle-check` |

JavaScript tests need installed dependencies, Node and Git. Pi checks use temporary
agent directories and no real credentials. Provider calls and Git HTTP transport
are mocked; Git operations still use a real local repository. Keep cross-language
serialization and runtime-boundary checks even when their fixtures are small.

Follow root `AGENTS.md`: AI agents need explicit confirmation before running the
Rust/build validation workflow, which auto-fixes code. In a container use
`mise run qa` or `mise run qa:server`; otherwise use native project tools.
Documentation-only edits do not need this workflow.

## Browser integration

Build the browser app, serve its static output on port 4174, then run:

```sh
bun run autoresearch:browser-smoke
```

Set `BROWSER_URL` for another origin and `AUTORESEARCH_BROWSER_PATH` for Chromium.
For an existing debug distribution, use `bun run autoresearch:browser-smoke:dist`.
The harness imports a ZIP into OPFS, traverses shared modules, checks editor,
preview, terminal and Git behavior, and exercises mocked AI streams/cancellation.
It fails on unexpected browser errors or browser-local requests to Syntaxis APIs.

`bun run build:browser-terminal` followed by `bun run autoresearch:wasmer-smoke`
checks the real Wasmer SDK, worker assets, package cache, execution, bounds and
missing-isolation fallback. It downloads packages; it does not prove editor/OPFS
reconciliation or deployed headers. See [Wasmer maintenance](browser-wasmer.md#verification-and-maintenance).

Server Home/editor/Git interaction measurements use the
[performance harness](../autoresearch/README.md). Android pairing, shared-home and
packaging checks are documented in the [Android guide](../apps/Android/README.md).

## Browser acceptance

Unit tests cannot establish browser permission, storage, IME or deployment behavior.
For relevant changes, exercise Chromium and available Firefox/Safari builds:

- OPFS reload persistence, quota errors, binary/Unicode paths and ZIP round-trips.
- Selected-folder grant, reconnect, revocation and OPFS fallback.
- Dirty buffers, pending saves, external/terminal edits and navigation protection.
- Command cancellation/timeouts, protected paths and workspace reconciliation.
- Wasmer cold/cached downloads, cleanup and deployed cross-origin isolation.
- Static preview local assets and restrictive sandbox behavior.
- Git authentication/CORS failures, configured proxies, branch switches and remote
  operations; unsupported capabilities must remain unavailable.
- AI key changes, discovery failures/retry, streaming cancellation, workspace
  instructions and key/chat loss on reload.
- Narrow phone layout, soft-keyboard resizing, IME/composition and keyboard-only
  navigation. The `maximum-scale=1` viewport is an intentional Safari editor
  workaround; do not remove it merely to improve an audit score.

Preserve runtime bounds: browser handles and keys stay inside their adapters,
command/guest networking stays disabled, and filesystem mutations retain conflict
checks and editor notifications. See [Architecture](architecture.md) and the
[runtime guide](runtimes.md) before changing those contracts.
