#!/usr/bin/env node

import { readFile } from "node:fs/promises";

const [beforePath, afterPath] = process.argv.slice(2);
if (!beforePath || !afterPath) {
  console.error("Usage: bun autoresearch/compare.mjs BEFORE.json AFTER.json");
  process.exit(2);
}

const [before, after] = await Promise.all([
  readFile(beforePath, "utf8").then(JSON.parse),
  readFile(afterPath, "utf8").then(JSON.parse),
]);

function median(result, key) {
  return result.medianMeasurements?.[key]?.median ?? null;
}

function comparison(key) {
  const baseline = median(before, key);
  const candidate = median(after, key);
  if (baseline === null || candidate === null || baseline === 0) {
    return { baseline, candidate, delta: null, percentChange: null };
  }
  return {
    baseline,
    candidate,
    delta: candidate - baseline,
    percentChange: ((candidate - baseline) / baseline) * 100,
  };
}

const metrics = [
  "initialPageLoadMs",
  "firstContentfulPaintMs",
  "largestContentfulPaintMs",
  "totalBlockingTimeMs",
  "applicationUiUsableMs",
  "recentProjectsUsableMs",
];
process.stdout.write(`${JSON.stringify({
  baseline: before.timestamp,
  candidate: after.timestamp,
  sameWorkload: JSON.stringify(before.inputs?.viewport) === JSON.stringify(after.inputs?.viewport)
    && before.inputs?.fixture === after.inputs?.fixture,
  sameToolchain: JSON.stringify(before.environment) === JSON.stringify(after.environment),
  metrics: Object.fromEntries(metrics.map((key) => [key, comparison(key)])),
}, null, 2)}\n`);
