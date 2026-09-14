#!/usr/bin/env node

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(root, "crates/runtime-browser/bridge-src/archive/bridge-source.js");
const outputDir = resolve(root, "crates/runtime-browser/assets");
const destination = resolve(outputDir, "browser-archive.bundle.js");
const stamp = resolve(outputDir, "browser-archive.bundle.stamp");
const manifest = resolve(root, "package.json");
const lockfile = resolve(root, "bun.lock");
const script = fileURLToPath(import.meta.url);
const cacheKey = createHash("sha256")
  .update("syntaxis-browser-archive-v1\0")
  .update(readFileSync(script))
  .update(readFileSync(source))
  .update(readFileSync(manifest))
  .update(readFileSync(lockfile))
  .digest("hex");

if (
  existsSync(destination) &&
  existsSync(stamp) &&
  readFileSync(stamp, "utf8").trim() === cacheKey
) {
  console.log(`Browser archive bundle is current (${cacheKey.slice(0, 12)})`);
  process.exit(0);
}

const { default: esbuild } = await import("esbuild");
mkdirSync(outputDir, { recursive: true });
const result = await esbuild.build({
  entryPoints: [source],
  bundle: true,
  format: "iife",
  outfile: destination,
  platform: "browser",
  target: "es2020",
  minify: true,
  sourcemap: false,
  logLevel: "info",
});

if (result.errors.length > 0) process.exit(1);

writeFileSync(stamp, `${cacheKey}\n`);
console.log(`Built browser archive bundle (${cacheKey.slice(0, 12)})`);
