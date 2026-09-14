# Shared-module migration record

This document records the characterized baseline and the completed implementation of the
shared-module architecture. It is evidence for the migration, not a second product specification.

The measurements and validation results below describe the original migration, not the later
review fixes. Product paths now use `server` and `browser`, with remote adapters in
`runtime-remote`. Follow-up changes must be validated independently.

## Source baseline

- Migration baseline: `89c04e5` (`fix: git`)
- Architecture-spec baseline: `d2df279`
- Canonical product behavior and visuals: the `syntaxis` main application
- Canonical screenshots: the tracked `screenshots/` references for Home, Files, Terminal, Git,
  Preview, and AI at desktop widths

The server-backed composition is `apps/server`, package `syntaxis-server`; its Dioxus output
is `target/dx/syntaxis-server/`. The browser-only composition is `apps/browser`, package
`syntaxis-browser`. Public routes are shared. No compatibility aliases or browser-storage
migration are provided for the earlier product names.

## Final package structure

The root manifest is a virtual workspace. Both executables are composition roots:

- `apps/server` installs document assets, authentication startup, remote runtime services, and
  `syntaxis_app_shell::SyntaxisApp`.
- `apps/browser` installs static document assets, the browser runtime services, and the same
  `SyntaxisApp`.
- `crates/app-shell` owns Home, the workspace shell, notifications, the only `Routable` enum, and
  selection of all five shared module entry components.
- `crates/module-{files,terminal,git,preview,ai}` own feature UI, controllers, models, and ports.
- `crates/runtime-remote` owns remote adapters, Dioxus server-function stubs and their
  feature-gated server handlers, host selection, and the interactive-terminal bridge lifecycle.
  It is not a native runtime: the client half runs in WASM, while the server half delegates
  native operations to domain host crates.
- `crates/runtime-browser` owns OPFS/browser bridge DTOs, browser adapters, and idempotent loading
  and version checks for its generated bridges.

`scripts/check-architecture.sh` enforces those dependency and ownership boundaries in CI.

## Routes

Both applications mount the same route enum and route components.

| Surface | Shared route |
| --- | --- |
| Home | `/` |
| Files | `/workspaces/:slug/files?:..query` |
| Terminal | `/workspaces/:slug/terminal?:..query` |
| Git | `/workspaces/:slug/git` |
| Preview | `/workspaces/:slug/preview` |
| AI | `/workspaces/:slug/ai?:..query` |
| AI settings redirect | `/workspaces/:slug/ai/settings` |
| AI settings section | `/workspaces/:slug/ai/settings/:section` |

The settings redirect targets the canonical section route in both runtimes.

## Runtime capabilities

Optional behavior is represented by an absent typed port or by a typed capability value. Shared UI
does not branch on product identity.

| Area | Remote/host runtime | Browser-local runtime |
| --- | --- | --- |
| Workspace sources | Registered roots, folders, clone, project bootstrap, management | Private OPFS workspace, local-folder picker, bounded ZIP import/export |
| Files/editor | Host-backed files, search, sessions, file watching, LSP, transfers | OPFS/local-folder files, bounded search and transfers; no host LSP/watch service |
| Terminal | Interactive socket transport, renderer, sessions, command discovery and mutation | Bounded cancellable `just-bash` command runner and command discovery; no PTY |
| Git | Full repository, conflict, history, branches, network, merge/rebase, tags, and worktrees as provided by the host | Local repository, diff, stage/commit, history, checkout, and branches; no network/conflict/merge/rebase/tag/worktree ports |
| Preview | Target configuration, process lifecycle, gateway leases, refresh, and sharing | Sandboxed bounded static HTML preview; no process/config/share ports |
| AI | Streaming Pi conversations plus optional usage, auth, resources, extensions, worktrees, and notifications | Bounded cancellable provider HTTP streams, model selection, and in-memory BYOK settings; optional server-management ports absent |
| Authentication | Server session and sign-out | No authentication action |

The browser terminal snapshots at most 32 MiB total and 8 MiB per file. Archive operations enforce
entry, per-file, total-size, path, and reserved-metadata limits. AI conversations and event queues
are bounded; cancellation reaches both main and browser adapters. Cross-module filesystem changes
publish through the bounded `WorkspaceEventBus`; lag forces an authoritative resync, while dirty
editor buffers retain conflict semantics. Bridge-load failures are timeout-bounded and surface as
typed offline errors; shared modules never receive script URLs or select bridge implementations.

## Build and validation matrix

Run these from the repository root. In the managed container, `mise` supplies the pinned tools.

| Surface | Command |
| --- | --- |
| Server product web client and server debug build | `dx build --package syntaxis-server --platform web` |
| Browser web debug build | `dx build --package syntaxis-browser --platform web --debug-symbols false` |
| Native server check | `just check server` |
| Shared/domain/adapter tests | `just test "" web` |
| Browser/JavaScript tests | `just test-web` |
| Architecture boundaries | `just architecture` |
| Optimized size report | `just bundle-report` |
| Optimized size gate | `just bundle-check` |
| Complete validation | `mise run qa` |

CI builds the main web/server surface and guest WASM surface, runs host and JavaScript tests, runs
the guest Chromium smoke, checks generated files, enforces architecture boundaries and bundle
budgets, and builds the production container.

## Browser and visual evidence

The tracked `screenshots/` directory remains the canonical main visual reference; the extraction
made no deliberate visual redesign. The existing main autoresearch workload passed against the
optimized app for Home/Recent Projects, New Project, Files/editor readiness, and Git diff. Its
320x700 mobile and 1440x900 desktop audits found no horizontal overflow, console error, page error,
or failed request. A focused optimized Chromium smoke also created a server terminal and verified
that the versioned remote renderer bridge loaded and mounted xterm without browser failures.

`autoresearch/browser-smoke.mjs` is the browser-local integration check. It opens the OPFS
workspace through a real bounded ZIP import and opens every shared module route. It opens a file in
the shared editor, renders an imported HTML document and stylesheet through the sandboxed static
Preview adapter, executes and cancels commands through the Terminal adapter, and initializes a
repository through the Git adapter. Its intercepted provider exercises progressive AI deltas,
request cancellation, the 1 MiB response limit, streaming request configuration, and browser-local
authorization. The check also verifies the versioned archive, Terminal, and Git bridges, exercises
the AI settings deep link, fails on unexpected browser errors, and rejects any local flow that
attempts a Syntaxis `/api/` request. It passed against both the completed debug and optimized
release guest artifacts.

## Release WASM comparison and budgets

The baseline release artifacts were reconstructed from commit `89c04e5` with Dioxus CLI 0.7.10,
the pinned lockfile, `--release --locked --debug-symbols false`, and generated assets reproduced
from that commit. Compression uses gzip level 9 and Brotli quality 11.

| Client | Baseline raw | Final raw | Change | Baseline Brotli | Final Brotli | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Main | 7,309,211 | 6,572,411 | -10.1% | 1,742,228 | 1,558,369 | -10.6% |
| Guest | 3,228,545 | 5,237,154 | +62.2% | 841,377 | 1,296,133 | +54.0% |
| Combined | 10,537,756 | 11,809,565 | +12.1% | 2,583,605 | 2,854,502 | +10.5% |

The guest increase is the measured cost of replacing its smaller parallel feature implementations
with the canonical shell and five shared modules, including the richer Files, Terminal, Git,
Preview, and AI flows required by the architecture. Main shrank despite moving the same UI into
libraries. Dioxus route splitting remains unavailable, so this migration does not rely on it.

No release threshold was committed before implementation; the old record explicitly left it
pending. The permanent completion budgets therefore use the measured final artifacts with less
than six percent headroom for toolchain noise and small follow-up changes:

| Client | Raw limit | Brotli limit |
| --- | ---: | ---: |
| Main | 6,900,000 | 1,640,000 |
| Guest | 5,500,000 | 1,370,000 |

The source measurements and limits are checked in at `scripts/bundle-budgets.json`.
`scripts/report-bundle-sizes.mjs release --check` fails when either raw or Brotli bytes exceed its
limit, and `just bundle-check` builds both release clients before invoking that gate.

## Phase completion

| Phase | Result |
| --- | --- |
| 0 — Baseline | Source/routes/capabilities/screenshots characterized; guest smoke and reproducible release comparison added. |
| 1 — Explicit apps | Virtual workspace and two composition-only apps use one shared route, Home, shell, and navigation. |
| 2 — Services/adapters | Typed service graph, errors, navigation intents, bounded event bus, main runtime, browser runtime, and in-memory test adapters established. |
| 3 — Files | One shared Files/editor module owns startup, tree, documents, sessions, search, mutations, uploads/transfers, conflicts, and UI. Guest duplicate controllers were removed. |
| 4 — Terminal | One shared Terminal module owns interactive and command-runner presentations; runtime transports and renderer are injected ports. |
| 5 — Git | One shared Git module owns repository UI and workflows; capabilities are split into optional port groups and browser bridge DTOs remain runtime-local. |
| 6 — Preview | One shared Preview module owns its lifecycle and presentations; main process/gateway and browser static-document behavior are adapters. |
| 7 — AI | One shared AI module owns conversations/settings and optional panels over normalized progressive streams; both adapters are bounded and cancellable. |
| 8 — Cleanup/enforcement | Parallel guest/app-local feature trees and CSS were removed, runtime ownership was tightened, architecture and bundle CI gates were added, and final size evidence was recorded. |

The implementation phases are complete. The repository's full `mise run qa` workflow is the final
validation gate and is intentionally run only after explicit confirmation because it affects Rust
and build configuration validation.
