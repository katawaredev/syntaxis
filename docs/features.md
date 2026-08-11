# Features and limitations

Syntaxis is a single-user workspace for projects on a trusted server. This page describes its
modules and their main limits.

## Projects

The home screen can:

- register an existing folder below the projects root;
- clone a Git repository over HTTPS or SSH;
- scaffold a project in an interactive terminal;
- remember recent projects and their last active section;
- store project notes;
- bootstrap or update Mise-managed tools;
- clean ignored files and runtime caches;
- unregister a project, with a separate option to delete its files.

A workspace is a registered folder, not an isolated VM or container. All workspaces share the
runtime user, home directory, credentials, tools, and machine resources.

## Files and editor

The Files module provides:

- create, rename, move, and delete operations;
- project and file-name search;
- multiple open files;
- find and replace with case, whole-word, and regular-expression modes;
- go to line;
- syntax highlighting and image viewing;
- Git change markers and file diffs;
- unsaved-change prompts;
- optional diagnostics and semantic completions.

The editor supports focused work from a small screen. It does not provide a general extension
marketplace, integrated debugging, notebooks, a test explorer, or the full refactoring surface of a
mature desktop IDE.

### Code intelligence

Code intelligence starts only while enabled for a supported document. Syntaxis runs at most one
primary language server and one project-specific supplementary server for the active file.

Supported project types include Rust, JavaScript, TypeScript, Deno, Python, Go, HTML, CSS/SCSS, JSON,
YAML, TOML, shell, Terraform, PHP, Ruby, Vue, Svelte, and Astro. Tailwind can run as a supplementary
server when detected.

Language servers are resolved through Mise. Compatible Node-based servers in the project root's
`node_modules/.bin` may be used before a Mise-managed server. Yarn Plug'n'Play and nested local
installations currently fall back to Mise.

### Bootstrap

For a project without Mise configuration, **Bootstrap** can infer a toolchain and language servers.
Accepted tools are written to the checkout-local `mise.local.toml`.

Existing Mise configuration remains authoritative: Syntaxis can trust and install its declared tools
but does not rewrite it. Tool installation executes third-party software; review the proposal first.

## Terminal

Terminal sessions are real shells on the Syntaxis runtime. The module supports:

- multiple named sessions per workspace;
- reconnection after leaving the page;
- scrollback and resizing;
- touch scrolling and a mobile key row;
- links from recognized source locations to the editor;
- saved and scaffold commands.

Commands run with the runtime user's permissions. Use a process supervisor when an application must
survive a Syntaxis container or host restart.

## Git

The Git module supports:

- repository initialization and status;
- staged and unstaged diffs;
- stage, unstage, discard, and discard-all actions;
- commits and signing retries;
- branches, tags, remotes, history, and worktrees;
- fetch, pull, push, and guarded force-push flows;
- comparison and common merge operations.

Git uses the runtime user's environment. The supplied Compose deployment mounts host SSH
configuration read-only and GnuPG configuration read/write. Pull requests, protected branches, and
server-side policy remain the responsibility of the Git provider.

## Preview

Preview can connect to either a runtime port or an HTTP(S) origin reachable from the runtime. On
Linux, Syntaxis suggests listening HTTP processes whose working directory is inside the current
workspace.

The gateway proxies HTTP and WebSocket traffic. A preview can open inside Syntaxis or in a separate
window.

Previews are private by default. One separate share URL can be created and revoked without stopping
the private preview. Preview URLs are bearer credentials: anyone with the complete URL can use it
while it remains active.

Public previews require wildcard DNS and TLS. See
[Application previews](deployment.md#application-previews).

## Optional Pi integration

The AI module is a client for an installed [Pi coding agent](https://pi.dev/). Each active chat uses a
real `pi --mode rpc` process, and saved chats remain Pi sessions.

The interface supports multiple chats, streaming tool activity, attachments, message editing,
steering, follow-ups, and selected Pi resource management. Working chats continue when you navigate
away; settled sessions may be stopped and resumed to limit idle resource use.

Syntaxis does not provide model access. Pi authentication and use are optional. See
[Pi integration](pi-management.md).

## Storage and cleanup

- **Cleanup files** removes selected Git-ignored entries from one project while excluding common
  local secrets such as `.env`, `.envrc`, `.direnv`, and `*.local`.
- **Prune unused tools** removes inactive Mise tool versions.
- **Remove all tools** removes all Mise-managed tools and their download cache.
- **Clear runtime caches** removes broad language and build caches.
- **Remove workspace** unregisters a project; deleting its directory is a separate choice.
- **Delete chat** permanently removes its Pi session.

Destructive actions have no application trash or recovery area.

## Main limitations

- Syntaxis is single-user and has no roles, teams, per-project permissions, or audit logs.
- Workspaces share one trusted runtime and are not isolated.
- Syntaxis runs on or beside the development machine; it is not a general SSH client.
- The documented production client is the web application.
- Offline editing is not supported.
- Public previews require wildcard DNS and TLS.
- Project scripts, tool bootstrap, terminal commands, Pi skills, and extensions can execute code as
  the runtime user.

## When not to use Syntaxis

Use a full IDE when deep debugging, extensions, notebooks, or large-scale manual editing matter more
than phone ergonomics.

Use an SSH/SFTP client when you mainly administer many unrelated hosts. Use a hosted development
environment when you do not want to operate a server. Use a multi-user workspace platform when users
must be isolated or centrally governed.
