# Development and maintenance

This page is for contributors. Operators installing a release should use
[Getting started](getting-started.md).

Syntaxis has two Dioxus 0.7 applications with a shared UI: a server-backed app and
a standalone browser app. Both compile browser UI to WebAssembly, but only the
server-backed app uses the server's filesystem, native processes, and Pi RPC.
Read the [Runtime guide](runtimes.md) before changing runtime-sensitive behavior.

## Repository layout

The server-backed composition lives in `apps/server/src/`, and the browser-only composition in
`apps/browser/`. Both mount the same `app-shell` and feature modules.
Workspace crates separate shared types from host implementations:

- `code-editor` and `editor` — editor integration and state;
- `terminal` and `terminal-host` — terminal contracts and processes;
- `git` and `git-host` — Git types and operations;
- `agent` and `agent-host` — Pi RPC and process lifecycle;
- `workspace` and `workspace-host` — files, sessions, and workspace registry;
- `lsp-host` — language-server processes;
- `notifications` and `notifications-host` — notification support;
- `ui` — shared components.

Authored browser bridge sources live with their owners under
`crates/code-editor/bridge-src/`, `crates/runtime-remote/bridge-src/`, and
`crates/runtime-browser/bridge-src/`. Their generated bundles are crate-local assets.

## Setup

The project uses [Mise](https://mise.jdx.dev/):

```bash
mise trust
mise install
mise run setup
```

Review `mise.toml` before trusting an unfamiliar checkout. It declares Rust and the WebAssembly
target, Node.js, Bun, Dioxus CLI, Just, Lefthook, quality tools, and development language servers.
Keep duplicate Rust, Node, Bun, and Dioxus versions aligned when upgrading.

The WebAssembly build includes a C shim and requires a C/C++ toolchain and Clang. The development
container includes them. Local Mise overrides belong in ignored `mise.local.toml`.

## Common tasks

```bash
mise run serve:server   # server-backed web app, loopback
mise run serve:browser  # standalone browser app, loopback
mise run serve:lan      # LAN debug app; password if configured, warning otherwise
mise run check          # non-mutating web checks
mise run check:server   # non-mutating server checks
mise run qa             # fix and validate the web build
mise run qa:server      # fix and validate the server build
mise run ci             # complete audit
```

Use `just --list` for lower-level tasks. Run tools through `mise run` or `mise exec`; do not assume
shell activation persists between commands.

Prefer `just serve-server [host] [port]` and `just serve-browser [host] [port]`.
Old launch names such as `just web` and `just serve-local` have been removed,
without aliases. Dioxus's `web` target does not select the standalone browser
runtime. See the [command map](runtimes.md#launch-commands) for launch recipes.

`serve-server` disables debug login on loopback and explicitly enables it on other
addresses. `just serve-lan [port] [host]` binds `0.0.0.0` by default and requires
the password when `SYNTAXIS_PASSWORD_HASH` is set (including via `.env`). Otherwise
it warns and disables debug login. Without login, all reachable clients get
workspace and shell access; use only a trusted LAN, preferably with a specific LAN
bind address. If UFW is installed, the recipe temporarily allows the TCP port
using sudo and cleans up its rule on exit, preserving existing matching rules.
Release authentication is unchanged. For example:

```bash
just serve-server                 # http://127.0.0.1:8080, server workspace
just serve-browser 127.0.0.1 8081   # separate standalone browser app
just serve-lan 8080 192.168.1.10    # substitute your own trusted LAN IP
```

## Generated assets

```bash
just build-assets
```

This builds the CodeMirror, terminal, and browser runtime bundles (including Pi AI)
and regenerates server Pi settings metadata. The settings
generator reads the pinned Pi package and writes `crates/runtime-remote/src/ai/generated_settings.rs`. It validates every
curated setting and setter on each run and hashes only the extracted metadata, so unrelated Pi
documentation changes do not churn the generated Rust file. Runtime capability checks are per setter,
not tied to Pi's version number.

The pre-commit task refreshes generated assets and checks formatting. Compilation and tests remain
explicit tasks and run in pull-request CI.

## Docker development

```bash
SYNTAXIS_PASSWORD_HASH='$argon2id$v=19$...' docker compose up --build
```

Open `http://localhost:8080`. The development Compose file mounts the checkout and host projects and
persists the runtime home and Cargo caches.

Authenticate Pi with:

```bash
docker compose exec syntaxis pi
```

Override paths and IDs through the environment:

```bash
PUID="$(id -u)" PGID="$(id -g)" \
HOST_HOME="$HOME" HOST_PROJECTS="$HOME/Projects" \
SYNTAXIS_DEV_PORT=8080 docker compose up --build
```

The development configuration uses an insecure local cookie. Never expose it to the internet.

## Validation

Quality tasks cover formatting, Clippy, compilation, tests, doctests, dependencies, and generated
assets. `qa` applies safe fixes; use `check` when the tree must not change.

For Rust, manifest, or build-configuration changes, run on a machine with adequate resources:

AI agents must first obtain explicit approval as required by root `AGENTS.md`.

```bash
mise run qa
mise run qa:server
```

Documentation-only changes do not require the Rust workflow.

### WASM bundle budgets

`just bundle-check` builds both release web clients and checks their raw and Brotli-compressed
WASM sizes. `just bundle-report` prints the same measurements without enforcing limits.
These measurements exclude JavaScript, styles, and other assets.

The workspace's `wasm-release` profile uses `opt-level = "z"` with release LTO and a single
codegen unit. Dioxus selects this profile for both apps' WASM clients; the native server keeps
the normal release profile. This favors download size over execution speed, so assess interactive
performance when changing it. Compare measured artifacts before increasing
`scripts/bundle-budgets.json`; both raw and compressed sizes must fit their limits.

On Rust 1.98.0 / Dioxus CLI 0.7.10, switching from Dioxus's default `s` to `z` reduced the
browser client from 6,286,257 to 5,276,710 raw bytes and from 1,535,295 to 1,383,673 Brotli
bytes. Its raw budget remains 5,500,000 bytes; the Brotli budget is 1,450,000 bytes, allowing
about 5% headroom over that measurement. The server client measures 6,222,429 raw bytes and
1,593,058 Brotli bytes, within its existing limits. The migration report records older,
historical budgets.

## Lighthouse

```bash
just lighthouse
just lighthouse-open
```

The first command builds the optimized fullstack web release, starts it on `127.0.0.1:4173`, and runs
three mobile-emulated audits. Reports are written to `lighthouse-reports/`; the second command opens
the newest report.

Local scores vary with hardware, load, and browser version. Compare repeated runs on the same machine
and prefer deployed field data.

## Updates

```bash
just update          # compatible updates
just update latest   # include major updates
```

Updates are interactive. Review the diff, align duplicate version declarations, and validate web and
server builds. Regenerate curated setting metadata when the pinned Pi package changes; deployed Pi
updates use its public SDK and per-setting capability detection.

## Releases

The release workflow uses Conventional Commits and release-please. Merging the generated release pull
request updates version files and the changelog, creates the matching GitHub release, and publishes:

```text
ghcr.io/katawaredev/syntaxis:<version>
ghcr.io/katawaredev/syntaxis:latest
```

A reusable publish workflow can recover a failed container publication from a matching version and
Git ref.

## Contributions

Keep changes focused and describe the user problem. Check interface work at both a narrow phone
viewport and a keyboard-driven desktop viewport. For host operations, consider cancellation,
bounded output, path validation, cleanup, and browser disconnects.

Update user documentation with new behavior and its limits. Do not describe implementation checks as
stronger isolation than they provide.
