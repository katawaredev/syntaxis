# Wasmer browser execution

The standalone browser app embeds [Wasmer SDK](https://github.com/wasmerio/wasmer-sdk)
0.19.0 as an optional command executor. It adds language runtimes and packaged
WASI/WASIX tools to the existing command console and Pi's `bash` tool. The server
app continues to use its native terminal. Pi itself still uses the browser
libraries and direct provider requests.

## Try it

Open the browser terminal's run menu and choose **Wasmer · browser tools and
examples**, or enter `wasmer help`. `wasmer status` checks threading availability
without downloading the runtime or a package.

```sh
wasmer run python -- -c 'print(sum(range(100)))'
wasmer tools
wasmer prepare python node
wasmer run python -- script.py
wasmer run node -- script.js
wasmer run rg -- 'TODO|FIXME' .
wasmer run bash -- -c 'echo hello'
printf 'hello' | wasmer run python -- -c 'import sys; print(sys.stdin.read().upper())'
wasmer run ./tool.wasm -- arguments
wasmer run wasmer/coreutils@=1.0.27 --command wc -- -c README.md
```

The shortcuts pin Python `python/python@=3.13.20`, Edge.js
`wasmer/edgejs@=0.2.4`, ripgrep `wasmer/ripgrep@=15.2.1`, and Bash
`wasmer/bash@=1.0.25`. `node` selects Edge.js, a Node-compatible runtime;
compatibility with individual Node APIs and dependencies needs checking.
Other packages must use `namespace/package@=version`; an optional `--command`
selects an exported command. Versions do not silently follow registry updates.
Local `.wasm` files must stay inside the workspace and export `_start` and memory.
This runs WASI command modules, not arbitrary Wasm exports or components.

Shell quoting, variables, pipes, and redirections still use just-bash. Wasmer
receives the command's arguments, stdin bytes, exported shell environment, and
current directory. Both text and binary redirections work. Commands are explicit:
ordinary `rg`, `bash`, or shell built-ins retain their existing implementation.

## Repeatable project tools

Create `.syntaxis/wasmer.json` in the workspace to define shared tool aliases:

```json
{
  "tools": {
    "check": {
      "package": "python/python@=3.13.20",
      "args": ["-m", "unittest", "discover", "-s", "tests"]
    },
    "count": {
      "package": "wasmer/coreutils@=1.0.27",
      "command": "wc",
      "args": ["-c"]
    }
  }
}
```

Then run `wasmer tools`, `wasmer prepare check`, `wasmer run check`, or
`wasmer run count -- README.md`. Default arguments are passed as data before
the arguments supplied at invocation; they are never evaluated as shell text.
An explicit `--command` overrides the alias's command. Aliases cannot replace
the built-in Python/Node/Bash/ripgrep shortcuts; use a separate alias for a
different version.

Discovery reads configuration without loading Wasmer or contacting the registry.
Preparation acquires up to eight requested packages (deduplicating aliases to
the same pin), reports the SDK's package-download byte count/cache hit, and closes
the runtime without creating or executing a guest. SDK asset downloads are
excluded from that count. Preparing helps separate first-use downloads from
later execution, but browser eviction and package dependency compatibility still
apply. Pinning a top-level package does not lock its entire dependency graph.

Configuration is an ordinary workspace file, persisted and exported with it.
It permits at most 32 aliases in a 32 KiB JSON file, with only `package`,
`command`, and `args` fields per tool. Each package requires an exact version.
Malformed, oversized, unpinned, or unsupported configuration fails before SDK
initialization; it cannot enable network access or change resource limits.
Configuration contents are re-read for each command. `wasmer help` and
`wasmer status` remain available even if the configuration is invalid.

## Workspace and lifetime

Each invocation gets a separate Wasmer client, worker pool, and guest filesystem.
The current shell workspace is copied into `/workspace`, including empty
directories. Completed guest writes, deletions, and directory changes are copied
back to the shell snapshot; unchanged bytes and directories are left alone.
Only `/workspace` is reconciled; guest system files and installed packages do
not become workspace files.

The existing Rust adapter then validates paths, protects `.git` and generated
directories, checks that the real workspace has not changed since execution
started, and publishes editor change notifications. Wasmer never receives OPFS
or local-directory handles. Dirty editor buffers retain the existing conflict
behavior. Simultaneous browser command calls are rejected instead of sharing
mutable shell state.

The workspace limit remains 32 MiB total and 8 MiB per file. Wasmer snapshots
also allow at most 10,000 entries, bounding trees of empty files and directories.
Guest output is captured up to 128 KiB per stream, leaving room for both streams and diagnostics
within the shell's 512 KiB output budget. Guest execution times out after
10 seconds; the shell's overall execution budget is now 120 seconds to allow first-use
package acquisition. Acquisition also has a 120-second abort timer. Cancellation
kills the active guest and discards the whole command's workspace changes.
Guest timeout or an invalid/oversized returned snapshot discards that guest's
changes; preceding shell statements may still have succeeded. A normally exited
command can retain writes even with a nonzero exit code, matching shell behavior.
Reconciliation failures discard the whole command's workspace changes.
Sandbox/runtime cleanup failures also discard the command's file changes and
report the shutdown error; cleanup does not mask an initialization error.
Workers and sandboxes close after execution. This remains a command console;
interactive stdin, persistent processes, dev servers, and package installation
are not exposed.

Package data is cached in origin-scoped browser storage under
`syntaxis-wasmer-v1`, separate from workspace files and AI credentials. Cache
entries can be evicted or removed with site data. This is a download cache, not
durable guest state or an offline application guarantee. Reload still clears Pi
keys and chats. The SDK assets are approximately 5.2 MiB uncompressed, including
a 4.7 MiB runtime Wasm module; they load only on a Wasmer command. Language
packages add their own first-use downloads. There is no configured per-guest
memory quota; browser memory limits still matter for large tools.

## Hosting and networking

Wasmer threads need a secure context and cross-origin isolation:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

`just serve-browser` enables Dioxus's `--cross-origin-policy`; browser Vercel
headers and the static browser smoke server supply the same policy. Other static
hosts need equivalent headers and JavaScript/Wasm MIME types. SDK, worker,
glue, and Wasm files ship together as a Manganis folder asset, preserving their
relative URLs. The CSP permits same-origin workers and Wasm compilation.
Cross-origin resources must satisfy CORS/CORP; test the deployed site's external
resources and analytics under this policy. Missing isolation leaves ordinary
shell commands usable and gives an explicit Wasmer error.

Guest networking is always `disabled`. Registry/package downloads use normal
browser HTTPS fetch; Wasmer guest processes cannot access external TCP/DNS or
receive the app's provider keys. This integration needs no WISP proxy or
Syntaxis server. Existing Git CORS proxy settings are unrelated.

For future networked guest programs, browsers cannot provide raw external
TCP/DNS: Wasmer uses an explicitly configured WISP WebSocket endpoint for that.
Direct Pi HTTP requests continue to use browser CORS. Local TCP between Wasmer
sandboxes and guest HTTP previews can use its `http` mode without WISP.
Neither network mode is enabled by this integration.

## Where Wasmer fits next

These are opportunities based on the [upstream SDK examples](https://github.com/wasmerio/wasmer-sdk/blob/main/js/README.md),
not additional implemented Syntaxis capabilities.

| Area | Useful adoption or replacement | Additional work |
| --- | --- | --- |
| Scripting and checks | Python/Edge.js scripts, generators, language-specific checks, packaged formatters | Commands can use the current executor; verify each package and its dependencies |
| Search | Real ripgrep semantics for AI and terminal work; potentially replace the bounded file-search engine | Connect query/results to search ports and benchmark snapshot/download costs |
| Compilers and media | Local WASI tools, packaged clang, FFmpeg or image processing | Validate packages, binary output, resource budgets and download size; longer jobs need richer lifecycle controls |
| Rich shell | Real Bash/Unix packages as an optional alternative to emulated commands | Interactive terminal requires streaming, resize, persistent state, and cancellation ports |
| Web previews | Run framework or PHP guest servers and replace static preview for capable projects | Persistent process ownership, save synchronization, port routing, cleanup, and a separate preview origin with a service worker |
| Databases | Browser PostgreSQL for sample apps and SQL tools | Shared client and linked guest lifetimes, durable workspace storage, recovery and migration; upstream pglite example accepts one connection per process |
| Dependency management | npm/pnpm/pip inside guest environments | Explicit trusted WISP configuration, storage policy for protected dependency folders, download budgets and package compatibility |

Python/JavaScript project checks and packaged formatters are the smallest next
extensions. Live previews offer a larger product gain but require a process and
preview adapter. Wasmer's default HTTP preview origin is hosted by Wasmer;
Syntaxis should make any such origin explicit or host its own isolated static
preview origin. A second static origin does not require an application backend.
Keep the existing restrictive static preview available.

## Verification and maintenance

Build via `bun run build:browser-terminal`; it also stages the SDK assets.
Focused checks:

```sh
bun test autoresearch/wasmer-command.test.js
bun run autoresearch:wasmer-smoke
```

The smoke script serves generated assets with the production CSP and isolation
headers and uses Chromium. It covers project alias discovery without network
requests, preparation/cache reuse across reload without workspace mutations,
a local WASI module, Python filesystem changes, empty directories,
Node-compatible execution, Bash, ripgrep, Unicode
and binary pipes, error recovery, nonzero exits, file limits, cancellation,
guest timeout, and missing-isolation fallback. It checks that no WebSocket is
opened. These tests exercise the real SDK; they do not prove Rust/OPFS editor
reconciliation, deployment headers, live providers, Firefox or Safari acceptance.
Broader app and Rust/build QA follows the repository's explicit confirmation rule.

Retest these checks when updating the SDK or package pins. Preserve the SDK's
modified MIT license in distributed assets; its extra condition requires visible
Wasmer attribution above its stated commercial usage/revenue thresholds.
The generated SDK folder also retains its bundled third-party license notices.
