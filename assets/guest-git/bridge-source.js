import * as git from "isomorphic-git";
import http from "isomorphic-git/http/web";
import { Buffer } from "buffer";

globalThis.Buffer ??= Buffer;

const DIR = "/";
const GITDIR = "/.git";
const LEGACY_HISTORY = ".syntaxis-guest-history.json";
const encoder = new TextEncoder();
const decoder = new TextDecoder();
const cache = {};
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
  return String(path)
    .replaceAll("\\", "/")
    .split("/")
    .filter((part) => part && part !== ".");
}

async function workspaceRoot() {
  return globalThis.__SYNTAXIS_GUEST_WORKSPACE_ROOT__ ?? navigator.storage.getDirectory();
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
        if (error?.code) throw error;
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
    git.statusMatrix({ fs, dir: DIR, cache, filter: (path) => path !== LEGACY_HISTORY }),
    git.currentBranch({ fs, dir: DIR, cache, fullname: false }),
    git.listBranches({ fs, dir: DIR, cache }),
    git.listRemotes({ fs, dir: DIR, cache }),
    git.getConfig({ fs, dir: DIR, cache, path: "user.name" }),
    git.getConfig({ fs, dir: DIR, cache, path: "user.email" }),
  ]);
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
    changes: matrix.map(mapStatus).filter((change) => change.staged || change.unstaged),
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

async function commit({ message, name, email }) {
  await git.setConfig({ fs, dir: DIR, cache, path: "user.name", value: name });
  await git.setConfig({ fs, dir: DIR, cache, path: "user.email", value: email });
  const oid = await git.commit({
    fs,
    dir: DIR,
    cache,
    message,
    author: { name: name || "Syntaxis Guest", email: email || "guest@syntaxis.local" },
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
      return null;
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

async function deleteBranch(ref) {
  await git.deleteBranch({ fs, dir: DIR, cache, ref });
  indexStats.clear();
  return repository();
}

async function sync({ action, url, corsProxy, username, password }) {
  if (url) await git.addRemote({ fs, dir: DIR, cache, remote: "origin", url, force: true });
  const options = {
    fs,
    http,
    dir: DIR,
    cache,
    remote: "origin",
    corsProxy: corsProxy || undefined,
    onAuth: username || password ? () => ({ username, password }) : undefined,
  };
  if (action === "fetch") await git.fetch(options);
  else if (action === "pull") {
    await git.pull({
      ...options,
      author: { name: "Syntaxis Guest", email: "guest@syntaxis.local" },
      fastForwardOnly: true,
    });
  } else if (action === "push" || action === "publish") {
    await git.push(options);
    if (action === "publish") {
      const branch = await git.currentBranch({ fs, dir: DIR, cache, fullname: false });
      if (branch) {
        await git.setConfig({
          fs,
          dir: DIR,
          cache,
          path: `branch.${branch}.remote`,
          value: "origin",
        });
        await git.setConfig({
          fs,
          dir: DIR,
          cache,
          path: `branch.${branch}.merge`,
          value: `refs/heads/${branch}`,
        });
      }
    }
  } else throw new Error(`Unsupported remote operation: ${action}`);
  indexStats.clear();
  return repository();
}

globalThis.SyntaxisGuestGit = {
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
  sync,
};
