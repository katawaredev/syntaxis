# Maintaining the Pi integration

For setup and supported behavior, use [Pi integration](pi-management.md) and
[Browser AI](browser-ai.md). This page records the seams to review on upgrades.

## Version and source map

Keep the Pi AI, agent-core, schema and coding-agent pins aligned in `package.json`.
Docker and Android packaging derive the CLI pin from that file. Updating the repo
does not update an existing host installation; `SYNTAXIS_PI_COMMAND` selects its
executable. Avoid changing the user's global Pi while updating project packages.

| Concern | Syntaxis source |
| --- | --- |
| Server process startup and RPC | `crates/agent-host/src/lib.rs`, `protocol.rs` |
| Pi records, history and model metadata | `crates/agent-host/src/protocol_mapping.rs` |
| Host SDK resolution, auth and settings | `crates/runtime-remote/src/ai/api/server/management/` |
| Browser agent loop and model catalog | `crates/runtime-browser/bridge-src/ai/bridge-source.js` |
| Essentials metadata generator | `scripts/generate-pi-settings.mjs` |

`just build-assets` rebuilds the browser bridge and generates Essentials metadata
from the installed, pinned Pi docs. The generator checks every exposed setter;
host capability detection is per setter so an older installation disables only
unsupported controls. Edit the generator, not `generated_settings.rs`.

## Upgrade contracts

- Server startup waits for all initial RPC replies. `agent_end` ends a low-level
  run; only `agent_settled` means retries, recovery and queued work have finished.
- Browser instructions belong in a named transcript section. Replace/remove it
  when workspace files change without discarding tool results or adding duplicate
  updates. `finishTurn` stops at the completed 20-turn boundary.
- Browser tool schemas use `pi-schema`. Optional-null handling matters: an explicit
  nullable numeric union can coerce an invalid zero offset into null. Keep line
  bounds and atomic batch-edit limits covered by regressions.
- OpenRouter discovery keeps one cache per key, shares in-flight requests and
  preserves the last success on failure. Pi's personalization helper falls back
  to an unfiltered catalog on 401/403/404; Syntaxis intentionally reports an error.
  Picker limits and the next request must use the same account snapshot.
- Use Pi's price conversion, retain prompt tiers, and keep conditional/automatic
  router prices distinct from free models. RPC session stats expose a numeric
  total without message-level billing certainty; keep its label as an estimate.
- `durationMs` is optional on tool completion/history. Preserve recorded values;
  do not invent timings for old sessions or calls that never executed.
- Browser provider factories remain explicit. OAuth, native extensions, MCP and
  background cache warming belong to the server's full coding agent.

Focused checks live beside the browser bridge and in `tests/pi-integration.test.js`
(public SDK and production Node RPC startup) and `tests/pi-settings.test.js`
(Markdown table parsing). They use isolated state and mocked providers, not user
credentials. See [Testing](testing.md) for commands and the Rust QA approval rule.

## Deliberate boundaries

Pi Durable is a separate harness, not a coding-agent session migration. Before
adopting it, define transactional storage, cancellation, one-owner coordination
and transcript import. Keep credentials out of durable state and never replay
writes or external actions without proven idempotency. Crash tests must cover the
gap between a side effect and its checkpoint. Browser persistence cannot keep an
agent running after the tab closes.

A full Pi CLI inside Wasmer would also need a separate credential/session policy,
RPC bridge and explicit network transport. Keep the current browser library loop
and optional workspace executor until those are actual requirements.
