#!/usr/bin/env node

import { readFile } from "node:fs/promises";

import { summary } from "./stats.mjs";

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
  const measured = result.medianMeasurements?.[key]?.median;
  if (measured !== undefined && measured !== null) return measured;
  if (key !== "homeTaskCompletionMs") return null;

  const recent = result.browserRawMeasurements?.recentProjectsUsableMs ?? [];
  const interaction = result.browserRawMeasurements?.homeInteractionResponseMs ?? [];
  if (recent.length === 0 || recent.length !== interaction.length) return null;
  return summary(recent.map((value, index) => value + interaction[index])).median;
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
  "homeInteractionResponseMs",
  "homeTaskCompletionMs",
  "editorUsableMs",
];

function asset(result, key) {
  if (key === "initialWasmBytes" || key === "initialWasmBrotliBytes") {
    const wasm = result.buildAssetSizes?.files?.find((file) => file.path.endsWith(".wasm"));
    return key === "initialWasmBytes" ? (wasm?.bytes ?? null) : (wasm?.brotliBytes ?? null);
  }
  return result.buildAssetSizes?.[key] ?? null;
}

function assetComparison(key) {
  const baseline = asset(before, key);
  const candidate = asset(after, key);
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

const assetMetrics = [
  "initialWasmBytes",
  "initialWasmBrotliBytes",
  "totalBytes",
  "totalBrotliBytes",
];

const stableEnvironmentKeys = [
  "hostname",
  "platform",
  "platformRelease",
  "architecture",
  "cpuModel",
  "logicalCpuCount",
  "totalMemoryBytes",
  "node",
  "bun",
  "dioxus",
  "lighthouse",
  "rust",
  "browser",
  "lighthouseConfig",
  "repetitions",
  "browserIsolation",
];

function stableEnvironment(result) {
  return Object.fromEntries(
    stableEnvironmentKeys.map((key) => [key, result.environment?.[key] ?? null]),
  );
}

process.stdout.write(
  `${JSON.stringify(
    {
      baseline: before.timestamp,
      candidate: after.timestamp,
      compatibleSchema: before.schemaVersion === after.schemaVersion,
      sameWorkload:
        JSON.stringify(before.inputs?.workload) === JSON.stringify(after.inputs?.workload),
      sameToolchain:
        JSON.stringify(stableEnvironment(before)) === JSON.stringify(stableEnvironment(after)),
      correctness: {
        baseline: before.correctness ?? null,
        candidate: after.correctness ?? null,
      },
      metrics: Object.fromEntries(metrics.map((key) => [key, comparison(key)])),
      assets: Object.fromEntries(assetMetrics.map((key) => [key, assetComparison(key)])),
    },
    null,
    2,
  )}\n`,
);
