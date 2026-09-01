import { Bash } from "just-bash/browser";

const ROOT = "/workspace";
let shell = null;
let activeController = null;

function workspacePath(path) {
  return path ? `${ROOT}/${path}` : ROOT;
}

async function initialize(snapshot) {
  const files = {};
  for (const file of snapshot.files) {
    files[workspacePath(file.path)] = new Uint8Array(file.content);
  }
  shell = new Bash({
    files,
    cwd: "/",
    executionLimitProfile: "hardened",
    executionLimits: {
      maxExecutionTimeMs: 10_000,
      maxFileSystemBytes: 32 * 1024 * 1024,
      maxOutputSize: 512 * 1024,
    },
  });
  await shell.fs.mkdir(ROOT, { recursive: true });
  for (const directory of snapshot.directories) {
    await shell.fs.mkdir(workspacePath(directory), { recursive: true });
  }
}

async function collectSnapshot() {
  const directories = [];
  const files = [];
  const pending = [ROOT];
  while (pending.length > 0) {
    const directory = pending.pop();
    const names = await shell.fs.readdir(directory);
    for (const name of names) {
      const path = `${directory}/${name}`;
      const relative = path.slice(ROOT.length + 1);
      const metadata = await shell.fs.lstat(path);
      if (metadata.isDirectory) {
        directories.push(relative);
        pending.push(path);
      } else if (metadata.isFile) {
        const bytes = await shell.fs.readFileBuffer(path);
        files.push({ path: relative, content: Array.from(bytes) });
      }
    }
  }
  return { directories, files };
}

async function execute(command, snapshot) {
  await initialize(snapshot);
  const controller = new AbortController();
  activeController = controller;
  try {
    const result = await shell.exec(command, { cwd: ROOT, signal: controller.signal });
    return {
      stdout: result.stdout,
      stderr: result.stderr,
      exitCode: result.exitCode,
      snapshot: await collectSnapshot(),
    };
  } catch (error) {
    if (controller.signal.aborted) {
      return {
        stdout: "",
        stderr: "Command cancelled.\n",
        exitCode: 130,
        snapshot,
      };
    }
    throw error;
  } finally {
    if (activeController === controller) activeController = null;
  }
}

function cancel() {
  activeController?.abort();
}

globalThis.SyntaxisGuestBash = { execute, cancel };
