# Security model

Syntaxis can edit source code, open shells, use Git credentials, install tools, run project commands,
and proxy development applications. Treat access to it much like shell access to the runtime.

This page describes the intended trust model, not a guarantee that the software has no
vulnerabilities.

## Intended use

Syntaxis is designed for one trusted operator using one trusted runtime account on a host they
administer. It is not a shared or multi-tenant service.

An authenticated user can intentionally reach arbitrary code execution through terminals, project
commands, Git hooks, tool installation, and optional agent tools. Therefore:

- do not share an instance with users who need different permissions;
- do not treat workspaces as operating-system boundaries;
- do not mount directories or secrets the runtime user should not access;
- assume a compromised Syntaxis login compromises mounted projects and the runtime home.

Enforce hard restrictions through the container, operating system, and network. UI controls and
agent instructions are not security boundaries.

## Authentication

Production requires an Argon2id hash in `SYNTAXIS_PASSWORD_HASH`. A successful login creates a
random, HTTP-only, 30-day cookie. Production cookies are `Secure` and `SameSite=Strict`. Sessions are
stored in memory and are invalidated by a server restart.

Use a long, unique password. Do not reuse the host's SSH password or another credential.

`SYNTAXIS_API_TOKEN` is a separate optional bearer token for non-browser clients. It must contain at
least 32 characters and should be stored in the client's platform keychain. Changing it and
restarting Syntaxis revokes the old value.

Syntaxis has no user database, roles, per-workspace permissions, or audit log. An identity-aware
proxy may add a login step, but every admitted identity still receives the same runtime authority.

## Network exposure

- Serve Syntaxis through HTTPS.
- Keep port 8080 on loopback or a private proxy network.
- Restrict the host firewall to required services.
- Preserve WebSocket support in the reverse proxy.
- Do not place authenticated pages behind a public cache.
- Never expose the development Compose service to the internet.
- Keep the host, Docker, and reverse proxy patched.

A VPN can reduce public exposure but does not replace application authentication or host security.

## Container boundary

The supplied image runs as a non-root user, but it is not a sandbox for hostile code. It includes
compilers, package managers, Git, SSH tools, Pi, and Mise because projects need them.

The default production deployment grants the container:

- read/write access to the projects directory and persistent home;
- read-only access to the selected host SSH directory;
- read/write access to the selected host GnuPG directory;
- the container's available network access.

Review and narrow these mounts. Separate credentials in the persistent container home reduce host
credential exposure. Resource and filesystem limits are useful when they do not prevent the intended
workflow.

Do not mount the Docker socket. It normally grants effective control of the Docker host.

## Projects and installed tools

Opening a repository does not make it trustworthy. The following may execute project or third-party
code:

- package installation and build scripts;
- Mise tools and tasks;
- Git hooks and signing helpers;
- language servers;
- scaffold commands and development servers;
- terminal commands;
- optional agent tools, skills, and extensions.

Bootstrap can infer and install tools through Mise. Existing Mise configuration remains
authoritative, but trusting it still permits its tools and tasks to run. Review unknown repositories
and commands; use a disposable or stronger sandbox for untrusted code.

## Optional Pi integration

Pi runs as the runtime user and can access the same workspace tools and credentials. Syntaxis can
install Pi packages, extensions, and skills, which may contain executable code or instructions.
Syntaxis applies checks to supported catalog operations but does not audit or sandbox third-party
content.

Check the source and contents before installation. Agent instructions guide behavior but cannot
prevent a malicious extension, compromised dependency, software bug, or model mistake from
performing an otherwise permitted operation.

## Preview URLs

Private and shared preview URLs contain bearer tokens in their hostnames. Anyone with the complete
URL can use it while it remains valid. URLs can leak through messages, screenshots, logs, browser
history, analytics, and referrer headers. Revoke shared links when they are no longer needed.

The main Syntaxis cookie is host-only and is not sent to preview applications. Shared preview access
does not grant workspace access.

Target validation reduces mistakes; it is not an authorization boundary against an operator who
already has terminal and network access from the runtime.

## Destructive operations and backups

Syntaxis can discard Git changes, delete files, remove a project directory, clear caches, remove
tools, and delete Pi sessions. Confirmation dialogs are not recovery mechanisms.

Keep independent backups of projects and persistent state, and test restoration.

## Secrets and reports

Never include these in public issues, screenshots, or logs:

- `.env` contents;
- password hashes or API tokens;
- private or shared preview URLs;
- SSH or GnuPG material;
- provider API keys;
- Pi session files;
- confidential source code or terminal output.

Rendered tool output can still contain secrets produced by the command itself.

Report vulnerabilities privately through the repository owner's security-reporting channel when
available. Include the affected version, deployment shape, impact, and the smallest safe proof of
concept.
