#!/usr/bin/env node

import { mkdir, readdir, readFile, stat, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { execFileSync, spawn } from "node:child_process";

const root = resolve(import.meta.dirname, "..");
const research = join(root, "autoresearch");
const reports = join(root, "lighthouse-reports");
const publicDir = join(root, "target/dx/syntaxis/release/web/public");
const outputDir = join(research, "results");

async function loadWorkload() {
  try {
    return JSON.parse(await readFile(join(research, "workload.json"), "utf8"));
  } catch (error) {
    throw new Error("Could not read autoresearch/workload.json", { cause: error });
  }
}

const workload = await loadWorkload();
const repetitions = workload.repetitions;

function toolVersion(executable, args) {
  try {
    return execFileSync(executable, args, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "unavailable";
  }
}

function command(executable, args, environment = process.env) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(executable, args, {
      cwd: root,
      stdio: "inherit",
      shell: false,
      env: environment,
    });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`${executable} exited with ${code ?? `signal ${signal}`}`));
    });
  });
}

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await filesUnder(path)));
    else files.push(path);
  }
  return files;
}

async function latestReports() {
  const entries = (await readdir(reports, { withFileTypes: true }))
    .filter((entry) => entry.isFile() && entry.name.endsWith(".json"));
  const withTimes = await Promise.all(entries.map(async (entry) => {
    const path = join(reports, entry.name);
    const metadata = await stat(path);
    return { path, time: metadata.mtimeMs };
  }));
  return withTimes.sort((a, b) => b.time - a.time).slice(0, repetitions);
}

function audit(report, id) {
  const value = report.audits?.[id]?.numericValue;
  return typeof value === "number" ? value : null;
}

function summary(values) {
  const usable = values.filter((value) => typeof value === "number").sort((a, b) => a - b);
  if (!usable.length) return { median: null, min: null, max: null, range: null, values: [] };
  return {
    median: usable[Math.floor(usable.length / 2)],
    min: usable[0],
    max: usable[usable.length - 1],
    range: usable[usable.length - 1] - usable[0],
    values: usable,
  };
}

async function assetSizes() {
  const files = await filesUnder(publicDir);
  const sizes = await Promise.all(files.map(async (path) => {
    const metadata = await stat(path);
    return { path: path.slice(publicDir.length + 1), bytes: metadata.size };
  }));
  sizes.sort((a, b) => b.bytes - a.bytes);
  return { totalBytes: sizes.reduce((total, file) => total + file.bytes, 0), files: sizes };
}

await command("bun", ["run", "lighthouse"]);
const recentProjectsOutput = join(outputDir, "recent-projects-latest.json");
let recentProjectsMeasurement = null;
if (process.env.AUTORESEARCH_MEASURE_RECENT_PROJECTS === "true") {
  const environment = { ...process.env, AUTORESEARCH_RECENT_OUTPUT: recentProjectsOutput };
  await mkdir(outputDir, { recursive: true });
  await command("bun", ["run", "autoresearch:recent-projects"], environment);
  try {
    recentProjectsMeasurement = JSON.parse(await readFile(recentProjectsOutput, "utf8"));
  } catch (error) {
    throw new Error("Could not read the Recent Projects measurement", { cause: error });
  }
}
const reportFiles = await latestReports();
if (reportFiles.length < repetitions) throw new Error("LHCI produced fewer reports than expected");
const reportsData = await Promise.all(reportFiles.map(({ path }) => readFile(path, "utf8").then(JSON.parse)));
const measurements = {
  firstContentfulPaintMs: summary(reportsData.map((report) => audit(report, "first-contentful-paint"))),
  largestContentfulPaintMs: summary(reportsData.map((report) => audit(report, "largest-contentful-paint"))),
  totalBlockingTimeMs: summary(reportsData.map((report) => audit(report, "total-blocking-time"))),
  initialPageLoadMs: summary(reportsData.map((report) => audit(report, "load"))),
  // Lighthouse's stable, browser-observed proxy for initial application usability.
  applicationUiUsableMs: summary(reportsData.map((report) => audit(report, "interactive"))),
  recentProjectsUsableMs: recentProjectsMeasurement
    ? {
        median: recentProjectsMeasurement.medianMs,
        min: recentProjectsMeasurement.minMs,
        max: recentProjectsMeasurement.maxMs,
        range: recentProjectsMeasurement.rangeMs,
        values: recentProjectsMeasurement.rawMeasurementsMs,
        status: "measured-with-playwright",
      }
    : {
        median: null,
        min: null,
        max: null,
        range: null,
        values: [],
        status: "pending-route-specific-browser-step",
      },
};
const editorUrl = process.env.AUTORESEARCH_EDITOR_URL ?? workload.editorUrl;
const result = {
  schemaVersion: 1,
  timestamp: new Date().toISOString(),
  environment: {
    node: process.version,
    bun: toolVersion("bun", ["--version"]),
    dioxus: toolVersion("dx", ["--version"]),
    lighthouse: toolVersion("lhci", ["--version"]),
    rust: toolVersion("rustc", ["--version"]),
    lighthouseConfig: "lighthouserc.json",
    repetitions,
  },
  inputs: {
    viewport: workload.viewport,
    fixture: workload.fixtureDirectory,
    homeUrl: workload.homeUrl,
    editorUrl,
    recentProjectsReady: workload.recentProjectsReady,
    usabilityMetric: "Lighthouse interactive audit; application-specific proxy until route-specific browser instrumentation is available",
    recentProjectsMetric: "Measure navigation start to section[aria-labelledby=recent-title] present with [aria-label=Loading recent projects] absent"
  },
  rawMeasurements: reportsData.map((report, index) => ({
    fetchTime: report.fetchTime,
    requestedUrl: report.finalUrl,
    firstContentfulPaintMs: audit(report, "first-contentful-paint"),
    largestContentfulPaintMs: audit(report, "largest-contentful-paint"),
    totalBlockingTimeMs: audit(report, "total-blocking-time"),
    initialPageLoadMs: audit(report, "load"),
    applicationUiUsableMs: audit(report, "interactive"),
    recentProjectsUsableMs: recentProjectsMeasurement?.rawMeasurementsMs[index] ?? null,
  })),
  recentProjects: {
    status: recentProjectsMeasurement ? "measured-with-playwright" : "pending-route-specific-browser-step",
    selectors: workload.recentProjectsReady,
  },
  medianMeasurements: measurements,
  buildAssetSizes: await assetSizes(),
};
await mkdir(outputDir, { recursive: true });
const isBaseline = !(await stat(join(research, "baseline.json")).catch(() => null));
const destination = isBaseline ? join(research, "baseline.json") : join(outputDir, `run-${Date.now()}.json`);
await writeFile(destination, `${JSON.stringify(result, null, 2)}\n`);
process.stdout.write(`Wrote ${destination}\n`);
