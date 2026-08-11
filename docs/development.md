# Development and maintenance

This page is for contributors. Operators installing a release should use
[Getting started](getting-started.md).

Syntaxis is a Dioxus 0.7 fullstack Rust application. The browser client compiles to WebAssembly; the
server owns filesystem, terminal, Git, language-server, preview, authentication, and Pi access.

## Repository layout

Application code lives in `src/`. Workspace crates separate shared types from host implementations:

- `code-editor` and `editor` — editor integration and state;
- `terminal` and `terminal-host` — terminal contracts and processes;
- `git` and `git-host` — Git types and operations;
- `agent` and `agent-host` — Pi RPC and process lifecycle;
- `workspace` and `workspace-host` — files, sessions, and workspace registry;
- `lsp-host` — language-server processes;
- `notifications` and `notifications-host` — notification support;
- `ui` — shared components.

Browser-side editor and terminal sources are under `assets/`.

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
mise run serve          # web development server
mise run check          # non-mutating web checks
mise run check:server   # non-mutating server checks
mise run qa             # fix and validate the web build
mise run qa:server      # fix and validate the server build
mise run ci             # complete audit
```

Use `just --list` for lower-level tasks. Run tools through `mise run` or `mise exec`; do not assume
shell activation persists between commands.

Debug `just serve` and `just web` disable login only when bound to loopback. `just serve-local` binds
to the network and keeps authentication enabled.

## Generated assets

```bash
just build-assets
```

This builds the CodeMirror and terminal bundles and regenerates Pi settings metadata. The settings
generator reads the pinned Pi package and writes `src/ai/generated_settings.rs`.

The pre-commit task refreshes generated assets and checks formatting. Compilation and tests remain
explicit tasks and run in pull-request CI.

## Docker development

```bash
SYNTAXIS_PASSWORD_HASH='$argon2id$v=19$...' docker compose up --build
```

Open `http://localhost:8080`. The development Compose file mounts the checkout and host projects and
persists the runtime home and Cargo caches.

Authenticate optional Pi support with:

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

```bash
mise run qa
mise run qa:server
```

Documentation-only changes do not require the Rust workflow.

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
server builds. Updating Pi may make the generated settings form read-only until its schema is
regenerated and pinned.

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
