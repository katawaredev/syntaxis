import { afterAll, beforeEach, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import "./bridge-source.js";

const bridge = globalThis.SyntaxisBrowserGit;
const temporary = mkdtempSync(join(tmpdir(), "syntaxis-git-test-"));
const remote = join(temporary, "remote.git");
const seed = join(temporary, "seed");
const originalFetch = globalThis.fetch;
const originalNavigator = globalThis.navigator;
const git = (...args) =>
  execFileSync("git", args, { encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] });
git("init", "--bare", "--initial-branch=main", remote);
git("init", "--initial-branch=main", seed);
git("-C", seed, "config", "user.name", "Fixture");
git("-C", seed, "config", "user.email", "fixture@example.test");
writeFileSync(join(seed, "README.md"), "initial\n");
git("-C", seed, "add", ".");
git("-C", seed, "commit", "-m", "Initial");
git("-C", seed, "remote", "add", "origin", remote);
git("-C", seed, "push", "origin", "main");

class Directory {
  kind = "directory";
  children = new Map();
  constructor(name) {
    this.name = name;
  }
  async getDirectoryHandle(name, { create = false } = {}) {
    return this.get(name, "directory", create);
  }
  async getFileHandle(name, { create = false } = {}) {
    return this.get(name, "file", create);
  }
  get(name, kind, create) {
    let value = this.children.get(name);
    if (value && value.kind !== kind) throw new DOMException("Wrong kind", "TypeMismatchError");
    if (!value && !create) throw new DOMException("Missing", "NotFoundError");
    if (!value) {
      value = kind === "directory" ? new Directory(name) : new FileHandle(name);
      this.children.set(name, value);
    }
    return value;
  }
  async removeEntry(name, { recursive = false } = {}) {
    const value = this.children.get(name);
    if (!value) throw new DOMException("Missing", "NotFoundError");
    if (value.kind === "directory" && value.children.size && !recursive)
      throw new Error("Not empty");
    this.children.delete(name);
  }
  async *keys() {
    yield* this.children.keys();
  }
  async *values() {
    yield* this.children.values();
  }
  async *entries() {
    yield* this.children.entries();
  }
}
class FileHandle {
  kind = "file";
  bytes = new Uint8Array();
  constructor(name) {
    this.name = name;
  }
  async getFile() {
    return new File([this.bytes], this.name, { lastModified: Date.now() });
  }
  async createWritable() {
    return {
      write: async (bytes) => {
        this.bytes =
          typeof bytes === "string" ? new TextEncoder().encode(bytes) : new Uint8Array(bytes);
      },
      close: async () => {},
    };
  }
}
let storage;
let requests;
const settings = {
  origin: "https://github.test",
  proxy: "https://proxy.test",
  username: "fixture",
  token: "private-test-token",
  name: "Browser User",
  email: "browser@example.test",
};

beforeEach(async () => {
  storage = new Directory("root");
  requests = [];
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: { storage: { getDirectory: async () => storage } },
  });
  globalThis.__SYNTAXIS_BROWSER_WORKSPACE_ROOT__ = undefined;
  globalThis.fetch = async (url, options) => {
    requests.push({ url, options });
    const service = url.includes("git-receive-pack") ? "git-receive-pack" : "git-upload-pack";
    const advertising = options.method === "GET";
    const output = execFileSync(
      "git",
      [service.slice(4), "--stateless-rpc", ...(advertising ? ["--advertise-refs"] : []), remote],
      { input: options.body },
    );
    const line = `# service=${service}\n`;
    const body = advertising
      ? Buffer.concat([
          Buffer.from((line.length + 4).toString(16).padStart(4, "0") + line + "0000"),
          output,
        ])
      : output;
    return new Response(body, {
      headers: {
        "content-type": `application/x-${service}-${advertising ? "advertisement" : "result"}`,
      },
    });
  };
  await bridge.configure(settings);
});

afterAll(() => {
  globalThis.fetch = originalFetch;
  Object.defineProperty(globalThis, "navigator", { configurable: true, value: originalNavigator });
  delete globalThis.__SYNTAXIS_BROWSER_WORKSPACE_ROOT__;
  rmSync(temporary, { recursive: true, force: true });
});

async function clone(name, mode = "full") {
  const id = bridge.startClone({
    url: "https://github.test/org/repo.git",
    destination_parent: "/",
    directory_name: name,
    mode,
  });
  let status;
  do {
    status = await bridge.cloneStatus(id);
  } while (!status.done);
  await bridge.finishClone(id);
  if (status.error) throw new Error(status.error);
  const directory = await (
    await storage.getDirectoryHandle(".syntaxis-repositories")
  ).getDirectoryHandle(name);
  globalThis.__SYNTAXIS_BROWSER_WORKSPACE_ROOT__ = directory;
  return directory;
}

test("full clone, edit, stage, commit, push and fetch use a real Git remote", async () => {
  const directory = await clone("roundtrip");
  expect(await bridge.listProjects()).toEqual(["roundtrip"]);
  expect((await bridge.repository()).branch).toBe("main");
  const file = await directory.getFileHandle("README.md");
  const writer = await file.createWritable();
  await writer.write("edited in browser\n");
  await writer.close();
  expect((await bridge.repository()).changes[0].unstaged).toBe("modified");
  await bridge.stage(["README.md"]);
  const { oid } = await bridge.commit({ message: "Browser commit" });
  const detail = await bridge.commitDetail(oid);
  expect(detail.commit.author_name).toBe("Browser User");
  expect(detail.files_changed).toBe(1);
  expect(detail.patch).toContain("+edited in browser");
  expect(detail.additions).toBe(1);
  expect(detail.deletions).toBe(1);
  expect((await bridge.history({ offset: 1, limit: 1 }))[0].subject).toBe("Initial");
  await bridge.push({});
  expect(git("--git-dir", remote, "rev-parse", "main").trim()).toBe(oid);
  expect(git("--git-dir", remote, "show", "main:README.md")).toBe("edited in browser\n");
  await bridge.fetch();
  expect((await bridge.repository()).ahead).toBe(0);
  const config = await (
    await (await directory.getDirectoryHandle(".git")).getFileHandle("config")
  ).getFile();
  const configText = await config.text();
  expect(configText).not.toContain(settings.token);
  expect(configText).not.toContain(settings.proxy);
  expect(requests.every(({ url }) => url.startsWith(settings.proxy))).toBe(true);
  expect(
    requests.every(({ options }) => options.credentials === "omit" && options.redirect === "error"),
  ).toBe(true);
});

test("shallow clones retain shallow metadata and cannot overwrite an existing project", async () => {
  const directory = await clone("shallow", "shallow");
  expect(await (await directory.getDirectoryHandle(".git")).getFileHandle("shallow")).toBeTruthy();
  await expect(clone("shallow")).rejects.toThrow("destination already exists");
  expect(await bridge.listProjects()).toEqual(["shallow"]);
});

test("cancellation cleans up the new clone and leaves unrelated workspace files alone", async () => {
  const sentinel = await storage.getFileHandle("keep.txt", { create: true });
  let started;
  const fetching = new Promise((resolve) => {
    started = resolve;
  });
  globalThis.fetch = async (_url, options) => {
    started();
    return new Promise((_resolve, reject) =>
      options.signal.addEventListener("abort", () => reject(new Error("aborted"))),
    );
  };
  const id = bridge.startClone({
    url: "https://github.test/org/repo.git",
    destination_parent: "/",
    directory_name: "cancelled",
    mode: "full",
  });
  await fetching;
  bridge.cancelClone(id);
  let result;
  do {
    result = await bridge.cloneStatus(id);
  } while (!result.done);
  expect(result.cancelled).toBe(true);
  expect(await storage.getFileHandle("keep.txt")).toBe(sentinel);
  expect(await bridge.listProjects()).toEqual([]);
  expect((await bridge.cloneStatus(id)).cancelled).toBe(true);
  bridge.finishClone(id);
});

test("credentials are scoped to the configured origin and cannot be embedded in URLs", async () => {
  expect(() => bridge.startClone({ url: "https://user:secret@github.test/repo" })).toThrow(
    "embedded credentials",
  );
  globalThis.fetch = async (url, options) => {
    requests.push({ url, options });
    return new Response(null, { status: 401 });
  };
  await expect(bridge.check("https://other.test/repo.git")).rejects.toThrow(
    "Authentication required",
  );
  expect(
    requests.every(
      ({ options }) => !options.headers.Authorization && !options.headers.authorization,
    ),
  ).toBe(true);
  requests = [];
  await expect(bridge.check("https://github.test/repo.git")).rejects.toThrow(
    "authentication failed",
  );
  expect(
    requests.some(({ options }) =>
      Object.values(options.headers).some((value) =>
        String(value).includes(Buffer.from(`fixture:${settings.token}`).toString("base64")),
      ),
    ),
  ).toBe(true);
});

test("unsafe and unsupported operations fail without network requests", async () => {
  expect(() =>
    bridge.startClone({
      url: "https://github.test/repo.git",
      directory_name: "../escape",
      destination_parent: "/",
    }),
  ).toThrow("destination");
  expect(() =>
    bridge.startClone({
      url: "https://github.test/repo.git",
      directory_name: "blobless",
      destination_parent: "/",
      mode: "blobless",
    }),
  ).toThrow("Blobless");
  await expect(bridge.push({ force_with_lease: true })).rejects.toThrow("Force-with-lease");
  expect(requests).toHaveLength(0);
});

test("line counts distinguish index and worktree, including separated edits", async () => {
  const directory = await clone("counts");
  const file = await directory.getFileHandle("counts.txt", { create: true });
  const write = async (text) => {
    const writer = await file.createWritable();
    await writer.write(text);
    await writer.close();
  };
  await write("one\ntwo\nthree\nfour\nfive\n");
  expect(
    (await bridge.repository()).changes.find((c) => c.path === "counts.txt").unstaged_additions,
  ).toBe(5);
  await bridge.stage(["counts.txt"]);
  await bridge.commit({ message: "Count baseline" });
  await write("ONE\ntwo\nthree\nfour\nFIVE\n");
  await bridge.stage(["counts.txt"]);
  await write("ONE\ntwo\nthree\nfour\nFIVE\nsix\n");
  expect((await bridge.repository()).changes.find((c) => c.path === "counts.txt")).toMatchObject({
    staged_additions: 2,
    staged_deletions: 2,
    unstaged_additions: 1,
    unstaged_deletions: 0,
  });
  await write("\0binary");
  expect(
    (await bridge.repository()).changes.find((c) => c.path === "counts.txt").unstaged_additions,
  ).toBe(0);
});

test("amend replaces HEAD, preserves its author and includes staged changes", async () => {
  const directory = await clone("amend");
  const original = (await bridge.history({ offset: 0, limit: 1 }))[0];
  const { oid: renamed } = await bridge.commit({ message: "Amended message", amend: true });
  const first = await bridge.commitDetail(renamed);
  expect(first.commit.parents).toEqual(original.parents);
  expect(first.commit.author_name).toBe(original.author_name);
  expect(first.commit.authored_unix_seconds).toBe(original.timestamp);
  const writer = await (await directory.getFileHandle("README.md")).createWritable();
  await writer.write("amended content\n");
  await writer.close();
  await bridge.stage(["README.md"]);
  const { oid } = await bridge.commit({ message: "Amended content", amend: true });
  expect(oid).not.toBe(renamed);
  expect((await bridge.commitDetail(oid)).commit.parents).toEqual(original.parents);
  expect((await bridge.repository()).changes).toEqual([]);
});

test("staged diffs read index content for root and nested files", async () => {
  const directory = await clone("index-diffs");
  const nested = await directory.getDirectoryHandle("src", { create: true });
  const rootFile = await directory.getFileHandle("README.md");
  const nestedFile = await nested.getFileHandle("app.js", { create: true });
  const write = async (file, text) => {
    const writer = await file.createWritable();
    await writer.write(text);
    await writer.close();
  };
  await write(nestedFile, "original nested\n");
  await bridge.stage(["src/app.js"]);
  await bridge.commit({ message: "Nested baseline" });
  for (const [path, file] of [
    ["README.md", rootFile],
    ["src/app.js", nestedFile],
  ]) {
    const before = await bridge.diff(path, "worktree");
    expect(before.before).toBe(before.after);
    expect(before.before).not.toBe("");
    await write(file, "staged content\n");
    const unstaged = await bridge.diff(path, "worktree");
    await bridge.stage([path]);
    expect(await bridge.diff(path, "staged")).toEqual(unstaged);
    await write(file, "later edit\n");
    expect(await bridge.diff(path, "worktree")).toMatchObject({
      before: "staged content\n",
      after: "later edit\n",
    });
    expect((await bridge.diff(path, "staged")).after).toBe("staged content\n");
    await bridge.discard([path]);
    expect(await (await file.getFile()).text()).toBe("staged content\n");
  }
});
