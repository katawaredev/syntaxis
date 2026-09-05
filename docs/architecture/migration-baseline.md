# Shared-module migration baseline

This document records the observable starting point for the shared-module architecture migration.
It is a characterization record, not a second product specification.

## Source baseline

- Migration baseline: `89c04e5` (`fix: git`)
- Architecture-spec baseline: `d2df279`
- Difference: the migration baseline includes the follow-up browser Git parity work in
  `apps/guest/src/app/git.rs`, `assets/guest-git/bridge-source.js`, and `syntaxis-ui`.
- Canonical product behavior: the `syntaxis` main application.

The structural move to `apps/main` intentionally preserves the package name `syntaxis`, Cargo
features, Dioxus output directory, server binary name, and public routes.

## Build and validation matrix

| Surface | Command | Expected package/features |
| --- | --- | --- |
| Main web client | `mise run check` | `syntaxis`, `web` |
| Main server | `mise run check:server` | `syntaxis`, `server` |
| Main development server | `mise run serve` | `syntaxis`, default web platform |
| Guest web client | `dx build --package syntaxis-guest --platform web` | `syntaxis-guest`, `web` |
| Full requested QA | `mise run qa` | workspace web checks, tests, and doctests |

The root manifest is now a virtual workspace. Main-specific Dioxus commands must select
`--package syntaxis`; guest commands already select `--package syntaxis-guest`.

## Route characterization

| Product location | Main route | Guest route at baseline |
| --- | --- | --- |
| Home | `/` | `/` |
| Files | `/workspaces/:slug/files?:..query` | `/workspaces/:slug/files?:..query` |
| Terminal | `/workspaces/:slug/terminal?:..query` | `/workspaces/:slug/terminal?:..query` |
| Git | `/workspaces/:slug/git` | `/workspaces/:slug/git` |
| Preview | `/workspaces/:slug/preview` | `/workspaces/:slug/preview` |
| AI | `/workspaces/:slug/ai?:..query` | `/workspaces/:slug/ai` |
| AI settings redirect | `/workspaces/:slug/ai/settings` | `/workspaces/:slug/ai/settings` component |
| AI settings section | `/workspaces/:slug/ai/settings/:section` | `/workspaces/:slug/ai/settings/:section` |

The shared route extraction must retain the main query models and redirect behavior. Guest route
wrappers may temporarily adapt their smaller state model, but no new route shapes should be added.

## Guest capability characterization

| Area | Supported in the browser guest | Intentionally unavailable or constrained |
| --- | --- | --- |
| Workspace sources | Private OPFS workspace, local-folder picker when supported, ZIP import/export | Registered server roots, server project bootstrap, account sync |
| Files/editor | Text editing, explorer operations, bounded browser search, image/Markdown preview, session-local state | Language servers and server file watching |
| Terminal | Detected command execution through the browser command runtime, cancellation, structured output | Interactive PTY, arbitrary native processes, server terminal sessions |
| Git | Local repository/status/diff/history/commit operations through the browser bridge; HTTPS remotes when CORS or a trusted proxy permits | SSH transport, host credential helpers, operations unsupported by the browser engine |
| Preview | Static HTML prepared from workspace files with bounded local assets | Process discovery/control, gateway leases, public sharing |
| AI | Browser-side provider configuration and HTTP chat requests; credentials remain in browser storage | Server Pi sessions, worktrees, extensions, server resources, provider login flows |
| Authentication | No logout action | Server authentication/session management |

Capability claims must be converted to optional ports or typed capability values during each
vertical extraction. This table documents behavior; it must not become a boolean feature matrix in
shared UI code.

## Existing visual and smoke evidence

The tracked `screenshots/` directory provides the canonical desktop reference for Home, Files,
Terminal, Git, Preview, and AI. The `autoresearch` browser workload covers main startup, workspace
selection, file opening, and editor rendering at desktop and mobile viewport sizes. Guest smoke
coverage still needs to be added before guest routing or chrome is deleted.

Existing debug artifacts at the baseline were:

| Artifact | Bytes | Notes |
| --- | ---: | --- |
| Main web WASM | 275,821,256 | Debug, uncompressed; not a regression budget |
| Main server | 563,277,032 | Debug binary; not a regression budget |
| Guest web WASM | 141,284,580 | Debug, uncompressed; not a regression budget |

Release and compressed WASM budgets remain pending. They should be recorded after the first
approved validation/build run and before shared feature code begins moving.

## Phase status

- Phase 0: source, routes, commands, capabilities, existing screenshots, and provisional artifact
  sizes recorded. Guest browser smoke coverage and release-size budgets remain open.
- Phase 1: main executable moved into `apps/main`; Files and AI route query models are shared and
  the guest AI settings route now follows the canonical redirect/section model. Full shell and
  navigation adoption remain open until feature entry points leave the app binaries.
- Phase 2: typed errors, navigation intents, stable application services, runtime composition
  packages, and a bounded workspace event bus are in place. Main watcher events publish into the
  bus, and Files consumes a workspace-scoped bounded subscription with exact-path coalescing and
  explicit authoritative resync on lag. The browser runtime now composes concrete
  OPFS Files, bounded search, and browser-session adapters. The main runtime now owns desktop host
  Files composition, the desktop registry singleton, and the remote Files adapter/transport
  mapping. Dioxus endpoint declarations remain in the main composition package so their server
  bodies retain the existing authorized workspace lookup; broader non-Files runtime extraction
  remains open.
- Phase 3: `syntaxis-module-files` owns the initial Files port bundle and normalized search
  contracts, shared matching and bounded filesystem traversal, and reusable in-memory test
  adapters. Guest workspace search, main Explorer search, and AI file-mention search now consume
  the port through `AppServices`; browser, desktop, and remote-server runtimes select adapters over
  the same contracts and matching implementation. The old main search endpoint and matching copy
  have been removed. Canonical main Files initialization, session persistence, explorer traversal,
  document I/O, mutations, uploads, and post-Git reloads also consume the injected port bundle; the
  obsolete combined bootstrap endpoint and app-local file-operation selection wrappers have been
  removed. `syntaxis-module-files` now owns the narrow cross-module Files UI state, source-reference
  formatting, debounced session persistence controller, workspace-event inbox, canonical open
  document/view models, private document/selection controller state, edit application, dirty-close
  decisions, active-tab repair, typed version-checked saves/reloads, and external-change buffer
  reconciliation. Validated file create/copy/move/delete use cases, mutation outcomes, dialog
  destination suggestions, and open-tab rename propagation are shared as well. Git and AI consume
  only the narrow published snapshot instead of importing the controller's full signal graph; the
  former app-local revision bridge has been removed. The browser guest now resolves the runtime's
  Files port bundle for uploads and runs create/copy/move/delete through the same validated mutation
  use cases and request models as the main application. Archive import remains browser transfer
  infrastructure rather than ordinary explorer mutation behavior. Shared Files startup now owns
  root listing, optional root editor-configuration discovery, and session loading; the main app
  wrapper adds only its transitional Git status decoration. Lazy directory loading and scoped
  `.editorconfig` discovery are shared use cases as well, leaving the main explorer responsible
  only for applying the returned state to its transitional signals. Document classification,
  bounded text/image loading, editor-buffer construction, missing restored-tab handling, and the
  merge/order policy that protects documents opened while restoration is running are shared too;
  the app wrapper temporarily converts image bytes into its existing preview source. Ordinary
  uploads now share picker-name/path validation, declared and actual byte limits, collision policy,
  and port-driven binary writes while retaining the characterized main overwrite and browser
  reject-existing policies. Browser-native file picking and byte reads remain adapter-adjacent, and
  archive import remains separate transfer infrastructure. The remaining canonical controller/UI
  and guest file operations still need to move into the shared module crate. Main and browser
  `AppServices` now both advertise complete required Files port bundles.

## Sequencing note

The proposed plan placed the final shared `Routable` enum before feature extraction. That would
force `app-shell` either to import app-local components or to resolve feature components through a
temporary service locator. The migration instead shares route query contracts now and will move the
actual route enum once the corresponding feature entry points live in shared module crates. This
preserves the target dependency direction throughout the migration.
