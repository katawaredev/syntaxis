#!/usr/bin/env node

import { readFile, readdir, stat } from "node:fs/promises";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";
import { resolve } from "node:path";

const profile = process.argv[2] ?? "release";
const checkBudget = process.argv.includes("--check");
if (!new Set(["debug", "release"]).has(profile)) {
  throw new Error(`Expected debug or release profile, received ${profile}`);
}
if (process.argv.slice(3).some((argument) => argument !== "--check")) {
  throw new Error("Only the optional --check flag is supported");
}

const root = resolve(import.meta.dirname, "..");
const budgets = checkBudget
  ? JSON.parse(await readFile(resolve(root, "scripts/bundle-budgets.json"), "utf8"))
  : undefined;
const artifacts = [
  {
    app: "server",
    publicDir: resolve(root, `target/dx/syntaxis-server/${profile}/web/public`),
    stem: "syntaxis-server",
  },
  {
    app: "browser",
    publicDir: resolve(root, `target/dx/syntaxis-browser/${profile}/web/public`),
    stem: "syntaxis-browser",
  },
];

const failures = [];
const report = { profile, budgetChecked: checkBudget, artifacts: [] };
for (const artifact of artifacts) {
  const path = await wasmPath(artifact);
  const bytes = await readFile(path);
  const result = {
    app: artifact.app,
    rawBytes: bytes.byteLength,
    gzipBytes: gzipSync(bytes, { level: 9 }).byteLength,
    brotliBytes: brotliCompressSync(bytes, {
      params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
    }).byteLength,
  };
  if (budgets) {
    const limits = budgets.limits[artifact.app];
    if (!limits) throw new Error(`Missing bundle budget for ${artifact.app}`);
    result.limits = limits;
    result.withinBudget = true;
    for (const metric of ["rawBytes", "brotliBytes"]) {
      const withinLimit = result[metric] <= limits[metric];
      if (!withinLimit) {
        failures.push(`${artifact.app} ${metric}: ${result[metric]} exceeds ${limits[metric]}`);
        result.withinBudget = false;
      }
    }
  }
  report.artifacts.push(result);
}

console.log(JSON.stringify(report, null, 2));
if (failures.length > 0) {
  console.error(`Bundle budget exceeded:\n${failures.join("\n")}`);
  process.exitCode = 1;
}

async function wasmPath({ app, publicDir, stem }) {
  const developmentPath = resolve(publicDir, "wasm", `${stem}_bg.wasm`);
  const development = await stat(developmentPath).catch(() => undefined);
  if (development) return developmentPath;

  const assetDir = resolve(publicDir, "assets");
  const names = await readdir(assetDir).catch(() => []);
  const matches = names.filter((name) => name.startsWith(`${stem}_bg-`) && name.endsWith(".wasm"));
  if (matches.length === 1) return resolve(assetDir, matches[0]);

  const html = await readFile(resolve(publicDir, "index.html"), "utf8");
  const escapedStem = stem.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const loaderMatch = html.match(new RegExp(`assets/${escapedStem}-[^"']+\\.js`));
  if (loaderMatch) {
    const loader = await readFile(resolve(publicDir, loaderMatch[0]), "utf8");
    const wasmMatch = loader.match(new RegExp(`assets/${escapedStem}_bg-[^"']+\\.wasm`));
    if (wasmMatch) return resolve(publicDir, wasmMatch[0]);
  }

  if (matches.length === 0) {
    throw new Error(`Could not find a ${app} ${profile} WASM artifact in ${assetDir}`);
  }
  throw new Error(
    `Could not identify the active ${app} ${profile} WASM among ${matches.length} files`,
  );
}
