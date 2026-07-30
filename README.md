# Syntaxis

Syntaxis is a mobile-first development workspace built with Dioxus. Its workspace includes files,
a code editor, terminal sessions, Git tools, and a focused chat interface for the
[Pi coding agent](https://pi.dev/).

## Code intelligence

The editor supports Mise-managed language servers for Rust, JavaScript and
TypeScript (using the latest TypeScript native LSP, or Deno's built-in LSP for
Deno projects), Python, Go, HTML, CSS/SCSS, JSON, YAML, TOML, shell scripts,
Terraform, PHP, Ruby, Vue, Svelte, and Astro. Tailwind projects additionally
use the Tailwind CSS language server alongside the file's primary server. Open
a supported file and enable **Code Intelligence** from the editor menu to
receive diagnostics and semantic completions.

When a project has no Mise configuration, **Bootstrap** infers both its
toolchain and suitable language servers, installs them, and records them in the
checkout-local `mise.local.toml`. Existing Mise configurations remain
authoritative: Bootstrap trusts and installs their declared tools without
modifying the configuration. Add any desired language-server tool to that
configuration when it is not already declared.

Mise-provided language servers are resolved with `mise which`, and all servers
are launched with `mise exec` inside the workspace. For Node-based servers,
Syntaxis first uses a compatible
executable already installed in the project root's `node_modules/.bin`, while
still applying the Mise runtime environment. Yarn Plug'n'Play and nested
package-local installations currently fall back to the Mise tool. Syntaxis
never accepts an executable or arguments from the browser. Connections and
messages are bounded, and the additional browser module is loaded only while
Code Intelligence is enabled. Syntaxis starts no more than one primary server
and one project-specific supplementary server for the active document;
Tailwind is not started for projects where it was not detected.

## Pi coding agent

The AI workspace uses Pi directly through its native RPC mode. Install and authenticate Pi on the
machine running the Syntaxis server before opening the AI tab:

```bash
curl -fsSL https://pi.dev/install.sh | sh
pi
```

Use Pi's `/login` flow or configure one of its supported provider API keys. Each chat has its own
long-lived `pi --mode rpc` process, so multiple chats and projects can work in parallel even after
you leave the AI screen. The sidebar is rebuilt from Pi's own persisted sessions after a Syntaxis
server restart; selecting a saved chat resumes its transcript with Pi directly.

The host treats `agent_settled` as Pi's authoritative idle boundary, preserves steering and
follow-up queue counts, and batches streaming deltas to at most roughly 30 UI updates per second.
RPC records, structured tool details, extension dialog queues, stderr, tool output, and rendered
transcript rows are explicitly bounded. Up to three settled background chats remain warm per
workspace; older settled processes are stopped and lazily resumed from Pi's session file. Chats
that are still working are never stopped by this limit.

Completed tool output is rendered as safe Markdown and may expose bounded structured arguments and
details. Displayable extension custom messages are retained in the timeline, while blocking
extension dialogs are shown in arrival order. Commands requiring Pi's terminal-only interactive UI
are labeled and rejected with an explanation instead of being submitted as model prompts.

Deleting a chat stops its process and moves the Pi JSONL session to the server operating system's
trash rather than permanently unlinking it.

If `pi` is not on the server's `PATH`, set `SYNTAXIS_PI_COMMAND` to the executable path. Syntaxis does
not embed a model provider, store API keys, or route the AI section through ACP or another agent SDK.
The optional All time, Trending, and Hot skills.sh leaderboards require a
`VERCEL_OIDC_TOKEN`; Syntaxis leaves those controls disabled when the token is
not present.

## Docker

The repository provides separate development and production targets in one multi-stage
`Dockerfile`. Both variants expose the host's projects at `/Projects`, mount SSH configuration
read-only, and mount GnuPG configuration read-write so Git operations behave like the local setup.

### Authentication

Syntaxis requires a single-user password on every fullstack server. Generate an Argon2id password
hash (the password input is hidden), then place the printed PHC string in your shell or Compose
`.env` file:

```bash
just auth-password
SYNTAXIS_PASSWORD_HASH='$argon2id$v=19$...'
```

The web app exchanges that password for a random, 30-day, HTTP-only session cookie. Sessions are
kept in memory and are intentionally invalidated whenever the server restarts. Production cookies
are `Secure` and `SameSite=Strict`; local HTTP development sets `SYNTAXIS_INSECURE_COOKIE=true` in
Compose.

`just web` and `just serve` skip authentication automatically when they bind to a loopback address.
This bypass is accepted only by debug builds. Authentication remains mandatory for release builds
and for `just serve-local`, which exposes the development server to the network.

Native clients can authenticate independently with an optional bearer token:

```bash
SYNTAXIS_API_TOKEN="$(openssl rand -base64 32)"
curl -H "Authorization: Bearer $SYNTAXIS_API_TOKEN" https://code.example.com/api/runtime
```

Store the token in the platform keychain on desktop/mobile, never in source or ordinary app
preferences. Changing `SYNTAXIS_API_TOKEN` revokes all native clients on the next server restart.
The token is optional until a native client is used and must contain at least 32 characters when
set.

### Application previews

The Preview module connects to an HTTP development server from the selected runtime. The default
runtime-port target uses `127.0.0.1`; start the project in Terminal and bind it there. On Linux
runtimes, Syntaxis finds listening HTTP processes whose working directory is inside the selected
workspace and offers their ports as Preview suggestions; manual port entry remains available.

Use the explicit HTTP(S) URL target for an existing remote app or a Docker service reachable from
the Syntaxis runtime, such as `http://frontend:3000`. Only origins are accepted: credentials, paths,
queries, fragments, and non-HTTP schemes are rejected. Syntaxis probes and proxies the target from
the runtime, not from the browser. The selected target is saved per workspace.

The supplied Compose files map `host.docker.internal` to the Docker host. Use
`http://host.docker.internal:<published-port>` for a server published by another local container.
Production services on the configured shared `DOCKER_NETWORK` can instead use their Compose service
name directly.

Syntaxis exposes either target through an authenticated HTTP and WebSocket gateway, so framework hot
reload continues to work without publishing the development port. Preview automatically connects a
reachable saved target, or the only detected workspace server when there is no saved target.

The active preview is kept in runtime memory and restored when the operator returns to Preview or
reloads Syntaxis. If the upstream server becomes unreachable during an HTTP request, Syntaxis
removes the complete preview session; its private and shared URLs remain invalid even if a server
later starts on the same port. Runtime restarts also clear active previews, while retaining saved
targets for automatic reconnection.

Local development opened through `http://localhost` or a loopback IP uses an automatically
generated `p-<private-token>.localhost` hostname. A production installation must set a base preview
origin:

```bash
SYNTAXIS_PREVIEW_ORIGIN=https://preview.example.com
```

Wildcard DNS and TLS for `*.preview.example.com` must route to the same Syntaxis listener. For
example, once wildcard certificate provisioning has been configured, the Caddy route is:

```caddyfile
*.preview.example.com {
	reverse_proxy syntaxis:8080
}
```

The unguessable private hostname is the owner's bearer credential; no gateway cookie or query token
is required. The main Syntaxis session cookie remains host-only and is not sent to the project
development server. Explicit URL targets should still be treated as trusted operator configuration:
an authenticated user who already has runtime Terminal access can use them to reach HTTP services
visible to that runtime.

Previews are private by default. **Share** creates one random bearer link that anyone can open
without a Syntaxis account, using a separate `s-<share-token>` hostname. **Revoke** immediately
invalidates that hostname without interrupting the private preview. Creating a new preview also
invalidates the previous session and its share. Sharing beyond the local machine requires a
publicly reachable `SYNTAXIS_PREVIEW_ORIGIN` with wildcard DNS and TLS as described above.

### Development

The default Compose file mounts the entire host `${HOME}/Projects` directory and starts the Dioxus
development server from `/Projects/syntaxis`:

```bash
SYNTAXIS_PASSWORD_HASH='$argon2id$v=19$...' docker compose up --build
```

Open <http://localhost:8080>. Changes made on the host are visible immediately in the container.
The container home is persisted under `./data/dev-home`; Cargo downloads use named volumes.
Authenticate the bundled Pi CLI once with `docker compose exec syntaxis pi`; its credentials and
sessions remain in the persisted container home.

Pi itself is installed under `/home/dev/.local`, so the **Update everything** button can update Pi
and all installed Pi packages without root access. Packages can contain extensions, skills, prompts,
and themes. Skills installed directly from skills.sh are also refreshed when their recorded source
is available.

The defaults assume UID/GID `1000`. Override paths, IDs, or ports without editing Compose:

```bash
PUID="$(id -u)" PGID="$(id -g)" \
HOST_HOME="$HOME" HOST_PROJECTS="$HOME/Projects" \
SYNTAXIS_DEV_PORT=8080 docker compose up --build
```

### Production

The production target compiles an optimized Dioxus fullstack server and contains only its runtime,
Node.js, Pi, and the command-line tools used by Syntaxis:

```bash
docker compose -f docker-compose.prod.yml build
docker compose -f docker-compose.prod.yml up -d
```

Production Compose joins the external `${DOCKER_NETWORK:-caddy_net}` network and exposes port `8080`
to other containers on that network. Its persistent home defaults to
`${DATA:-./data}/syntaxis/home`. Run `pi` once inside the container to authenticate; the resulting
state and user-updated Pi installation survive image upgrades:

```bash
docker exec -it syntaxis pi
```

For `arvigeus.one`, the service can use the published
`${SYNTAXIS_IMAGE:-ghcr.io/katawaredev/syntaxis:latest}` image, keep the same `/Projects`, SSH, and
GnuPG mounts, and proxy Caddy to `syntaxis:8080`. Set `HOST_PROJECTS` to the server's projects
directory; this replaces devbox's old `/workspace` convention.

### Publishing

The `Release` GitHub Actions workflow uses Conventional Commits to maintain a release pull request.
Merging that pull request updates `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`, creates the matching
`v<version>` GitHub release, and publishes `ghcr.io/katawaredev/syntaxis:<version>` and
`ghcr.io/katawaredev/syntaxis:latest`. The reusable `Publish container` workflow can also be run
manually with a version and matching Git ref to recover a failed publication.

# Development

Your new bare-bones project includes minimal organization with a single `main.rs` file and a few assets.

```
project/
├─ assets/ # Any assets that are used by the app should be placed here
├─ src/
│  ├─ main.rs # main.rs is the entry point to your application and currently contains all components for the app
├─ Cargo.toml # The Cargo.toml file defines the dependencies and feature flags for your project
```

### Automatic Tailwind (Dioxus 0.7+)

As of Dioxus 0.7, there no longer is a need to manually install tailwind. Simply `dx serve` and you're good to go!

Automatic tailwind is supported by checking for a file called `tailwind.css` in your app's manifest directory (next to Cargo.toml). To customize the file, use the dioxus.toml:

```toml
[application]
tailwind_input = "my.css"
tailwind_output = "assets/out.css" # also customize the location of the out file!
```

### Tailwind Manual Install

To use tailwind plugins or manually customize tailwind, you can install the Tailwind CLI and use it directly.

### Tailwind
1. Install bun: https://bun.sh
2. Install the Tailwind CSS CLI: https://tailwindcss.com/docs/installation/tailwind-cli
3. Run the following command in the root of the project to start the Tailwind CSS compiler:

```bash
bunx @tailwindcss/cli -i ./input.css -o ./assets/tailwind.css --watch
```

### Serving Your App

Run the following command in the root of your project to start developing with the default platform:

```bash
dx serve --platform web
```

To run for a different platform, use the `--platform platform` flag. E.g.
```bash
dx serve --platform desktop
```

## Lighthouse

Run a complete local audit with:

```bash
just lighthouse
```

This command installs the pinned Lighthouse CI tool when needed, creates an optimized Dioxus
fullstack web build, starts its release server on `127.0.0.1:4173`, and runs Lighthouse three times
with mobile emulation. The server is stopped automatically. The median run is checked against
performance, accessibility, best-practice, SEO, and key loading/responsiveness thresholds.

The terminal output summarizes enforced failures and warning-level improvement targets. Full HTML
and JSON reports are written to `lighthouse-reports/`; open the most recent collected report with:

```bash
just lighthouse-open
```

The audit uses `target/dx/syntaxis/release/web/server`, not the hot-reloading development server or a
standalone static server. The release server provides the server-rendered HTML and hydration data
that the Dioxus fullstack client expects. Debug builds do not represent production asset size or
runtime performance. Local Lighthouse numbers still vary with CPU load, Chrome version, and
hardware, so compare repeated runs on the same machine and treat field data from a deployed site as
the final measure of user experience.
