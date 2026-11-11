<!-- markdownlint-disable MD025 MD041 -->

# Runtime identity — read before changing application behavior

Syntaxis has **two runtimes sharing the same UI**, not two names for the same backend.
Read [Runtime guide](docs/runtimes.md) before runtime-sensitive work and
[Browser AI](docs/browser-ai.md) before changing browser AI behavior.

* **Server app:** `apps/server` (`syntaxis-server`) composes `runtime-remote`.
  The UI runs in a browser, but files, native terminals, Git, LSPs, and Pi RPC
  operate on the server. Start with `just serve-server`.
* **Browser app:** `apps/browser` (`syntaxis-browser`) composes `runtime-browser`.
  Workspace files and tools run locally in the browser. Pi uses browser libraries,
  API keys in memory, and sandboxed workspace tools—not the server's Pi process,
  credentials, sessions, or model cache. Start with `just serve-browser`.
* **Android shell:** `apps/Android` loads the server UI in a WebView. Local uses
  the Android backend in Termux; Remote uses an HTTPS server. It never uses
  `runtime-browser`. See `apps/Android/README.md` for build and setup instructions.
* Dioxus `--platform web` describes the UI build target, **not** the browser-only
  runtime. Use explicit recipe names; old launch names have been removed.
* `module-*`, `ui`, and `app-shell` are shared. Changes there can affect both apps.
  Gate runtime-specific features through port capabilities, not labels or URL guesses.
  Keep the two adapters and credential stores separate; do not introduce hidden
  server/proxy dependencies or emulate native/OAuth features in the browser.
* Update the runtime guide and browser AI docs when capabilities, persistence,
  authentication, or launch commands change. State which runtime a fix affects.
* `serve-lan` uses `SYNTAXIS_PASSWORD_HASH` when set; otherwise it warns and disables
  **debug-only** login on a trusted LAN. It temporarily opens the UFW port when
  available. Never extend this bypass to production/release builds.

## Dioxus

Use Dioxus 0.7 APIs and [0.7 documentation](https://dioxuslabs.com/learn/0.7).
`cx`, `Scope`, and `use_state` are obsolete. Use signals and owned component props;
keep browser-only APIs in effects after hydration. Shared feature UI belongs in
`module-*` and reusable visual components in `ui`.

See [Architecture](docs/architecture.md) for ownership and dependency boundaries,
and [Testing](docs/testing.md) for focused checks and browser acceptance.

## Container toolchain

Apply this section only when `/.dockerenv` or `/run/.containerenv` exists. On a
normal development machine, use the developer's existing native tools.

The container has an intentionally minimal `PATH`. Do not first try `just`,
`cargo`, or other project tools directly. From the repository root:

1. Read `mise.toml`.
2. Check `command -v mise`, `mise trust --show`, and `mise current`.
3. If setup is missing, obtain approval before the resource-intensive bootstrap:

   ```bash
   MISE_JOBS=2 mise install
   ```

   Review an untrusted config before running `mise trust mise.toml`.
4. Prefer `mise run qa`, `mise run qa:server`, and `mise run serve:server`; use
   `mise exec -- just <task>` for an unaliased task. Do not rely on interactive
   shell activation persisting between agent tool calls.

## Validation

If your changes affect Rust source code, Cargo manifests, or build/lint configuration, **do not run the validation workflow immediately**.

Instead, ask for confirmation first:

> "The changes affect Rust code or build configuration. Shall I run the validation workflow now?"

After explicit confirmation, run `mise run qa` in a container or `just qa` on the
developer's normal machine.

Do **not** ask for confirmation or run the command after documentation-only changes, asset updates, or other changes unrelated to code quality. The command auto-fixes formatting and Clippy issues, then runs code checks, tests, and doctests, so it should only be used when those checks are relevant.
