import * as git from "isomorphic-git";
import { Buffer } from "buffer";
import http from "isomorphic-git/http/web";

globalThis.Buffer ??= Buffer;

const DIR = "/";
const GITDIR = "/.git";
const encoder = new TextEncoder();
const decoder = new TextDecoder();
let cache = {};
let operationRoot;
let cancellation;
let queue = Promise.resolve();
const connections = new Map();
let identity = { name: "", email: "" };
const clones = new Map();
const PROJECTS = ".syntaxis-repositories";
const indexStats = new Map();
let observedWorkspaceRevision;
let forceWorktreeScan = false;

function fsError(code, path, message) {
  const error = new Error(`${message}: ${path}`);
  error.code = code;
  error.path = path;
  return error;
}

function parts(path) {
  const result = String(path)
    .replaceAll("\\", "/")
    .split("/")
    .filter((part) => part && part !== ".");
  if (result.includes("..")) throw new Error("Parent traversal is unavailable in browser Git.");
  return result;
}

async function workspaceRoot() {
  cancellation?.throwIfAborted();
  return (
    operationRoot ??
    globalThis.__SYNTAXIS_BROWSER_WORKSPACE_ROOT__ ??
    navigator.storage.getDirectory()
  );
}

async function directoryAt(path, create = false) {
  let directory = await workspaceRoot();
  for (const part of parts(path)) {
    try {
      directory = await directory.getDirectoryHandle(part, { create });
    } catch (error) {
      throw fsError(
        error?.name === "TypeMismatchError" ? "ENOTDIR" : "ENOENT",
        path,
        "Directory is unavailable",
      );
    }
  }
  return directory;
}

async function parentAt(path, create = false) {
  const segments = parts(path);
  const name = segments.pop();
  if (!name) throw fsError("EINVAL", path, "A file name is required");
  return { directory: await directoryAt(`/${segments.join("/")}`, create), name };
}

async function handleAt(path) {
  if (parts(path).length === 0) return { handle: await workspaceRoot(), kind: "directory" };
  const { directory, name } = await parentAt(path);
  try {
    return { handle: await directory.getFileHandle(name), kind: "file" };
  } catch (fileError) {
    try {
      return { handle: await directory.getDirectoryHandle(name), kind: "directory" };
    } catch {
      throw fsError(
        fileError?.name === "TypeMismatchError" ? "EISDIR" : "ENOENT",
        path,
        "Entry is unavailable",
      );
    }
  }
}

function pathInode(path) {
  let hash = 2166136261;
  for (const character of path) {
    hash ^= character.codePointAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

function stats(kind, file, path) {
  const modified = file?.lastModified ?? 0;
  const indexed = forceWorktreeScan ? undefined : indexStats.get(path.replace(/^\//, ""));
  return {
    size: file?.size ?? 0,
    mode: kind === "directory" ? 0o040000 : 0o100644,
    mtimeMs: modified,
    ctimeMs: modified,
    uid: indexed?.uid ?? 0,
    gid: indexed?.gid ?? 0,
    dev: indexed?.dev ?? 0,
    ino: indexed?.ino ?? pathInode(path),
    ctimeSeconds: indexed?.ctimeSeconds,
    ctimeNanoseconds: indexed?.ctimeNanoseconds,
    mtime: new Date(modified),
    ctime: new Date(modified),
    isFile: () => kind === "file",
    isDirectory: () => kind === "directory",
    isSymbolicLink: () => false,
  };
}

async function removeTree(directory) {
  for await (const [name, handle] of directory.entries()) {
    if (handle.kind === "directory") await removeTree(handle);
    await directory.removeEntry(name);
  }
}

const fs = {
  promises: {
    async readFile(path, options) {
      const { handle, kind } = await handleAt(path);
      if (kind !== "file") throw fsError("EISDIR", path, "Cannot read a directory");
      const file = await handle.getFile();
      const bytes = new Uint8Array(await file.arrayBuffer());
      const encoding = typeof options === "string" ? options : options?.encoding;
      return encoding ? decoder.decode(bytes) : bytes;
    },
    async writeFile(path, value, options) {
      const { directory, name } = await parentAt(path);
      let handle;
      try {
        handle = await directory.getFileHandle(name, { create: true });
      } catch {
        throw fsError("ENOENT", path, "Parent directory is unavailable");
      }
      const writable = await handle.createWritable();
      const encoding = typeof options === "string" ? options : options?.encoding;
      const bytes = typeof value === "string" ? encoder.encode(value) : value;
      await writable.write(encoding && typeof value !== "string" ? decoder.decode(value) : bytes);
      await writable.close();
    },
    async mkdir(path) {
      const { directory, name } = await parentAt(path);
      try {
        await directory.getDirectoryHandle(name, { create: true });
      } catch {
        throw fsError("ENOENT", path, "Could not create directory");
      }
    },
    async rmdir(path) {
      const { directory, name } = await parentAt(path);
      const child = await directory.getDirectoryHandle(name).catch(() => {
        throw fsError("ENOENT", path, "Directory is unavailable");
      });
      for await (const _entry of child.values()) {
        throw fsError("ENOTEMPTY", path, "Directory is not empty");
      }
      await directory.removeEntry(name);
    },
    async rm(path, options = {}) {
      const { directory, name } = await parentAt(path);
      try {
        const target = await directory.getDirectoryHandle(name);
        if (options.recursive) await removeTree(target);
      } catch {
        // Files and missing entries are both handled by removeEntry below.
      }
      try {
        await directory.removeEntry(name);
      } catch (error) {
        if (!options.force) throw fsError("ENOENT", path, error?.message ?? "Entry is unavailable");
      }
    },
    async unlink(path) {
      const { directory, name } = await parentAt(path);
      try {
        const handle = await directory.getFileHandle(name);
        if (handle.kind !== "file") throw fsError("EISDIR", path, "Cannot unlink a directory");
        await directory.removeEntry(name);
      } catch (error) {
        if (typeof error?.code === "string") throw error;
        throw fsError("ENOENT", path, "File is unavailable");
      }
    },
    async stat(path) {
      const { handle, kind } = await handleAt(path);
      return stats(kind, kind === "file" ? await handle.getFile() : undefined, path);
    },
    async lstat(path) {
      return this.stat(path);
    },
    async readdir(path) {
      const directory = await directoryAt(path);
      const names = [];
      for await (const name of directory.keys()) names.push(name);
      return names;
    },
    async readlink(path) {
      throw fsError("ENOSYS", path, "Symbolic links are unavailable in browser storage");
    },
    async symlink(_target, path) {
      throw fsError("ENOSYS", path, "Symbolic links are unavailable in browser storage");
    },
  },
};

function kind(head, value) {
  if (head === 0 && value !== 0) return "added";
  if (value === 0) return "deleted";
  return "modified";
}

function mapStatus([path, head, worktree, stage]) {
  const staged = stage !== head;
  const unstaged = worktree !== stage;
  return {
    path,
    staged: staged ? kind(head, stage) : null,
    unstaged: unstaged ? kind(stage, worktree) : null,
    head,
    worktree,
    stage,
  };
}

async function isRepository() {
  try {
    await fs.promises.stat(GITDIR);
    return true;
  } catch {
    return false;
  }
}

async function primeIndexStats() {
  if (indexStats.size > 0) return;
  try {
    await git.walk({
      fs,
      dir: DIR,
      cache,
      trees: [git.STAGE()],
      map: async (path, [entry]) => {
        const stat = await entry?.stat();
        if (stat) indexStats.set(path, stat);
        return null;
      },
    });
  } catch {
    // New repositories have no index yet.
  }
}

async function repository(request = {}) {
  if (
    request?.revision !== undefined &&
    observedWorkspaceRevision !== undefined &&
    request.revision !== observedWorkspaceRevision
  ) {
    forceWorktreeScan = true;
  }
  if (request?.revision !== undefined) observedWorkspaceRevision = request.revision;
  if (!(await isRepository()))
    return {
      initialized: false,
      branch: null,
      branches: [],
      remotes: [],
      changes: [],
      commits: [],
      author_name: null,
      author_email: null,
      upstream: null,
      ahead: 0,
      behind: 0,
    };
  await primeIndexStats();
  const [matrix, branch, branches, remotes, authorName, authorEmail] = await Promise.all([
    git.statusMatrix({ fs, dir: DIR, cache }),
    git.currentBranch({ fs, dir: DIR, cache, fullname: false }),
    git.listBranches({ fs, dir: DIR, cache }),
    git.listRemotes({ fs, dir: DIR, cache }),
    git.getConfig({ fs, dir: DIR, cache, path: "user.name" }),
    git.getConfig({ fs, dir: DIR, cache, path: "user.email" }),
  ]);
  for (const remote of remotes) {
    remote.push_url =
      (await git.getConfig({ fs, dir: DIR, path: `remote.${remote.remote}.pushurl` })) ?? null;
  }
  forceWorktreeScan = false;
  indexStats.clear();
  if (branch && !branches.includes(branch)) branches.unshift(branch);
  let commits = [];
  try {
    commits = (await git.log({ fs, dir: DIR, cache, depth: 100 })).map(({ oid, commit }) => ({
      oid,
      short_oid: oid.slice(0, 7),
      subject: commit.message.split("\n", 1)[0],
      message: commit.message,
      parents: commit.parent,
      author_name: commit.author.name,
      author_email: commit.author.email,
      timestamp: commit.author.timestamp,
      date: new Date(commit.author.timestamp * 1000).toLocaleString(),
    }));
  } catch {
    // An initialized repository has no log before its first commit.
  }
  let upstream = null;
  let ahead = 0;
  let behind = 0;
  if (branch) {
    const remoteName = await git.getConfig({
      fs,
      dir: DIR,
      cache,
      path: `branch.${branch}.remote`,
    });
    const mergeRef = await git.getConfig({
      fs,
      dir: DIR,
      cache,
      path: `branch.${branch}.merge`,
    });
    if (remoteName && mergeRef) {
      const remoteBranch = mergeRef.replace("refs/heads/", "");
      upstream = `${remoteName}/${remoteBranch}`;
      const upstreamRef =
        remoteName === "." ? mergeRef : `refs/remotes/${remoteName}/${remoteBranch}`;
      try {
        const [localLog, remoteLog] = await Promise.all([
          git.log({ fs, dir: DIR, cache, ref: branch, depth: 1_000 }),
          git.log({ fs, dir: DIR, cache, ref: upstreamRef, depth: 1_000 }),
        ]);
        const local = new Set(localLog.map((entry) => entry.oid));
        const remote = new Set(remoteLog.map((entry) => entry.oid));
        ahead = localLog.filter((entry) => !remote.has(entry.oid)).length;
        behind = remoteLog.filter((entry) => !local.has(entry.oid)).length;
      } catch {
        // The upstream may not have been fetched in this browser yet.
      }
    }
  }
  return {
    initialized: true,
    branch,
    branches,
    remotes,
    changes: await Promise.all(
      matrix
        .map(mapStatus)
        .filter((change) => change.staged || change.unstaged)
        .map(async (change) => {
          const staged = change.staged ? lineCounts(await diff(change.path, "staged")) : [0, 0];
          const unstaged = change.unstaged
            ? lineCounts(await diff(change.path, "worktree"))
            : [0, 0];
          return {
            ...change,
            staged_additions: staged[0],
            staged_deletions: staged[1],
            unstaged_additions: unstaged[0],
            unstaged_deletions: unstaged[1],
          };
        }),
    ),
    commits,
    author_name: authorName,
    author_email: authorEmail,
    upstream,
    ahead,
    behind,
  };
}

async function init(defaultBranch = "main") {
  await git.init({ fs, dir: DIR, cache, defaultBranch });
  return repository();
}

async function stage(paths) {
  for (const path of paths) {
    const row = (await git.statusMatrix({ fs, dir: DIR, cache, filepaths: [path] }))[0];
    if (row?.[2] === 0) await git.remove({ fs, dir: DIR, cache, filepath: path });
    else await git.add({ fs, dir: DIR, cache, filepath: path });
  }
  indexStats.clear();
  return repository();
}

async function unstage(paths) {
  for (const path of paths) {
    const row = (await git.statusMatrix({ fs, dir: DIR, cache, filepaths: [path] }))[0];
    if (!row) continue;
    if (row[1] === 0) await git.remove({ fs, dir: DIR, cache, filepath: path });
    else await git.resetIndex({ fs, dir: DIR, cache, filepath: path });
  }
  indexStats.clear();
  return repository();
}

async function discard(paths) {
  for (const path of paths) {
    const row = (await git.statusMatrix({ fs, dir: DIR, cache, filepaths: [path] }))[0];
    if (!row) continue;
    if (row[3] === 0) {
      try {
        await fs.promises.unlink(`/${path}`);
      } catch {
        // The untracked file may already have been removed.
      }
    } else {
      await fs.promises.writeFile(`/${path}`, await stageContent(path));
    }
  }
  indexStats.clear();
  return repository();
}

async function commit({ message, amend = false }) {
  const name = identity.name || (await git.getConfig({ fs, dir: DIR, path: "user.name" }));
  const email = identity.email || (await git.getConfig({ fs, dir: DIR, path: "user.email" }));
  if (!name || !email)
    throw new Error("Set your author name and email in Git connection settings before committing.");
  await git.setConfig({ fs, dir: DIR, cache, path: "user.name", value: name });
  await git.setConfig({ fs, dir: DIR, cache, path: "user.email", value: email });
  const oid = await git.commit({
    fs,
    dir: DIR,
    cache,
    message,
    amend,
    author: amend ? undefined : { name, email },
    committer: { name, email },
  });
  indexStats.clear();
  return { oid, repository: await repository() };
}

async function headContent(path) {
  try {
    const oid = await git.resolveRef({ fs, dir: DIR, gitdir: GITDIR, cache, ref: "HEAD" });
    return new Uint8Array((await git.readBlob({ fs, dir: DIR, cache, oid, filepath: path })).blob);
  } catch {
    return new Uint8Array();
  }
}

async function stageContent(path) {
  let oid;
  await git.walk({
    fs,
    dir: DIR,
    cache,
    trees: [git.STAGE()],
    map: async (filepath, [entry]) => {
      if (filepath === path && (await entry?.type()) === "blob") oid = await entry.oid();
      // Returning null prunes a directory, including the repository root.
      // Visit the target's ancestors so nested index entries can be read.
      return filepath === "." || path.startsWith(`${filepath}/`) ? undefined : null;
    },
  });
  if (!oid) return new Uint8Array();
  return new Uint8Array((await git.readBlob({ fs, dir: DIR, cache, oid })).blob);
}

async function worktreeContent(path) {
  try {
    return new Uint8Array(await fs.promises.readFile(`/${path}`));
  } catch {
    return new Uint8Array();
  }
}

// Myers shortest edit distance counts changed lines without counting unchanged
// lines between separate edits. Keep only the frontier, not the edit history.
function lineCounts({ before, after, binary }) {
  if (binary || before === after) return [0, 0];
  const left = before.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  const right = after.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  if (!left.length || !right.length) return [right.length, left.length];
  const frontier = new Map([[1, 0]]);
  for (let distance = 0; distance <= left.length + right.length; distance++) {
    for (let diagonal = -distance; diagonal <= distance; diagonal += 2) {
      let x =
        diagonal === -distance ||
        (diagonal !== distance &&
          (frontier.get(diagonal - 1) ?? -1) < (frontier.get(diagonal + 1) ?? -1))
          ? (frontier.get(diagonal + 1) ?? 0)
          : (frontier.get(diagonal - 1) ?? 0) + 1;
      let y = x - diagonal;
      while (x < left.length && y < right.length && left[x] === right[y]) {
        x++;
        y++;
      }
      frontier.set(diagonal, x);
      if (x >= left.length && y >= right.length) {
        const additions = (distance + right.length - left.length) / 2;
        return [additions, distance - additions];
      }
    }
  }
}

async function diff(path, area) {
  const before = area === "staged" ? await headContent(path) : await stageContent(path);
  const after = area === "staged" ? await stageContent(path) : await worktreeContent(path);
  const binary = before.subarray(0, 8_000).includes(0) || after.subarray(0, 8_000).includes(0);
  return {
    path,
    binary,
    before: binary ? "" : decoder.decode(before),
    after: binary ? "" : decoder.decode(after),
  };
}

async function checkout(ref) {
  await git.checkout({ fs, dir: DIR, cache, ref });
  indexStats.clear();
  return repository();
}

async function createBranch({ ref, startPoint, checkout: switchToBranch }) {
  const object = startPoint
    ? await git.resolveRef({ fs, dir: DIR, cache, ref: startPoint })
    : undefined;
  await git.branch({ fs, dir: DIR, cache, ref, object });
  if (switchToBranch) await git.checkout({ fs, dir: DIR, cache, ref });
  indexStats.clear();
  return repository();
}

async function renameBranch({ oldref, ref }) {
  await git.renameBranch({ fs, dir: DIR, cache, oldref, ref, checkout: true });
  indexStats.clear();
  return repository();
}

async function deleteBranch({ ref, force }) {
  const current = await git.currentBranch({ fs, dir: DIR, cache });
  if (current === ref) throw new Error("Cannot delete the current branch.");
  if (!force) {
    const oid = await git.resolveRef({ fs, dir: DIR, cache, ref });
    const head = await git.resolveRef({ fs, dir: DIR, cache, ref: "HEAD" });
    if (
      oid !== head &&
      !(await git.isDescendent({ fs, dir: DIR, cache, oid: head, ancestor: oid }))
    ) {
      throw new Error(
        "The branch is not merged. Use force deletion only if you intend to discard it.",
      );
    }
  }
  await git.deleteBranch({ fs, dir: DIR, cache, ref });
  indexStats.clear();
  return repository();
}

function httpsUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("Enter a valid HTTPS URL.");
  }
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash) {
    throw new Error(
      "Use an HTTPS URL without embedded credentials, query parameters, or fragments.",
    );
  }
  return url;
}

function configure(settings) {
  const origin = httpsUrl(settings.origin).origin;
  const proxy = settings.proxy.trim();
  if (proxy) httpsUrl(proxy);
  if (/[^\x20-\x7e]/.test(settings.username + settings.token) || settings.username.includes(":")) {
    throw new Error("Invalid Git credentials.");
  }
  connections.set(origin, { proxy, username: settings.username.trim(), token: settings.token });
  identity = { name: settings.name.trim(), email: settings.email.trim() };
  return true;
}

function network(url) {
  const target = httpsUrl(url);
  const settings = connections.get(target.origin) ?? {};
  const signal = cancellation;
  return {
    url: target.href,
    // Empty overrides any proxy imported in a repository's .git/config.
    corsProxy: settings.proxy || "",
    http: {
      async request(request) {
        signal?.throwIfAborted();
        // Never follow redirects with credentials or browser cookies.
        try {
          return await http.request({
            ...request,
            signal,
            fetchOptions: {
              credentials: "omit",
              redirect: "error",
              signal,
            },
          });
        } catch {
          signal?.throwIfAborted();
          throw new Error(
            "Git connection failed. Check the HTTPS URL and trusted CORS proxy in Git connection settings.",
          );
        }
      },
    },
    onAuth: () => {
      if (!settings.token)
        throw new Error(
          "Authentication required. Add a token for this host in Git connection settings.",
        );
      return { username: settings.username || "x-access-token", password: settings.token };
    },
    onAuthFailure: () => {
      throw new Error("Git authentication failed. Check the token and repository permissions.");
    },
  };
}

async function remoteUrl(remote, push = false) {
  const url =
    (push && (await git.getConfig({ fs, dir: DIR, path: `remote.${remote}.pushurl` }))) ||
    (await git.getConfig({ fs, dir: DIR, path: `remote.${remote}.url` }));
  if (!url) throw new Error("The remote has no URL.");
  return url;
}

async function saveRemote({ previous_name, name, fetch_url, push_url }) {
  if (!/^[\w-]+$/.test(name))
    throw new Error("Use letters, numbers, underscores, or hyphens for the remote name.");
  httpsUrl(fetch_url);
  if (push_url) httpsUrl(push_url);
  if (previous_name && previous_name !== name) {
    throw new Error(
      "Browser Git cannot rename a remote yet. Add the new remote and publish the branch before removing the old one.",
    );
  }
  await git.addRemote({ fs, dir: DIR, remote: name, url: fetch_url, force: !!previous_name });
  await git.setConfig({
    fs,
    dir: DIR,
    path: `remote.${name}.pushurl`,
    value: push_url || undefined,
  });
  return true;
}

async function fetchRemote(remote) {
  await git.fetch({ fs, dir: DIR, cache, remote, ...network(await remoteUrl(remote)) });
  return { message: `Fetched ${remote}.` };
}

async function upstream() {
  const ref = await git.currentBranch({ fs, dir: DIR, cache });
  if (!ref) throw new Error("Switch to a branch before synchronizing.");
  const remote = await git.getConfig({ fs, dir: DIR, path: `branch.${ref}.remote` });
  const merge = await git.getConfig({ fs, dir: DIR, path: `branch.${ref}.merge` });
  if (!remote || !merge) throw new Error("Publish this branch to set its upstream first.");
  return { ref, remote, remoteRef: merge.replace(/^refs\/heads\//, "") };
}

async function pull() {
  const { ref, remote, remoteRef } = await upstream();
  const matrix = await git.statusMatrix({ fs, dir: DIR, cache });
  if (matrix.some((row) => row[1] !== row[2] || row[2] !== row[3])) {
    throw new Error("Commit or discard local changes before pulling.");
  }
  await git.fastForward({
    fs,
    dir: DIR,
    cache,
    ref,
    remote,
    remoteRef,
    ...network(await remoteUrl(remote)),
  });
  indexStats.clear();
  return { message: "Pulled upstream changes (fast-forward only)." };
}

async function push({ publish, force_with_lease = false } = {}) {
  if (force_with_lease) throw new Error("Force-with-lease is unavailable in browser Git.");
  let target;
  if (publish) {
    const ref = await git.currentBranch({ fs, dir: DIR, cache });
    if (!ref) throw new Error("Switch to a branch before publishing.");
    target = { ref, remote: publish, remoteRef: ref };
  } else target = await upstream();
  const result = await git.push({
    fs,
    dir: DIR,
    cache,
    ...target,
    ...network(await remoteUrl(target.remote, true)),
    force: false,
  });
  if (!result.ok || Object.values(result.refs ?? {}).some((ref) => !ref.ok)) {
    throw new Error(
      "The remote rejected the push. Fetch and resolve upstream changes, and check repository permissions.",
    );
  }
  if (publish) {
    await git.setConfig({
      fs,
      dir: DIR,
      path: `branch.${target.ref}.remote`,
      value: target.remote,
    });
    await git.setConfig({
      fs,
      dir: DIR,
      path: `branch.${target.ref}.merge`,
      value: `refs/heads/${target.remoteRef}`,
    });
  }
  return { message: "Pushed branch successfully." };
}

async function history({ offset, limit }) {
  return (await git.log({ fs, dir: DIR, cache, depth: offset + limit }))
    .slice(offset)
    .map(({ oid, commit }) => ({
      oid,
      short_oid: oid.slice(0, 7),
      parents: commit.parent,
      subject: commit.message.split("\n")[0],
      message: commit.message,
      author_name: commit.author.name,
      author_email: commit.author.email,
      timestamp: commit.author.timestamp,
    }));
}

async function commitMessage(ref) {
  const oid = await git.resolveRef({ fs, dir: DIR, cache, ref });
  return (await git.readCommit({ fs, dir: DIR, cache, oid })).commit.message;
}

async function commitDetail(ref) {
  const oid = await git.resolveRef({ fs, dir: DIR, cache, ref });
  const { commit: detail } = await git.readCommit({ fs, dir: DIR, cache, oid });
  const hasParent = detail.parent.length > 0;
  const patches = [];
  let additions = 0;
  let deletions = 0;
  const trees = hasParent
    ? [git.TREE({ ref: detail.parent[0] }), git.TREE({ ref: oid })]
    : [git.TREE({ ref: oid })];
  await git.walk({
    fs,
    dir: DIR,
    cache,
    trees,
    map: async (path, entries) => {
      if (path === ".") return;
      const [before, after] = hasParent ? entries : [undefined, entries[0]];
      if ((await before?.type()) === "tree" || (await after?.type()) === "tree") return;
      if (
        (await before?.oid()) === (await after?.oid()) &&
        (await before?.mode()) === (await after?.mode())
      )
        return;
      let patch = `diff --git a/${path} b/${path}\n`;
      const left = (await before?.content()) ?? new Uint8Array();
      const right = (await after?.content()) ?? new Uint8Array();
      if (
        left.includes(0) ||
        right.includes(0) ||
        (await before?.type()) === "commit" ||
        (await after?.type()) === "commit"
      ) {
        patch += `Binary files or submodule references differ: ${path}\n`;
      } else {
        const lines = (bytes) => decoder.decode(bytes).match(/[^\n]*\n|[^\n]+$/g) ?? [];
        const oldLines = lines(left);
        const newLines = lines(right);
        let start = 0;
        while (
          start < oldLines.length &&
          start < newLines.length &&
          oldLines[start] === newLines[start]
        )
          start++;
        let oldEnd = oldLines.length;
        let newEnd = newLines.length;
        while (oldEnd > start && newEnd > start && oldLines[oldEnd - 1] === newLines[newEnd - 1]) {
          oldEnd--;
          newEnd--;
        }
        const removed = oldEnd - start;
        const added = newEnd - start;
        additions += added;
        deletions += removed;
        patch += `--- ${before ? `a/${path}` : "/dev/null"}\n+++ ${after ? `b/${path}` : "/dev/null"}\n`;
        if (removed || added) {
          patch += `@@ -${removed ? start + 1 : start},${removed} +${added ? start + 1 : start},${added} @@\n`;
          const marked = (line, mark) =>
            `${mark}${line}${line.endsWith("\n") ? "" : "\n\\ No newline at end of file\n"}`;
          patch += oldLines
            .slice(start, oldEnd)
            .map((line) => marked(line, "-"))
            .join("");
          patch += newLines
            .slice(start, newEnd)
            .map((line) => marked(line, "+"))
            .join("");
        }
      }
      patches.push({ path, patch });
    },
  });
  patches.sort((a, b) => a.path.localeCompare(b.path));
  return {
    commit: {
      oid,
      short_oid: oid.slice(0, 7),
      parents: detail.parent,
      author_name: detail.author.name,
      author_email: detail.author.email,
      authored_unix_seconds: detail.author.timestamp,
      subject: detail.message.split("\n")[0],
    },
    patch: patches.map((entry) => entry.patch).join(""),
    files_changed: patches.length,
    additions,
    deletions,
  };
}

function exclusive(operation, root) {
  const requestedRoot =
    root ?? globalThis.__SYNTAXIS_BROWSER_WORKSPACE_ROOT__ ?? navigator.storage.getDirectory();
  const run = queue.then(async () => {
    operationRoot = await requestedRoot;
    cache = {};
    indexStats.clear();
    forceWorktreeScan = true;
    try {
      return await operation();
    } finally {
      operationRoot = undefined;
      cancellation = undefined;
    }
  });
  queue = run.catch(() => {});
  return run;
}

async function listProjects() {
  const root = await navigator.storage.getDirectory();
  let projects;
  try {
    projects = await root.getDirectoryHandle(PROJECTS);
  } catch {
    return [];
  }
  const result = [];
  for await (const [name, handle] of projects.entries()) {
    if (handle.kind !== "directory") continue;
    try {
      const gitdir = await handle.getDirectoryHandle(".git");
      await gitdir.getFileHandle("syntaxis-complete");
      result.push(name);
    } catch {
      /* Incomplete clones are not registered. */
    }
  }
  return result.sort();
}

function startClone(request) {
  httpsUrl(request.url);
  if (request.mode === "blobless")
    throw new Error("Blobless clones are unavailable in browser Git.");
  const name = request.directory_name;
  if (request.destination_parent !== "/" || !/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,99}$/.test(name ?? "")) {
    throw new Error("Use a destination such as /my-project (one folder, up to 100 characters).");
  }
  const id = crypto.randomUUID();
  const controller = new AbortController();
  const state = { controller, phase: "preparing", percent: null, done: false, error: null, name };
  clones.set(id, state);
  exclusive(async () => {
    cancellation = controller.signal;
    const root = await navigator.storage.getDirectory();
    const projects = await root.getDirectoryHandle(PROJECTS, { create: true });
    for await (const entry of projects.keys()) {
      if (entry === name)
        throw new Error("The destination already exists. Choose a different folder name.");
    }
    controller.signal.throwIfAborted();
    const directory = await projects.getDirectoryHandle(name, { create: true });
    operationRoot = directory;
    try {
      await git.clone({
        fs,
        dir: DIR,
        cache,
        ...network(request.url),
        depth: request.mode === "shallow" ? 1 : undefined,
        singleBranch: request.mode === "shallow",
        nonBlocking: true,
        onProgress: ({ phase, loaded, total }) => {
          controller.signal.throwIfAborted();
          state.phase =
            phase === "Receiving objects"
              ? "receiving"
              : phase === "Resolving deltas"
                ? "resolving"
                : "checking_out";
          state.percent = total ? Math.min(100, Math.floor((loaded / total) * 100)) : null;
        },
      });
      controller.signal.throwIfAborted();
      await git.setConfig({ fs, dir: DIR, path: "http.corsProxy", value: undefined });
      await fs.promises.writeFile("/.git/syntaxis-complete", "1");
    } catch (error) {
      // Only this operation's newly created directory is removed.
      await projects.removeEntry(name, { recursive: true });
      throw error;
    }
  }).then(
    () => {
      state.done = true;
    },
    (error) => {
      state.error = controller.signal.aborted ? "Clone cancelled." : safeError(error);
      state.done = true;
    },
  );
  return id;
}

function safeError(error) {
  let message = error?.message ?? "Browser Git operation failed.";
  for (const connection of connections.values()) {
    if (connection.token) {
      message = message.replaceAll(connection.token, "[redacted]");
      message = message.replaceAll(encodeURIComponent(connection.token), "[redacted]");
      message = message.replaceAll(
        btoa(`${connection.username || "x-access-token"}:${connection.token}`),
        "[redacted]",
      );
    }
  }
  return message;
}

const operations = {
  repository,
  init,
  stage,
  unstage,
  discard,
  commit,
  diff,
  checkout,
  createBranch,
  renameBranch,
  deleteBranch,
  configure,
  saveRemote,
  removeRemote: async (remote) => {
    await git.deleteRemote({ fs, dir: DIR, remote });
    return true;
  },
  check: async (url) => {
    await git.getRemoteInfo({ ...network(url) });
    return true;
  },
  fetchRemote,
  fetch: async () => {
    for (const { remote } of await git.listRemotes({ fs, dir: DIR })) await fetchRemote(remote);
    return { message: "Fetched all remotes." };
  },
  pull,
  push,
  history,
  commitMessage,
  commitDetail,
  listProjects,
};

globalThis.SyntaxisBrowserGit = {
  version: 1,
  ...Object.fromEntries(
    Object.entries(operations).map(([name, operation]) => [
      name,
      (...args) =>
        exclusive(() => operation(...args)).catch((error) => {
          throw new Error(safeError(error));
        }),
    ]),
  ),
  startClone,
  async cloneStatus(id) {
    const state = clones.get(id);
    if (!state) throw new Error("Clone operation not found.");
    if (!state.done) await new Promise((resolve) => setTimeout(resolve, 150));
    const result = {
      done: state.done,
      error: state.error,
      name: state.name,
      cancelled: state.controller.signal.aborted,
      phase: state.phase,
      percent: state.percent,
    };
    return result;
  },
  finishClone(id) {
    if (clones.get(id)?.done) clones.delete(id);
    return true;
  },
  cancelClone(id) {
    clones.get(id)?.controller.abort();
    return true;
  },
};
