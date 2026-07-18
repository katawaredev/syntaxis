#!/usr/bin/env node

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = resolve(root, "package.json");
const lockfile = resolve(root, "bun.lock");
const stamp = resolve(root, "node_modules/.syntaxis-dependencies.stamp");
const cacheKey = createHash("sha256")
  .update(readFileSync(manifest))
  .update(readFileSync(lockfile))
  .digest("hex");

if (existsSync(stamp) && readFileSync(stamp, "utf8").trim() === cacheKey) {
  console.log(`JavaScript dependencies are current (${cacheKey.slice(0, 12)})`);
  process.exit(0);
}

const install = spawnSync(process.execPath, ["install", "--frozen-lockfile"], {
  cwd: root,
  stdio: "inherit",
});
if (install.status !== 0) process.exit(install.status ?? 1);
writeFileSync(stamp, `${cacheKey}\n`);
console.log(`Installed JavaScript dependencies (${cacheKey.slice(0, 12)})`);
