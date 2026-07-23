# Syntaxis

Syntaxis is a mobile-first development workspace built with Dioxus. Its workspace includes files,
a code editor, terminal sessions, Git tools, and a focused chat interface for the
[Pi coding agent](https://pi.dev/).

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

If `pi` is not on the server's `PATH`, set `SYNTAXIS_PI_COMMAND` to the executable path. Syntaxis does
not embed a model provider, store API keys, or route the AI section through ACP or another agent SDK.
The optional All time, Trending, and Hot skills.sh leaderboards require a
`VERCEL_OIDC_TOKEN`; Syntaxis leaves those controls disabled when the token is
not present.

## Docker

The repository provides separate development and production targets in one multi-stage
`Dockerfile`. Both variants expose the host's projects at `/Projects`, mount SSH configuration
read-only, and mount GnuPG configuration read-write so Git operations behave like the local setup.

### Development

The default Compose file mounts the entire host `${HOME}/Projects` directory and starts the Dioxus
development server from `/Projects/syntaxis`:

```bash
docker compose up --build
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
`${SYNTAXIS_IMAGE:-docker.io/arvigeus/syntaxis:latest}` image, keep the same `/Projects`, SSH, and
GnuPG mounts, and proxy Caddy to `syntaxis:8080`. Set `HOST_PROJECTS` to the server's projects
directory; this replaces devbox's old `/workspace` convention.

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
