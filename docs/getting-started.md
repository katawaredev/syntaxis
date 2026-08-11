# Getting started

This guide installs one Syntaxis instance on Linux with Docker Compose. Read the
[security model](security.md) before exposing it to a network.

## Requirements

- Docker Engine with the Compose plugin
- A domain and HTTPS reverse proxy
- A host directory containing your projects
- A host user that can read and write that directory

The examples use `code.example.com`, `/home/dev/Projects`, and UID/GID `1000`. Replace them with your
own values.

## 1. Download the deployment files

```bash
git clone https://github.com/katawaredev/syntaxis.git
cd syntaxis
```

The production Compose file uses the published image. A checkout is needed only for the deployment
files and explicit upgrades.

## 2. Generate a password hash

```bash
docker run --rm -it \
  --entrypoint /app/server \
  ghcr.io/katawaredev/syntaxis:latest \
  hash-password
```

The command prints an Argon2id hash beginning with `$argon2id$`. It does not print the password.

## 3. Configure the deployment

Create `.env` beside `docker-compose.prod.yml`:

```dotenv
SYNTAXIS_PASSWORD_HASH='$argon2id$v=19$...complete-hash...'
HOST_PROJECTS=/home/dev/Projects
HOST_HOME=/home/dev
PUID=1000
PGID=1000
SYNTAXIS_BIND=127.0.0.1
SYNTAXIS_PORT=8080
DATA=./data
```

Keep the hash in single quotes so its `$` characters remain literal. Do not commit `.env`.

Create the mounted directories if needed:

```bash
mkdir -p /home/dev/Projects /home/dev/.ssh /home/dev/.gnupg
```

Use `id -u` and `id -g` to find the correct IDs for your host user. Files created by Syntaxis will
belong to that numeric user.

`HOST_PROJECTS` is mounted read/write at `/Projects`. The SSH and GnuPG mounts allow existing Git
credentials and signing to work; remove those mounts from Compose if you prefer separate container
credentials.

## 4. Start Syntaxis

```bash
docker compose -f docker-compose.prod.yml pull
docker compose -f docker-compose.prod.yml up -d
```

Check the service:

```bash
docker compose -f docker-compose.prod.yml ps
curl --fail http://127.0.0.1:8080/health
```

## 5. Add HTTPS

Keep Syntaxis bound to loopback and route your public hostname through an HTTPS reverse proxy. A
minimal host-installed Caddy configuration is:

```caddyfile
code.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

The proxy must support WebSockets. Do not expose the production service as plain HTTP; its login
cookie is intended for HTTPS.

See [Deployment](deployment.md) for containerized Caddy and public preview configuration.

## 6. Open a project

Visit `https://code.example.com`, sign in, and choose:

- **Open folder** to register an existing directory below `/Projects`;
- **Open Git URL** to clone a repository;
- **New project** to run a scaffold command in a terminal.

Registering a folder does not copy it. Removing a workspace keeps its files unless you explicitly
choose to delete them.

For projects without a Mise configuration, **Bootstrap** can infer and install a toolchain and
language servers. Review the proposal before installing anything.

## Optional: authenticate Pi

Pi is included in the image but is not required for the rest of Syntaxis. To use the AI section:

```bash
docker exec -it syntaxis pi
```

Complete Pi's `/login` flow or configure a supported provider. Pi data is stored in the persistent
container home. See [Pi integration](pi-management.md).

## Optional: enable public previews

Local project servers work without additional published ports, but public preview hostnames require
wildcard DNS and TLS. Follow [Application previews](deployment.md#application-previews).

## Updating

Back up projects and persistent data, read the [changelog](../CHANGELOG.md), then run:

```bash
docker compose -f docker-compose.prod.yml pull
docker compose -f docker-compose.prod.yml up -d
```

Pin `SYNTAXIS_IMAGE` to a version tag instead of `latest` when repeatable deployments matter.

## Next steps

- [Features and limitations](features.md)
- [Deployment](deployment.md)
- [Security model](security.md)
