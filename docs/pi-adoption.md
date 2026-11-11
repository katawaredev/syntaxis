# Pi adoption review — 2026-11-11

## Pi 1.5.2

Reviewed the stable releases from 1.0.0 through
[1.5.2](https://pi.dev/changelog/releases/1.5.2), including
[1.5.0's schema migration](https://pi.dev/changelog/releases/1.5.0) and
[1.5.1's cache warming and provider changes](https://pi.dev/changelog/releases/1.5.1).
The AI, agent-core, coding-agent, and newly direct schema dependency are pinned
together at 1.5.2. Docker and Android continue to derive their packaged CLI version
from `package.json`; this does not update an existing host Pi installation.

Adopted in Syntaxis:

- **Server cache controls:** Essentials exposes Pi's `cacheWarming` setter with
  `off`, `streaming`, and `idle`. Pi now defaults to `idle`, which can refresh
  eligible prompt caches for up to 30 minutes between runs. These requests cost
  tokens, are included in Pi's session totals, and require a sufficient estimated
  saving. Older host installations disable just this control when its setter is
  absent. The browser does not start background warming requests.
- **Browser schemas:** removed the discontinued `Type` import from `pi-ai` and
  use `pi-schema` builders. Optional fields rely on Pi's optional-null handling;
  a nullable numeric union would let coercion turn an invalid line offset of zero
  into null. Positive line bounds and the 1–100 edit batch bound remain enforced.
- **Browser instruction updates:** Pi's transcript now owns system instructions.
  A named workspace section replaces changed or deleted instructions on later
  prompts, preserves tool results, and avoids adding a patch for unchanged content.
- **Both model pickers:** OpenRouter automatic routers remain paid/variable-price
  choices after Pi's catalog normalizes their negative price placeholders to zero.
  The explicitly free router can still appear as free.
- **Browser account catalogs:** adopted Pi's
  [per-key OpenRouter limits and prices](https://pi.dev/changelog/releases/1.2.0).
  The existing discovery request now updates context limits, maximum output, and
  input/output/cache prices for supported tool-capable chat models. Pi's price
  converter retains prompt-length tiers. Conditional prices remain marked as
  paid even when their base is zero. Missing or invalid limits/rates retain the
  bundled values. The actual next request uses the same account limits as the
  picker, without changing the bundled catalog or sharing metadata across keys.
  Cached reads, one-minute freshness, concurrent discovery, and explicit errors
  remain intact; upstream's unfiltered 401/403/404 fallback is not adopted.
- **Recorded tool execution times:** adopted Pi's
  [`durationMs`](https://pi.dev/changelog/releases/1.1.0) in both runtimes. Tool
  rows show execution time for completed and failed calls. Server history restores
  the recorded value from Pi's tool results; browser chats retain it in memory.
  Older sessions and skipped tools have no timing instead of a guessed duration.

The upgrade also brings newer chat catalogs, cache-preserving thinking-level
changes, cleaner model switches, Anthropic message threads, compaction fixes,
safer subprocess limits,
project-trust fixes, and hardened MCP OAuth handling where the upstream provider
or full coding agent supports them. No additional Syntaxis credentials or proxy
are needed. These inherited changes are not live-provider acceptance claims.

Useful follow-ups, kept outside this dependency update:

- [Configuration schemas](https://pi.dev/docs/latest/settings) could add structured
  completion to Advanced JSON. Existing host installations may expose older
  settings, so select documentation/schema compatibility by installed version.
- `pi auth login` is useful for host provisioning without opening a session.
  `selfUpdateCommand` can support package-manager-owned host installations. Neither
  requires replacing the current provider UI or changing packaged update ownership.
- Provider-reported versus estimated costs and nested codemode calls merit a
  dedicated shared-UI change. Pi's RPC session stats expose one numeric total,
  without the message-level billing-certainty flag; calling that entire total
  provider-billed would require accounting for every contributing usage entry.
  Classifiers, generated images, and Pi Durable still
  need their own capability, rendering, persistence, and acceptance work.

Verification: 63 JavaScript tests passed, including isolated Node and Bun RPC
startup queries, public SettingsManager/ModelRuntime calls, mocked browser
requests, instruction replacement/removal, schema bounds, cancellation, and the
20-turn transcript boundary, account pricing/limits, request output limits,
credential isolation, malformed metadata, conditional pricing, concurrent custom
models, and tool timings matching Pi's retained transcript. Rust regression tests
also cover live/saved tool timings, old sessions, and failed-tool projection;
they remain pending QA confirmation. Node subprocess checks require execution outside
this environment's sandbox: its plain Node stdin fixture fails there too. All
JavaScript bundles and Pi settings metadata regenerated successfully. Rust QA,
Android builds/device checks, real credentials, OAuth, and paid model calls have
not run as part of this review.

## Earlier Pi 1.0 review — 2026-10-02

This review covers [Pi 1.0](https://pi.dev/changelog/releases/1.0.0),
[wasmer.sh](https://www.wasmer.sh/?example=pi), and
[Pi Durable](https://earendil.com/posts/pi-durable/).

## Adopted

The Pi AI, agent-core, and coding-agent packages move together from 0.84.3 to
1.0.0. Docker and Android packaging read the coding-agent pin from `package.json`.
Host installations still need their explicit update step.

The upgrade refreshes provider catalogs, thinking capabilities, and upstream
streaming/authentication fixes. The browser continues to register only its seven
API-key providers and expose chat models. It migrates the removed
`shouldStopAfterTurn` hook to `finishTurn`, retains finalized tools at the
20-turn boundary, and passes an instance-isolated chat ID for provider cache
routing. Ranged reads reduce repeated file context; atomic batch edits allow
several exact replacements in one tool call and one version-checked save.

The settings generator now understands escaped Markdown table pipes, keeping
Pi's union types separate from defaults and descriptions. Server users can
configure the newly built-in MCP/codemode through Pi and Advanced JSON; see
[Pi integration](pi-management.md#built-in-mcp-and-codemode-pi-10).

Verification: 21 focused JavaScript tests passed with mocked provider calls;
the browser bundle and server settings metadata regenerated successfully. An
isolated Bun RPC process answered state, model, and command queries, and Pi's
public SettingsManager/ModelRuntime APIs loaded without provider credentials.
Node subprocess stdin could not be verified in this environment (a plain Node
stdin fixture fails too). Rust/tool-adapter checks and broader browser acceptance
remain pending the repository's QA confirmation; these checks do not claim live
provider, MCP, OAuth, or crash-recovery acceptance. Wasmer's separate SDK checks
are recorded below.

## Wasmer: optional browser executor adopted — 2026-10-03

The [Wasmer Pi example](https://github.com/wasmerio/wasmer-sdk/blob/main/wasmer-sh/README.md)
runs the full CLI and its filesystem/search tools inside a WASIX sandbox using
Edge.js. Its example currently pins Pi 0.87.1 and requires SDK/runtime fixes from
the source checkout. External TCP/DNS uses a WISP WebSocket proxy. The SDK is
alpha, and the full Pi flow is verified upstream in Chromium, not WebKit.

Syntaxis now pins SDK 0.19.0 and adds explicit `wasmer run` commands alongside
just-bash: Python, Edge.js, ripgrep, Bash, exact-version registry packages, and
local WASI modules. SDK assets load on demand; package data uses origin-scoped
browser caching. Commands get separate worker pools, bounded output/time, and
workspace snapshot reconciliation through the existing Rust adapter. Guest
networking is disabled, so this needs no WISP proxy. Browser hosting enables
cross-origin isolation; missing headers preserve the ordinary shell.

Real Chromium SDK checks cover language/tool execution, filesystem and binary
pipe behavior, cancellation, timeouts, file bounds, nonzero exits, malformed
module recovery, and missing-isolation fallback. Broader Rust/app validation,
OPFS/editor acceptance, deployed headers, and Firefox/Safari remain pending.
See [Wasmer browser execution](browser-wasmer.md) for commands, limits, and a
comparison of further scripting, search, compiler, preview, and database uses.
Project aliases in `.syntaxis/wasmer.json`, `wasmer tools` discovery, and
`wasmer prepare` package caching now make checks repeatable without executing
guests during setup. Read-only guests no longer rewrite unchanged shell files.
Focused tests cover configuration validation, entry-count bounds, cleanup errors,
and package-cache reuse after browser reload.

Running a separate full Pi CLI would also require a browser session and credential
policy and an RPC bridge to the shared UI. Keeping our current Pi library loop
and adding execution tools is the smaller first step.

## Pi Durable: useful for recovery, deferred

[Pi Durable](https://earendil.com/posts/pi-durable/) is a separate experimental
harness, not the coding agent's next session format. It provides checkpointed
tasks, concurrent conversations, idempotent submissions, and selective tool
replay. It ships memory, SQLite, and JSONL storage; a browser storage and execution
adapter would still be Syntaxis work. Pi 1.0 removes the previous experimental
harness exports from agent-core; our browser only imports the retained `Agent`.

Its strongest fit is server jobs that must resume after process restarts. A
browser harness could retain chats and pending work, but it cannot keep executing
after the browser closes. Memory storage alone does not give crash durability.
Current Pi RPC sessions and browser chats are therefore unchanged.

Evaluate it behind a separate adapter when durable runs become a requirement.
Define transactional storage, cancellation, one-owner/tab coordination, and a
versioned transcript import strategy first. Keep API keys outside durable state
and require re-authentication after reload. Treat writes, batch edits, shell
commands, and external actions as non-replayable unless their idempotency is
proven; an interrupted action should be reported rather than silently repeated.
Acceptance needs crash/reopen and duplicate-submission tests, including a crash
between a filesystem side effect and its checkpoint. Do not reuse coding-agent
session files as Durable storage or promise automatic migration.
