const ROOT = "/workspace";
const MAX_FILE_BYTES = 8 * 1024 * 1024;
const MAX_WORKSPACE_BYTES = 32 * 1024 * 1024;
const MAX_ENTRIES = 10_000;
const TOOL_CONFIG = `${ROOT}/.syntaxis/wasmer.json`;
const PINNED_PACKAGE = /^[a-zA-Z0-9_-]+\/[a-zA-Z0-9_.-]+@=\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$/;
// Leave headroom for both streams and diagnostics within the shell's 512 KiB budget.
const MAX_OUTPUT_BYTES = 128 * 1024;
const PACKAGES = Object.freeze({
  python: "python/python@=3.13.20",
  node: "wasmer/edgejs@=0.2.4",
  rg: "wasmer/ripgrep@=15.2.1",
  bash: "wasmer/bash@=1.0.25",
});
const HELP = `Wasmer browser tools (guest networking disabled)
  wasmer tools                       List built-in and workspace tools
  wasmer prepare python node         Download packages without executing guests
  wasmer run python -- script.py
  wasmer run python -- -c 'print("Hello")'
  wasmer run node -- script.js
  wasmer run rg -- pattern .
  wasmer run bash -- -c 'echo hello'
  wasmer run ./tool.wasm -- arguments
  wasmer run namespace/package@=version [--command name] -- arguments

Packages download on first use and are cached by this browser origin.
Only /workspace changes persist. Guest commands have a 10-second timeout.
Use wasmer status to check browser support without loading the runtime.
Workspace tools: .syntaxis/wasmer.json (exact package versions, optional command/args).
`;
let sdkPromise;

function supported() {
  return globalThis.crossOriginIsolated === true && typeof SharedArrayBuffer !== "undefined";
}

async function getClient() {
  if (!supported()) {
    throw new Error(
      "Wasmer needs cross-origin isolation (COOP: same-origin, COEP: require-corp). Use just serve-browser or configure your static host; ordinary shell commands remain available.",
    );
  }
  if (!globalThis.SyntaxisWasmerAssets) throw new Error("Wasmer assets are unavailable.");
  sdkPromise ??= (async () => {
    const base = new URL(`${globalThis.SyntaxisWasmerAssets.replace(/\/$/, "")}/`, location.href);
    return import(new URL("dist/index.js", base).href);
  })().catch((error) => {
    sdkPromise = undefined;
    throw error;
  });
  const { Wasmer } = await sdkPromise;
  // One worker pool per invocation: a failing guest cannot poison subsequent runs.
  const client = new Wasmer({
    parallelism: 2,
    outputBytes: MAX_OUTPUT_BYTES,
    cache: { namespace: "syntaxis-wasmer-v1" },
  });
  try {
    await client.ready();
    return client;
  } catch (error) {
    try {
      await client.close();
    } catch {
      /* Preserve the initialization error. */
    }
    throw error;
  }
}

function insideWorkspace(path) {
  return path === ROOT || path.startsWith(`${ROOT}/`);
}

export async function readWorkspaceTools(fs) {
  const tools = Object.create(null);
  if (!(await fs.exists(TOOL_CONFIG))) return tools;
  const canonical = await fs.realpath(TOOL_CONFIG);
  if (!insideWorkspace(canonical))
    throw new Error("Wasmer tool configuration must stay inside /workspace.");
  const stat = await fs.stat(canonical);
  if (!stat.isFile || stat.size > 32 * 1024)
    throw new Error("Wasmer tool configuration must be a file no larger than 32 KiB.");
  const bytes = await fs.readFileBuffer(canonical);
  if (bytes.length > 32 * 1024) throw new Error("Wasmer tool configuration exceeds 32 KiB.");
  let config;
  try {
    config = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new Error("Invalid JSON in .syntaxis/wasmer.json.");
  }
  const object = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
  if (
    !object(config) ||
    Object.keys(config).some((key) => key !== "tools") ||
    !object(config.tools)
  )
    throw new Error("Wasmer configuration requires a tools object.");
  const entries = Object.entries(config.tools);
  if (entries.length > 32) throw new Error("Wasmer configuration allows at most 32 tools.");
  for (const [name, spec] of entries) {
    if (!/^[a-z][a-z0-9_-]{0,63}$/.test(name) || Object.hasOwn(PACKAGES, name))
      throw new Error(`Invalid or reserved workspace tool name: ${name}`);
    if (
      !object(spec) ||
      Object.keys(spec).some((key) => !["package", "command", "args"].includes(key)) ||
      typeof spec.package !== "string" ||
      !PINNED_PACKAGE.test(spec.package)
    )
      throw new Error(`Tool ${name} requires an exact-version package and supported fields only.`);
    if (
      spec.command !== undefined &&
      (typeof spec.command !== "string" ||
        !spec.command ||
        spec.command.length > 128 ||
        /[\0\r\n]/.test(spec.command))
    )
      throw new Error(`Invalid command for workspace tool ${name}.`);
    if (
      spec.args !== undefined &&
      (!Array.isArray(spec.args) ||
        spec.args.length > 64 ||
        spec.args.some((arg) => typeof arg !== "string" || arg.length > 4096 || arg.includes("\0")))
    )
      throw new Error(`Invalid arguments for workspace tool ${name}.`);
    tools[name] = { source: spec.package, command: spec.command, args: spec.args ?? [] };
  }
  return tools;
}

function toolList(tools) {
  const builtins = Object.entries(PACKAGES).map(
    ([name, source]) => `${name}: ${source} (built-in)`,
  );
  const workspace = Object.entries(tools)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(
      ([name, spec]) =>
        `${name}: ${spec.source}${spec.command ? ` --command ${spec.command}` : ""}${spec.args.length ? ` args=${JSON.stringify(spec.args)}` : ""} (workspace)`,
    );
  return [...builtins, ...workspace].join("\n") + "\n";
}

// Complete and bound each snapshot before changing the destination filesystem.
export async function readShellWorkspace(fs) {
  const files = Object.create(null);
  const directories = [];
  const pending = [ROOT];
  let total = 0;
  let count = 0;
  while (pending.length) {
    const directory = pending.pop();
    for (const name of await fs.readdir(directory)) {
      if (++count > MAX_ENTRIES) throw new Error("Wasmer workspace exceeds 10,000 entries.");
      const path = `${directory}/${name}`;
      const stat = await fs.lstat(path);
      if (stat.isDirectory) {
        directories.push(path);
        pending.push(path);
      } else if (stat.isFile) {
        if (stat.size > MAX_FILE_BYTES) throw new Error(`File exceeds 8 MiB: ${path}`);
        const bytes = await fs.readFileBuffer(path);
        total += bytes.length;
        if (bytes.length > MAX_FILE_BYTES || total > MAX_WORKSPACE_BYTES)
          throw new Error("Workspace exceeds Wasmer snapshot limits.");
        files[path] = bytes;
      }
    }
  }
  return { files, directories };
}

export async function readGuestWorkspace(fs) {
  const files = Object.create(null);
  const directories = [];
  const pending = [ROOT];
  let total = 0;
  let count = 0;
  while (pending.length) {
    const directory = pending.pop();
    for (const entry of await fs.readDir(directory)) {
      if (++count > MAX_ENTRIES)
        throw new Error("Guest workspace exceeds 10,000 entries; changes were discarded.");
      if (!entry.name || /[/\\\0]/.test(entry.name) || entry.name === "." || entry.name === "..")
        throw new Error("Invalid guest workspace path.");
      const path = `${directory}/${entry.name}`;
      if (entry.kind === "directory") {
        directories.push(path);
        pending.push(path);
      } else if (entry.kind === "file") {
        total += entry.size;
        if (entry.size > MAX_FILE_BYTES || total > MAX_WORKSPACE_BYTES)
          throw new Error("Guest workspace exceeds snapshot limits; changes were discarded.");
        const bytes = await fs.readFile(path);
        if (bytes.length !== entry.size)
          throw new Error("Guest file changed while reading; changes were discarded.");
        files[path] = bytes;
      } else {
        throw new Error("Unsupported guest filesystem entry; changes were discarded.");
      }
    }
  }
  return { files, directories };
}

export async function reconcileGuestWorkspace(fs, before, after) {
  for (const path of Object.keys(before.files)) {
    if (!(path in after.files)) await fs.rm(path, { force: true });
  }
  const wanted = new Set(after.directories);
  const existing = new Set(before.directories);
  for (const path of [...before.directories].sort((a, b) => b.length - a.length)) {
    if (!wanted.has(path)) await fs.rm(path, { recursive: true, force: true });
  }
  for (const path of [...after.directories].sort((a, b) => a.length - b.length)) {
    // A guest may replace a file with a directory.
    if (path in before.files) await fs.rm(path, { force: true });
    if (!existing.has(path)) await fs.mkdir(path, { recursive: true });
  }
  for (const [path, bytes] of Object.entries(after.files)) {
    const original = before.files[path];
    if (original?.length === bytes.length && bytes.every((byte, index) => byte === original[index]))
      continue;
    await fs.writeFile(path, bytes);
  }
}

function byteString(bytes) {
  let result = "";
  for (let i = 0; i < bytes.length; i += 8192)
    result += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return result;
}

export function parseRun(args, ctx, tools = Object.create(null)) {
  if (args[0] !== "run" || !args[1])
    throw new Error("Use wasmer run <tool> -- <arguments>. See wasmer help.");
  const tool = args[1];
  let source = Object.hasOwn(PACKAGES, tool) ? PACKAGES[tool] : undefined;
  const spec = Object.hasOwn(tools, tool) ? tools[tool] : undefined;
  source ??= spec?.source;
  let localPath;
  if (!source && tool.endsWith(".wasm") && !/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(tool)) {
    localPath = ctx.fs.resolvePath(ctx.cwd, tool);
    if (!insideWorkspace(localPath))
      throw new Error("Local WASI modules must be inside /workspace.");
  } else if (!source && PINNED_PACKAGE.test(tool)) {
    source = tool;
  } else if (!source) {
    throw new Error(
      "Choose a tool from wasmer tools, a local .wasm file, or namespace/package@=version.",
    );
  }
  let offset = 2;
  let command = spec?.command;
  if (args[offset] === "--command") {
    command = args[offset + 1];
    if (!command || command.startsWith("--"))
      throw new Error("--command requires a package command name.");
    offset += 2;
  }
  if (args[offset] === "--") offset++;
  return { source, localPath, command, args: [...(spec?.args ?? []), ...args.slice(offset)] };
}

export function createWasmerCommand(loadClient = getClient, onSyncError = () => {}) {
  return async (args, ctx) => {
    if (!args.length || args[0] === "help" || args[0] === "--help")
      return { stdout: HELP, stderr: "", exitCode: 0 };
    if (args[0] === "status")
      return {
        stdout: supported()
          ? "Wasmer browser threads available; runtime loads on first run. Guest networking disabled.\n"
          : "Wasmer unavailable: cross-origin isolation is required. Ordinary shell commands work.\n",
        stderr: "",
        exitCode: supported() ? 0 : 1,
      };
    let sandbox;
    let client;
    let guestProcess;
    const kill = () => {
      void guestProcess?.kill().catch(() => {});
    };
    // Acquisition can involve a large first download; guest execution is bounded separately.
    const controller = new AbortController();
    const abort = () => {
      controller.abort();
      kill();
    };
    ctx.signal?.addEventListener("abort", abort, { once: true });
    const timer = setTimeout(abort, 120_000);
    try {
      ctx.signal?.throwIfAborted();
      const tools = await readWorkspaceTools(ctx.fs);
      if (args[0] === "tools") return { stdout: toolList(tools), stderr: "", exitCode: 0 };
      if (args[0] === "prepare") {
        if (args.length < 2 || args.length > 9)
          throw new Error("Use wasmer prepare <1–8 tools or pinned packages>.");
        const runs = args.slice(1).map((tool) => parseRun(["run", tool], ctx, tools));
        if (runs.some((run) => run.localPath))
          throw new Error("Local .wasm files need no package download; use wasmer run.");
        client = await loadClient();
        controller.signal.throwIfAborted();
        let progress;
        await client.packages.loadMany([...new Set(runs.map((run) => run.source))], {
          signal: controller.signal,
          onProgress: (value) => {
            progress = value;
          },
        });
        controller.signal.throwIfAborted();
        const download = progress?.download.downloadedBytes;
        return {
          stdout:
            runs.map((run, i) => `Prepared ${args[i + 1]} (${run.source})\n`).join("") +
            (download === undefined
              ? "Package download size unavailable.\n"
              : `Package download: ${download} bytes${download === 0 ? " (cache hit)" : ""}; SDK assets excluded.\n`),
          stderr: "",
          exitCode: 0,
        };
      }
      const run = parseRun(args, ctx, tools);
      const before = await readShellWorkspace(ctx.fs);
      if (!insideWorkspace(ctx.cwd))
        throw new Error("Wasmer commands must start inside /workspace.");
      client = await loadClient();
      controller.signal.throwIfAborted();
      let source = run.source;
      if (run.localPath) {
        const canonical = await ctx.fs.realpath(run.localPath);
        if (!insideWorkspace(canonical))
          throw new Error("Local WASI modules must be inside /workspace.");
        source = await ctx.fs.readFileBuffer(canonical);
        const module = await WebAssembly.compile(source);
        const exports = WebAssembly.Module.exports(module);
        if (
          !exports.some((entry) => entry.name === "_start" && entry.kind === "function") ||
          !exports.some((entry) => entry.kind === "memory")
        )
          throw new Error("Local WASI modules must export _start and memory.");
      }
      const pkg = await client.packages.load(source, { signal: controller.signal });
      sandbox = await client.sandboxes.create({
        packages: [pkg],
        files: before.files,
        network: { mode: "disabled" },
        env: ctx.exportedEnv ?? {},
        signal: controller.signal,
      });
      await sandbox.fs.mkdir(ROOT, { recursive: true });
      for (const directory of before.directories)
        await sandbox.fs.mkdir(directory, { recursive: true });
      controller.signal.throwIfAborted();
      guestProcess = await sandbox.command(run.command ?? pkg, run.args, { cwd: ctx.cwd }).spawn({
        stdin: "pipe",
        stdout: "capture",
        stderr: "capture",
        timeoutMs: 10_000,
        outputBytes: MAX_OUTPUT_BYTES,
      });
      controller.signal.throwIfAborted();
      // just-bash pipes carry Latin-1 byte strings, not Unicode text.
      const stdin = Uint8Array.from(ctx.stdin, (character) => character.charCodeAt(0));
      if (stdin.length) await guestProcess.stdin.write(stdin);
      await guestProcess.stdin.close();
      const output = await guestProcess.wait();
      controller.signal.throwIfAborted();
      if (output.reason === "exited") {
        const after = await readGuestWorkspace(sandbox.fs);
        controller.signal.throwIfAborted();
        try {
          await reconcileGuestWorkspace(ctx.fs, before, after);
        } catch (error) {
          onSyncError("Workspace reconciliation failed");
          throw error;
        }
      }
      const truncated = output.stdout.truncated || output.stderr.truncated;
      return {
        stdout: byteString(output.stdout.bytes),
        stdoutKind: "bytes",
        stderr:
          output.stderr.text() +
          (truncated ? "\n[Wasmer output truncated]\n" : "") +
          (output.reason === "timeout"
            ? "\nWasmer timed out; guest file changes discarded.\n"
            : ""),
        exitCode: output.reason === "timeout" ? 124 : output.exitCode,
      };
    } catch (error) {
      kill();
      return {
        stdout: "",
        stderr: `wasmer: ${controller.signal.aborted ? "Command cancelled or package acquisition timed out." : (error?.message ?? String(error))}\n`,
        exitCode: controller.signal.aborted ? 130 : 1,
      };
    } finally {
      clearTimeout(timer);
      ctx.signal?.removeEventListener("abort", abort);
      try {
        await sandbox?.close();
      } catch (error) {
        onSyncError(`Wasmer sandbox cleanup failed: ${error?.message ?? String(error)}`);
      } finally {
        try {
          await client?.close();
        } catch (error) {
          onSyncError(`Wasmer runtime cleanup failed: ${error?.message ?? String(error)}`);
        }
      }
    }
  };
}
