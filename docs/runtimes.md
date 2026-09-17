# Runtime guide

This is the contributor and AI-agent reference for choosing the correct app.
Both apps use the same Dioxus UI; seeing a browser window does not identify the runtime.

| Concern | Server-backed app | Standalone browser app |
| --- | --- | --- |
| Entry point | `apps/server`, package `syntaxis-server` | `apps/browser`, package `syntaxis-browser` |
| Adapter | `crates/runtime-remote` and host crates | `crates/runtime-browser` and browser crates |
| Launch | `just serve-server` | `just serve-browser` |
| Workspace | Ordinary server directories | Browser-private storage or an explicitly selected local directory |
| Terminal | Native server processes | Limited just-bash sandbox, rooted at `/workspace` |
| AI | Pi coding-agent process over RPC | Pi AI/agent-core browser libraries with workspace tools |
| Provider login | Pi-managed API keys and supported OAuth/subscriptions | API keys held in app memory; no subscription/OAuth login |
| AI state | Pi server sessions/configuration | Chats, credentials, preferences, defaults held in memory; reload clears them |
| AI resources | Project and global Pi instructions, skills, prompts, extensions | Workspace Markdown instructions, skills and templates; no native extensions/global resources |
| Model availability | Server Pi snapshot; not necessarily live provider discovery | Bundled Pi catalog filtered to configured keys, plus cached OpenRouter account discovery |
| Skill discovery | Server-side skills.sh search; leaderboard requires configuration | External skills.sh link; its current search API lacks browser CORS permission |

The browser development server serves app assets; it does not turn the standalone
app into the server-backed runtime. Browser workspace files, including Markdown AI
resources, persist with the workspace even though browser chat state does not.
See [Browser AI](browser-ai.md) for exact paths, limits, model refresh behavior,
and the distinction between Markdown guidance and executable capabilities.

## Rules for changes

1. Identify the app and adapter before debugging. Do not confuse a `web` compilation
   target with the standalone browser app.
2. Inspect port capabilities in `module-*` and composition in each runtime. Shared
   UI edits need both runtimes considered, even when only one reported the bug.
3. Preserve separation: browser keys must not use server auth, and the browser must
   not quietly call a server to provide a missing native feature.
4. Keep unsupported features explicit. A browser sandbox is not a native terminal;
   Markdown skills do not enable arbitrary Pi extensions or subscription login.
5. Follow the validation approval rule in root `AGENTS.md`. Do not interpret a task
   rename or documentation update as permission to launch services or run QA.

## Launch commands

| Command | Meaning |
| --- | --- |
| `just serve-server [host] [port]` | Server-backed web development; loopback defaults, no debug login on loopback |
| `just serve-browser [host] [port]` | Standalone browser development; loopback defaults |
| `just serve-lan [port] [host]` | LAN debug server; configured password required, otherwise warns and bypasses login; defaults to all IPv4 interfaces |
| `just serve-server-platform [platform] [host] [port]` | Explicit advanced server-backed Dioxus platform selection |
| `just serve-desktop`, `just serve-mobile` | Existing server-package desktop/mobile launch targets |

Old launch names (`web`, `browser`, `serve-local`, `desktop`, `mobile`, and generic
`serve`) are removed, without aliases. Use the explicit names above in docs and
automation. `build`, `check`, `qa`, and their platform parameters retain their
existing meanings; they are build/validation targets, not app launch selectors.

`serve-lan` explicitly enables login when `SYNTAXIS_PASSWORD_HASH` is nonempty,
even if the environment requests an auth bypass. An invalid configured hash fails
startup; it does not fall back to password-free access. Without a hash, it disables
debug login and prints a warning. When UFW is installed, it uses sudo to open the
TCP port temporarily and removes its rule on exit; an existing matching allow
rule is preserved. A private network is not an authorization boundary:
any reachable device can access files and execute commands as the server user.
Bind a specific LAN address when possible, do not forward this port publicly, and
use authenticated deployment for untrusted networks. Release builds continue to
ignore `SYNTAXIS_AUTH_DISABLED`. See [Security](security.md).

## Browser deployment

The standalone browser app can be deployed as static files on Vercel with
`apps/browser` as the project root and access to source files outside that root
enabled. Its build script provisions Rust/WASM and Dioxus CLI, builds the browser
JavaScript bundles, and stages `apps/browser/dist`. See the
[browser deployment instructions](../apps/browser/README.md#deploy-on-vercel).
This deployment does not run the server app.
The Vercel build injects Web Analytics only into the browser deployment artifact
when `VERCEL=1`; enable Web Analytics in that Vercel project's dashboard.
Ordinary browser builds and the server app have no analytics integration.

## Browser Git

The browser Git toolbar keeps Connection in the synchronization button's menu;
Add remote remains the default when no remote exists. Connection configures the
browser identity and network credentials separately from the server runtime.
Staged and unstaged text changes show line counts against HEAD and the index,
respectively; binary changes have no text line counts.

Browser commits support amending HEAD, including message-only amendments, while
preserving the original author. Native commit hooks and commit signing are not
supported, so skip-validation and signing controls are unavailable. Amending
published commits rewrites history; browser Git does not support force pushes.
