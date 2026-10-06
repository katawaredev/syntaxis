# Pi adoption review — 2026-10-02

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
