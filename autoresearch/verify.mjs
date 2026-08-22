#!/usr/bin/env node

import { spawn } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const commands = [
  ["mise", ["run", "check"]],
  ["mise", ["run", "check:server"]],
];

function run(executable, args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(executable, args, { cwd: root, stdio: "inherit", shell: false });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`${executable} exited with ${code ?? `signal ${signal}`}`));
    });
  });
}

for (const [executable, args] of commands) await run(executable, args);
