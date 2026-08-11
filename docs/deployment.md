# Deployment

This is the operator reference for a production Syntaxis instance. Start with
[Getting started](getting-started.md) for the installation walkthrough.

## Model

```text
internet
   |
HTTPS reverse proxy
   |
Syntaxis container :8080
   |-- persistent home
   |-- mounted projects
   |-- optional SSH configuration (read-only)
   `-- optional GnuPG configuration (read/write)
```

The container runs as a non-root numeric UID/GID and starts project tools with that user's
permissions. Registered workspaces are not isolated from one another.

## Production Compose

`docker-compose.prod.yml` defaults to:

- `ghcr.io/katawaredev/syntaxis:latest`;
- host bind `127.0.0.1:8080`;
- project root `/Projects`;
- persistent home `./data/syntaxis/home`;
- UID/GID `1000:1000`;
- restart policy `unless-stopped`.

```bash
docker compose -f docker-compose.prod.yml pull
docker compose -f docker-compose.prod.yml up -d
```

The file can also build the production target from the checkout. Building is unnecessary when using
the published image.

## Configuration

Compose reads these variables from the shell or `.env`:

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `SYNTAXIS_PASSWORD_HASH` | Yes | None | Argon2id hash for browser login |
| `SYNTAXIS_IMAGE` | No | `ghcr.io/katawaredev/syntaxis:latest` | Image or pinned release |
| `HOST_PROJECTS` | No | `${HOME}/Projects` | Host directory mounted at `/Projects` |
| `HOST_HOME` | No | `${HOME}` | Host home containing `.ssh` and `.gnupg` |
| `PUID` / `PGID` | No | `1000` | Numeric runtime user and group |
| `DATA` | No | `./data` | Persistent-data base directory |
| `SYNTAXIS_BIND` | No | `127.0.0.1` | Published host address |
| `SYNTAXIS_PORT` | No | `8080` | Published host port |
| `SYNTAXIS_API_TOKEN` | No | Empty | Bearer token for a non-browser client |
| `SYNTAXIS_PREVIEW_ORIGIN` | No | Empty | Base origin for wildcard previews |
| `VERCEL_OIDC_TOKEN` | No | Empty | Enables optional skills leaderboard views |
| `DOCKER_NETWORK` | No | `caddy_net` | Network used by the Caddy override |

The supplied Compose file sets `SYNTAXIS_PROJECTS_ROOT=/Projects` inside the container.

### Authentication values

Generate the required password hash with:

```bash
docker run --rm -it \
  --entrypoint /app/server \
  ghcr.io/katawaredev/syntaxis:latest \
  hash-password
```

Store the complete hash in single quotes in `.env`. Browser sessions use random, HTTP-only, 30-day
cookies kept in server memory. Restarting Syntaxis invalidates them.

`SYNTAXIS_API_TOKEN` is optional and separate from browser login. It must contain at least 32
characters. Generate one with `openssl rand -base64 32` and store it in the client's platform
keychain. Changing it and restarting Syntaxis revokes the old token.

## Mounts and permissions

The production Compose file mounts:

```text
${HOST_PROJECTS}        -> /Projects         read/write
${HOST_HOME}/.ssh       -> /home/dev/.ssh    read-only
${HOST_HOME}/.gnupg     -> /home/dev/.gnupg  read/write
${DATA}/syntaxis/home   -> /home/dev         read/write
```

The SSH and GnuPG mounts let Git use existing remotes and signing configuration. They are optional:
remove them and configure separate credentials in the persistent Syntaxis home if you do not want to
share host credentials.

The configured UID/GID must be able to traverse and modify the required directories. Do not run the
container as root merely to bypass permission problems.

## HTTPS and reverse proxy

Keep Syntaxis bound to loopback unless it shares a private container network with the proxy.
Production login expects HTTPS.

Host-installed Caddy:

```caddyfile
code.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

For an existing Caddy container, use the provided override:

```bash
docker compose \
  -f docker-compose.prod.yml \
  -f docker-compose.caddy.yml \
  up -d
```

This joins the external network named by `DOCKER_NETWORK`. Route the proxy to `syntaxis:8080`.
Preserve WebSocket upgrades and the original host, and do not place authenticated pages behind a
public cache.

## Application previews

### Targets

A preview target is either:

- a port on the Syntaxis runtime, such as `5173`; or
- an HTTP(S) origin reachable from it, such as `http://frontend:3000`.

Explicit URLs must be origins without credentials, paths, queries, or fragments. For runtime ports,
Syntaxis tries IPv4 and then IPv6 loopback. Bind development servers to loopback or a wildcard
address, for example:

```bash
npm run dev -- --host 127.0.0.1
```

The supplied Compose files map `host.docker.internal` to the Docker host. Use
`http://host.docker.internal:<port>` for a service published there, or a Compose service name for a
service on a shared network.

### Public origin

Set a dedicated preview origin:

```dotenv
SYNTAXIS_PREVIEW_ORIGIN=https://preview.example.com
```

Create wildcard DNS at that exact suffix:

```text
*.preview.example.com.  A  203.0.113.10
```

Configure a wildcard certificate, normally with DNS-01 validation, and route the hostname to
Syntaxis:

```caddyfile
*.preview.example.com {
    reverse_proxy syntaxis:8080
}
```

A proxy route does not create DNS or obtain a wildcard certificate. Verify the complete path:

```bash
dig +short test.preview.example.com
curl -I https://test.preview.example.com
```

An HTTP `401 Unauthorized` for that made-up label is expected: routing works, but the preview token
is invalid.

### Privacy and lifetime

Private and shared preview hostnames contain bearer tokens. Anyone with the complete URL can access
the preview while it remains valid. Do not expose these URLs in logs, screenshots, or messages.

The main Syntaxis cookie is host-only and is not sent to the project application. A shared preview
does not grant workspace access.

Revoking a share leaves the private preview running. Creating a new preview invalidates previous
private and shared URLs. Active preview leases are lost on a Syntaxis restart, while the selected
target is retained for reconnection.

The gateway is not a sandbox for hostile applications. Configure only trusted targets.

## Persistent data and backups

The persistent home contains installed tools plus optional Pi credentials, settings, packages, and
sessions. Pi can update itself there, so its data survives container replacement.

Back up:

- the host project directory;
- `${DATA}/syntaxis/home`;
- deployment configuration and secrets;
- any separate host credentials.

Use a filesystem snapshot, stop the container, or use a backup tool that handles live data. Git does
not protect uncommitted work, ignored files, runtime configuration, or Pi sessions.

## Updates and rollback

For repeatable deployments, pin an image version:

```dotenv
SYNTAXIS_IMAGE=ghcr.io/katawaredev/syntaxis:0.7.0
```

Before updating, read the [changelog](../CHANGELOG.md) and back up projects and persistent data. Then
run:

```bash
docker compose -f docker-compose.prod.yml pull
docker compose -f docker-compose.prod.yml up -d
```

Check `/health`, login, and the main workspace functions. Roll back by restoring the previous image
tag and, if persistent data changed incompatibly, its matching backup.

## Development Compose

`docker-compose.yml` builds the development target, mounts the source checkout, enables an insecure
local cookie, and persists development caches. It is for a trusted local machine only. Never expose
it to the internet; use `docker-compose.prod.yml` for deployments.
