#!/usr/bin/env node

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(root, "assets/code-editor/bridge-source.js");
const destination = resolve(root, "crates/code-editor/assets/editor.bundle.js");
const lspSource = resolve(root, "assets/code-editor/lsp-source.js");
const lspDestination = resolve(root, "crates/code-editor/assets/lsp.bundle.js");
const stamp = resolve(root, "crates/code-editor/assets/editor.bundle.stamp");
const manifest = resolve(root, "package.json");
const lockfile = resolve(root, "bun.lock");
const script = fileURLToPath(import.meta.url);
const cacheKey = createHash("sha256")
  .update("syntaxis-code-editor-v2\0")
  .update(readFileSync(script))
  .update(readFileSync(source))
  .update(readFileSync(lspSource))
  .update(readFileSync(manifest))
  .update(readFileSync(lockfile))
  .digest("hex");

if (
  existsSync(destination) &&
  existsSync(lspDestination) &&
  existsSync(stamp) &&
  readFileSync(stamp, "utf8").trim() === cacheKey
) {
  console.log(`Code editor bundle is current (${cacheKey.slice(0, 12)})`);
  process.exit(0);
}

const { default: esbuild } = await import("esbuild");
const result = await esbuild.build({
  entryPoints: [source],
  bundle: true,
  format: "esm",
  outfile: destination,
  platform: "browser",
  target: "es2020",
  minify: true,
  sourcemap: false,
  logLevel: "info",
});

if (result.errors.length > 0) process.exit(1);

const lspResult = await esbuild.build({
  entryPoints: [lspSource],
  bundle: true,
  format: "esm",
  outfile: lspDestination,
  platform: "browser",
  target: "es2020",
  minify: true,
  sourcemap: false,
  logLevel: "info",
});

if (lspResult.errors.length > 0) process.exit(1);

writeFileSync(stamp, `${cacheKey}\n`);
console.log(`Built CodeMirror editor bundle (${cacheKey.slice(0, 12)})`);
