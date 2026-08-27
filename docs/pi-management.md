# Pi integration

Syntaxis provides an optional graphical workspace for the
[Pi coding agent](https://pi.dev/). It runs Pi directly through `pi --mode rpc`; it does not
reimplement the agent, proxy model requests, or store provider keys in the browser.

## Setup

Pi is included in the supplied container. Authenticate it once:

```bash
docker exec -it syntaxis pi
```

Complete Pi's `/login` flow or configure a supported provider. Pi credentials, settings, resources,
and sessions remain in Pi's normal directories inside the persistent runtime home.

For a non-container deployment, install and authenticate Pi on the Syntaxis host:

```bash
curl -fsSL https://pi.dev/install.sh | sh
pi
```

Set `SYNTAXIS_PI_COMMAND` if the executable is not available as `pi` on the server's `PATH`.
Syntaxis resolves this executable once for each operation and passes its resolved Pi agent directory
to chat processes, so management actions and RPC sessions use the same credentials.

## Chats and persistence

Each active chat has its own Pi RPC process. Syntaxis can therefore:

- keep a chat working while you use another workspace section;
- show streaming text, reasoning, tool calls, and extension dialogs;
- preserve steering and follow-up queues;
- restore conversations from Pi's session files;
- notify the interface when a chat needs attention.

Working chats are not stopped merely to reduce idle use. Syntaxis keeps a limited number of settled
background chats warm; older settled processes stop and resume from their Pi session when selected.

A Syntaxis restart stops live processes, but saved sessions remain available. Work interrupted by a
container or host restart is not guaranteed to continue automatically.

Deleting a chat stops its process and permanently deletes the Pi session. There is no application
trash.

## Chat interface

The interface supports:

- creating, renaming, and resuming chats;
- message editing, steering, and follow-ups;
- separate steering and after-task queues with visible pending messages;
- `@` file-reference autocomplete for project files;
- one-click references for the active editor file, cursor, or selection;
- branching from any earlier user prompt, including compacted history;
- cloning the current conversation branch into a new chat;
- downloading a bounded HTML export of the current session;
- extension-provided dialogs, statuses, titles, and text widgets;
- image attachments;
- model selection and available usage information;
- manual context compaction from the usage menu;
- rendered tool output and structured tool details;
- queued extension messages and dialogs.

Commands requiring Pi's terminal-only interactive interface are rejected with an explanation. Run
them in Terminal instead.

Live output, RPC records, dialogs, and retained timeline items are bounded to protect the server and
browser. Pi's session file remains the complete record.

## Settings and instructions

Syntaxis keeps an **Essentials** view for common agent-relevant settings and an **Advanced JSON**
view for the complete global or project settings file. Essentials uses Pi's public settings manager
and enables each control only when the installed Pi exposes its setter; an unrelated Pi change no
longer disables the whole page. Advanced includes searchable documentation from the installed Pi,
validates JSON, detects concurrent changes, writes atomically, and keeps one rolling
`settings.json.syntaxis-backup` beside a file that it replaces.

Global settings apply to the runtime user's Pi installation. Project settings live in `.pi` and Pi
applies them according to project trust. Running chats keep the values they loaded until they reload
or restart. Advanced editing intentionally accepts strict JSON, matching Pi's settings parser.

Global instructions manage Pi's instance-wide `AGENTS.md`, normally `~/.pi/agent/AGENTS.md`. Saving
an empty document removes it. New sessions load the updated instructions; existing processes may
retain the previous version.

Instructions guide agent behavior but do not enforce permissions. Use container, operating-system,
or network controls for hard restrictions.

## Prompt templates

Syntaxis can create, edit, rename, and delete:

- global templates in `~/.pi/agent/prompts`;
- project templates in `.pi/prompts`.

The editor supports Pi's optional `description` and `argument-hint` frontmatter. Pi loads project
templates only after project trust.

## Skills

Skills can be managed in global and project scopes:

- `~/.pi/agent/skills`;
- `.pi/skills`.

Syntaxis preserves unknown frontmatter and sibling files when editing a skill. It can also search and
install from the public skills catalog. Downloads are size-limited, paths must remain relative, and
existing skills are not overwritten.

Public search needs no extra token. The optional leaderboard views require `VERCEL_OIDC_TOKEN` and
remain hidden when it is absent.

The catalog is an external convenience service. Local skill management continues to work if it is
unavailable. Skills may contain executable code and instructions; review them before installation.

## Packages and extensions

Syntaxis searches npm for the `pi-package` keyword and can install or remove user-scoped packages
through Pi's CLI. Project-scoped packages are recognized but are not replaced or removed.

Limits:

- npm search is not an official Pi catalog and may be incomplete;
- Git and local-only packages are not discoverable in the UI;
- filters apply to the result pages loaded so far;
- operations have a three-minute timeout;
- running chats do not automatically load newly installed resources;
- user-scoped packages are shared by every workspace using the runtime account.

Extensions execute code as the runtime user. Syntaxis does not audit or sandbox them.

## Updates and data

**Check for updates** runs Pi's self-update command and may install an update. Existing chats continue
with their current Pi process; new chats use the updated installation. After updating, Syntaxis checks
the executable, Pi's public `SettingsManager` and `ModelRuntime` exports, and configured provider
credentials before reporting success. If Pi removes one curated setter, only that Essentials control
becomes unavailable; Advanced JSON remains available.

Container deployments keep the image's pinned Pi installation as a recovery copy. The entrypoint
uses the persisted, self-updated Pi when it starts successfully and restores the image copy if that
installation can no longer launch. This is one fixed fallback, not an accumulating version archive.

Syntaxis does not keep a second authoritative copy of Pi data. Sessions, settings, prompts, skills,
and packages remain in Pi's directories under the runtime home.

Removing a Syntaxis workspace stops its live Pi processes but does not delete saved chats. Back up
the runtime home if they matter. See the
[security model](security.md#optional-pi-integration) for the trust implications of agents and
extensions.
