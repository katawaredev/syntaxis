import { expect, test } from "bun:test";
import { InMemoryFs } from "just-bash/browser";
import {
  createWasmerCommand,
  parseRun,
  readWorkspaceTools,
  readGuestWorkspace,
  readShellWorkspace,
  reconcileGuestWorkspace,
} from "../crates/runtime-browser/bridge-src/terminal/wasmer-command.js";

test("Wasmer packages require exact versions and local modules stay in the workspace", () => {
  const ctx = { fs: new InMemoryFs(), cwd: "/workspace/src" };
  expect(parseRun(["run", "../tool.wasm", "--", "value"], ctx).localPath).toBe(
    "/workspace/tool.wasm",
  );
  expect(parseRun(["run", "python"], ctx).source).toBe("python/python@=3.13.20");
  expect(parseRun(["run", "node"], ctx).source).toBe("wasmer/edgejs@=0.2.4");
  expect(
    parseRun(["run", "wasmer/coreutils@=1.0.27", "--command", "wc", "--", "-c"], ctx),
  ).toMatchObject({ command: "wc", args: ["-c"] });
  for (const source of [
    "/tmp/guest.wasm",
    "../../../guest.wasm",
    "python/python",
    "python/python@latest",
    "https://example.com/guest.wasm",
    "constructor",
    "toString",
  ]) {
    expect(() => parseRun(["run", source], ctx)).toThrow();
  }
});

test("workspace tools use pinned packages and keep default arguments as data", async () => {
  const fs = new InMemoryFs({
    "/workspace/.syntaxis/wasmer.json": JSON.stringify({
      tools: {
        check: {
          package: "python/python@=3.13.20",
          command: "python",
          args: ["-c", "print('a; b')"],
        },
      },
    }),
  });
  const tools = await readWorkspaceTools(fs);
  expect(parseRun(["run", "check", "--", "more"], { fs, cwd: "/workspace" }, tools)).toMatchObject({
    source: "python/python@=3.13.20",
    command: "python",
    args: ["-c", "print('a; b')", "more"],
  });
  expect(
    parseRun(
      ["run", "check", "--command", "other", "--", "value"],
      { fs, cwd: "/workspace" },
      tools,
    ).command,
  ).toBe("other");
  let initialized = false;
  const result = await createWasmerCommand(async () => {
    initialized = true;
  })(["tools"], { fs });
  expect(initialized).toBe(false);
  expect(result.stdout).toContain("check: python/python@=3.13.20 --command python");
});

test("invalid project tool configuration fails before initializing or downloading", async () => {
  for (const config of [
    "{",
    JSON.stringify({ tools: { python: { package: "python/python@=3.13.20" } } }),
    JSON.stringify({ tools: { check: { package: "python/python@latest" } } }),
    JSON.stringify({ tools: { check: { package: "python/python@=3.13.20", network: "wisp" } } }),
    JSON.stringify({ tools: { check: { package: "python/python@=3.13.20", args: [42] } } }),
  ]) {
    const fs = new InMemoryFs({ "/workspace/.syntaxis/wasmer.json": config });
    let initialized = false;
    const result = await createWasmerCommand(async () => {
      initialized = true;
    })(["prepare", "python"], { fs });
    expect(result.exitCode).toBe(1);
    expect(initialized).toBe(false);
  }
  const fs = new InMemoryFs({ "/workspace/.syntaxis/wasmer.json": "x".repeat(32769) });
  await expect(readWorkspaceTools(fs)).rejects.toThrow("32 KiB");
});

test("preparation deduplicates downloads, reports cached data, and never runs a guest", async () => {
  const fs = new InMemoryFs({
    "/workspace/.syntaxis/wasmer.json": JSON.stringify({
      tools: { pyproject: { package: "python/python@=3.13.20" } },
    }),
  });
  let closed = false;
  let sources;
  const command = createWasmerCommand(async () => ({
    packages: {
      loadMany: async (values, options) => {
        sources = values;
        options.onProgress({ download: { downloadedBytes: 0 } });
      },
    },
    close: async () => {
      closed = true;
    },
  }));
  const result = await command(["prepare", "python", "pyproject"], { fs, cwd: "/workspace" });
  expect(sources).toEqual(["python/python@=3.13.20"]);
  expect(result.stdout).toContain("0 bytes (cache hit)");
  expect(result.stdout).toContain("Prepared pyproject");
  expect(result.exitCode).toBe(0);
  expect(closed).toBe(true);
});

test("preparation cancellation releases the client and cleanup errors remain visible", async () => {
  const fs = new InMemoryFs();
  const controller = new AbortController();
  let closed = false;
  let failure;
  const command = createWasmerCommand(
    async () => ({
      packages: {
        loadMany: async (_, { signal }) => {
          controller.abort();
          signal.throwIfAborted();
        },
      },
      close: async () => {
        closed = true;
        throw new Error("worker failed");
      },
    }),
    (message) => {
      failure = message;
    },
  );
  const result = await command(["prepare", "python"], {
    fs,
    cwd: "/workspace",
    signal: controller.signal,
  });
  expect(result.exitCode).toBe(130);
  expect(closed).toBe(true);
  expect(failure).toContain("runtime cleanup failed: worker failed");
});

test("guest snapshots reject malformed paths and oversized files before copying bytes", async () => {
  let reads = 0;
  const fs = {
    readDir: async () => [{ name: "../outside", kind: "file", size: 1 }],
    readFile: async () => {
      reads++;
      return new Uint8Array(1);
    },
  };
  await expect(readGuestWorkspace(fs)).rejects.toThrow("Invalid guest workspace path");
  fs.readDir = async () => [{ name: "large", kind: "file", size: 8 * 1024 * 1024 + 1 }];
  await expect(readGuestWorkspace(fs)).rejects.toThrow("snapshot limits");
  expect(reads).toBe(0);
});

test("guest snapshots enforce the aggregate bound and detect file changes", async () => {
  const fs = {
    readDir: async () =>
      Array.from({ length: 5 }, (_, i) => ({
        name: `${i}.bin`,
        kind: "file",
        size: 8 * 1024 * 1024,
      })),
    readFile: async () => new Uint8Array(8 * 1024 * 1024),
  };
  await expect(readGuestWorkspace(fs)).rejects.toThrow("snapshot limits");
  fs.readDir = async () => [{ name: "changing", kind: "file", size: 1 }];
  fs.readFile = async () => new Uint8Array(2);
  await expect(readGuestWorkspace(fs)).rejects.toThrow("Guest file changed");
});

test("workspace reconciliation preserves binary data and empty directories, and handles type changes", async () => {
  const fs = new InMemoryFs({
    "/workspace/file-to-dir": "old",
    "/workspace/dir-to-file/old": "remove",
    "/workspace/deleted": "remove",
  });
  await fs.mkdir("/workspace/empty", { recursive: true });
  const before = await readShellWorkspace(fs);
  const after = {
    directories: ["/workspace/empty", "/workspace/file-to-dir"],
    files: {
      "/workspace/dir-to-file": new Uint8Array([0, 128, 255]),
      "/workspace/file-to-dir/new": new Uint8Array([42]),
    },
  };
  await reconcileGuestWorkspace(fs, before, after);
  expect(await readShellWorkspace(fs)).toEqual(after);
});

test("read-only guests do not rewrite unchanged files or recreate directories", async () => {
  const fs = new InMemoryFs({ "/workspace/src/file.bin": new Uint8Array([0, 128, 255]) });
  const before = await readShellWorkspace(fs);
  const after = {
    directories: [...before.directories],
    files: { "/workspace/src/file.bin": new Uint8Array([0, 128, 255]) },
  };
  let writes = 0;
  let directories = 0;
  const originalWrite = fs.writeFile.bind(fs);
  fs.writeFile = async (...args) => {
    writes++;
    return originalWrite(...args);
  };
  const originalMkdir = fs.mkdir.bind(fs);
  fs.mkdir = async (...args) => {
    directories++;
    return originalMkdir(...args);
  };
  await reconcileGuestWorkspace(fs, before, after);
  expect(writes).toBe(0);
  expect(directories).toBe(0);
  after.files["/workspace/src/file.bin"][2] = 42;
  await reconcileGuestWorkspace(fs, before, after);
  expect(writes).toBe(1);
  expect(Array.from(await fs.readFileBuffer("/workspace/src/file.bin"))).toEqual([0, 128, 42]);
});

test("guest snapshots bound empty entries as well as file contents", async () => {
  const fs = {
    readDir: async () =>
      Array.from({ length: 10001 }, (_, index) => ({
        name: `empty-${index}`,
        kind: "directory",
        size: 0,
      })),
  };
  await expect(readGuestWorkspace(fs)).rejects.toThrow("10,000 entries");
});
