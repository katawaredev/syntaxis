# Browser AI

This document describes `apps/browser`. The Android shell's Local mode runs the
full host Pi RPC integration inside Termux; Remote uses its selected server's Pi.
The Android app requires its paired Termux runtime even when opening a remote
project. Its merged project list does not merge Pi credentials or sessions.
Neither Android mode uses the browser sandbox or its in-memory credential store.
See [Android setup](../apps/Android/README.md) for local persistence and limitations.

The standalone browser runtime uses `@earendil-works/pi-ai` and
`@earendil-works/pi-agent-core`, pinned to the same release as the server's Pi
coding agent. The shared Dioxus UI provides model selection, reasoning controls,
favourites, provider accounts, streaming messages, tool activity, and token usage.

## Setup

1. Open **Settings → Provider accounts** and add your own API key for a provider.
2. Open the default-model selector under **General**, select and save a model,
   then create a chat. No message is required to load the model list.
3. Change models or reasoning effort within a chat using its model picker.

Supported built-in providers are OpenAI, Anthropic, Google, Mistral, Groq,
OpenRouter, and xAI. Only providers with saved browser keys appear. OpenRouter's
authenticated `/api/v1/models/user` endpoint filters the bundled Pi catalog to
account-allowed, tool-capable models in the background when a model selector opens.
Selectors initially show the last successful list, or the configured providers'
bundled catalog before the first successful discovery. Discovery errors retain
that list and show an inline Retry action. Saving keys does not start discovery;
credential changes invalidate the cached account filter on its next use.
Chat creation, model selection, and sending do not wait for provider discovery.
Concurrent discovery requests share one in-flight lookup for the same OpenRouter key.
The chat picker automatically refreshes only on its first opening in that chat
view; failures can be retried explicitly. Successful browser lookups are reused
for one minute across selectors and chats, preventing repeated Settings openings
or rapid chat creation from repeating provider requests. Changing the OpenRouter
key invalidates that cooldown. Re-entering a chat view may trigger another refresh,
still subject to the browser cooldown.
Other providers use Pi's bundled catalog; listing does not guarantee account access
or that the provider permits requests from your browser origin. This catalog is
independent of the server app. Newly released models require a Pi catalog update.
If the saved default is unavailable, new chats select the first available model.
API keys containing internal whitespace or non-ASCII characters are rejected
before constructing HTTP headers; keys are never included in error messages.

For a custom OpenAI-compatible service, save its API base URL under Provider
accounts and add its key under **Custom OpenAI-compatible**. In General, choose
the custom-model option and enter its model ID. Existing URLs ending in
`/chat/completions` are accepted and normalized by the Pi bridge. Custom model
capabilities and prices are not discovered automatically.

API keys, conversations, model preferences, and defaults are held in memory for
the current app instance. Reloading or closing the tab discards them. Credentials
are never placed in localStorage, IndexedDB, exported chats, or chat histories.
Draft text uses the existing local draft storage. API keys are sent to their
provider; the custom-provider key is sent to the configured custom endpoint.

Pi OAuth login, subscription credentials, and Bedrock require the server runtime.
No browser OAuth workarounds or authentication proxies are included.

## Workspace tools

The browser agent has `read`, `list`, `write`, `edit`, and `bash` tools. File tools
use the same workspace filesystem and change notifications as the editor. Edits
require a unique text match and check the file version before writing. File
content is limited to 256 KiB per tool call; directory listings stop at 1,000
entries.

File tools accept both `app.js` / `./app.js` and `/workspace/app.js`, matching
the terminal's working directory. Absolute paths outside `/workspace` and parent
traversal are rejected. Reading a nested file also loads its ancestor directories'
`AGENTS.md` files and returns them with the file. Failed tool calls are displayed
as failed; shell and file output preserve line breaks.

## Workspace instructions, skills, and templates

Browser AI resources are ordinary workspace files, persisted with the workspace
and included in its normal exports. Unlike API keys, they survive app reloads.
The instructions editor edits root `AGENTS.md`; there is no browser-wide global
resource directory. Root `AGENTS.md` and `.pi/APPEND_SYSTEM.md` are re-read before
every message, including continuing existing chats. Missing files are optional;
unreadable or oversized instruction files produce an error rather than silently
omitting instructions. Guidance does not enforce security boundaries.

- **Prompt templates:** `.pi/prompts/<name>.md`, editable in Settings. Invoke with
  `/<name> arguments`. `$ARGUMENTS` and `$@` insert all arguments; `$1`, `$2`, etc.
  insert whitespace-separated arguments. Frontmatter supports `description` and
  `argument-hint`. Templates expand once, not recursively. Existing template names
  are fixed in the browser editor; create a copy to use another name.
- **Skills:** `.pi/skills/<name>/SKILL.md` or `.pi/skills/<name>.md`. Frontmatter
  supports `name` and `description` as plain or quoted single-line values, matching
  the server editor's basic metadata format. The agent receives the skill catalog
  with every message and can read matching skills and their relative references.
  `/skill:<name> task` includes the skill instructions directly in that turn.
  This does not execute native scripts or load Pi extensions.
- Slash-command suggestions are loaded when a chat is created or reopened.
  Resource contents are re-read when invoked, so edits do not require a new chat.
- Deleting a directory-backed skill removes only `SKILL.md`, preserving its
  supporting files. Remote skill discovery/installation and global scope remain
  server-only and are hidden in the browser settings.
  Browser settings link to skills.sh for discovery in a separate tab; its search
  API does not currently return the CORS header needed for direct in-app browser
  search. Copy reviewed Markdown into the local skill editor instead.

Each resource is limited to 128 KiB; resource directories allow 100 entries.
Combined automatically loaded instructions and expanded prompts each have a
128 KiB limit. Large resources fail explicitly rather than being silently truncated.

`bash` uses the existing just-bash browser sandbox. It does not provide native
processes, arbitrary package installation, or a server filesystem. The agent is
told these limits. It stops after 20 turns per prompt, and response/event limits
also apply. Pi's tool transcript is retained when continuing or branching chats.

The full coding-agent CLI, server extensions, session compaction, and background
execution are not emulated. Leaving the chat cancels its browser agent run.

## Development

`bun run build:browser-ai` builds the lazily loaded browser bundle. The regular
`build-assets` task includes it. The bundle uses explicit provider factories and
does not install Node shims or inject credentials at build time.

Regression coverage lives in
`crates/runtime-browser/bridge-src/ai/bridge-source.test.js` and the existing
`autoresearch/browser-smoke.mjs` flow. Provider requests in these tests are mocked;
no real credentials or paid model calls are needed. Follow AGENTS.md's approval
requirement before running the validation workflow.
