#!/usr/bin/env node

import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readdir, readFile, stat, writeFile } from "node:fs/promises";
import { cpus, freemem, hostname, platform, release, totalmem } from "node:os";
import { join, resolve } from "node:path";

import { summary } from "./stats.mjs";

const root = resolve(import.meta.dirname, "..");
const research = join(root, "autoresearch");
const reports = join(root, "lighthouse-reports");
const publicDir = join(root, "target/dx/syntaxis/release/web/public");
const outputDir = join(research, "results");
const profilesDir = join(research, "profiles");
const workload = JSON.parse(await readFile(join(research, "workload.json"), "utf8"));
const lighthouseConfig = JSON.parse(await readFile(join(root, "lighthouserc.json"), "utf8"));
const repetitions = workload.repetitions;

function assertHarnessConfiguration() {
  const collect = lighthouseConfig.ci?.collect;
  const screen = collect?.settings?.screenEmulation;
  const mismatches = [];
  if (collect?.numberOfRuns !== repetitions) {
    mismatches.push(
      `Lighthouse runs ${collect?.numberOfRuns ?? "unset"}; workload requires ${repetitions}`,
    );
  }
  for (const key of ["width", "height", "deviceScaleFactor"]) {
    if (screen?.[key] !== workload.viewport[key]) {
      mismatches.push(
        `Lighthouse ${key} is ${screen?.[key] ?? "unset"}; workload requires ${workload.viewport[key]}`,
      );
    }
  }
  if (collect?.settings?.formFactor !== workload.viewport.formFactor) {
    mismatches.push(
      `Lighthouse formFactor is ${collect?.settings?.formFactor ?? "unset"}; workload requires ${workload.viewport.formFactor}`,
    );
  }
  if (mismatches.length)
    throw new Error(`Benchmark configuration drift:\n${mismatches.join("\n")}`);
}

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

async function withBenchmarkServer(action) {
  const child = spawn("bash", ["scripts/lighthouse-server.sh"], {
    cwd: root,
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  let readySettled = false;
  const exit = new Promise((resolvePromise) => child.once("exit", resolvePromise));
  const ready = new Promise((resolvePromise, reject) => {
    const inspect = (chunk) => {
      const text = chunk.toString();
      output = `${output}${text}`.slice(-64 * 1024);
      process.stdout.write(text);
      if (output.includes("Lighthouse server ready")) {
        readySettled = true;
        resolvePromise();
      }
    };
    child.stdout.on("data", inspect);
    child.stderr.on("data", inspect);
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (!readySettled) {
        reject(
          new Error(
            `benchmark server exited before measurement with ${code ?? `signal ${signal}`}\n${output}`,
          ),
        );
      }
    });
  });
  let timeoutId;
  const timeout = new Promise((_, reject) => {
    timeoutId = setTimeout(
      () => reject(new Error(`Benchmark server was not ready within 30 seconds\n${output}`)),
      30_000,
    );
  });
  await Promise.race([ready, timeout]);
  clearTimeout(timeoutId);
  try {
    return await action();
  } finally {
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGTERM");
    await exit;
  }
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

async function reportNames() {
  return new Set(
    (await readdir(reports, { withFileTypes: true }).catch(() => []))
      .filter((entry) => entry.isFile() && entry.name.endsWith(".report.json"))
      .map((entry) => entry.name),
  );
}

function audit(report, id) {
  const value = report.audits?.[id]?.numericValue;
  return typeof value === "number" ? value : null;
}

function observedMetric(report, id) {
  const value = report.audits?.metrics?.details?.items?.[0]?.[id];
  return typeof value === "number" ? value : null;
}

function browserSummary(measurement) {
  if (!measurement) return null;
  return {
    median: measurement.medianMs,
    min: measurement.minMs,
    max: measurement.maxMs,
    range: measurement.rangeMs,
    p25: measurement.p25Ms,
    p75: measurement.p75Ms,
    p95: measurement.p95Ms,
    variance: measurement.varianceMs2,
    standardDeviation: measurement.standardDeviationMs,
    medianAbsoluteDeviation: measurement.medianAbsoluteDeviationMs,
    values: measurement.rawMeasurementsMs,
    status: "measured-with-isolated-puppeteer-processes",
  };
}

function gitSourceState() {
  const status = toolVersion("git", ["status", "--porcelain=v1", "--untracked-files=all"]);
  const trackedDiff = execFileSync("git", ["diff", "--binary", "HEAD"], {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return {
    dirty: status.length > 0,
    status: status ? status.split("\n") : [],
    trackedDiffSha256: createHash("sha256").update(trackedDiff).digest("hex"),
  };
}

async function assetSizes() {
  const files = (await filesUnder(publicDir)).filter(
    (path) => !path.endsWith(".br") && !path.endsWith(".gz"),
  );
  const sizes = await Promise.all(
    files.map(async (path) => {
      const [metadata, gzipMetadata, brotliMetadata] = await Promise.all([
        stat(path),
        stat(`${path}.gz`).catch(() => null),
        stat(`${path}.br`).catch(() => null),
      ]);
      return {
        path: path.slice(publicDir.length + 1),
        bytes: metadata.size,
        gzipBytes: gzipMetadata?.size ?? null,
        brotliBytes: brotliMetadata?.size ?? null,
      };
    }),
  );
  sizes.sort((a, b) => b.bytes - a.bytes);
  return {
    totalBytes: sizes.reduce((total, file) => total + file.bytes, 0),
    totalGzipBytes: sizes.reduce((total, file) => total + (file.gzipBytes ?? file.bytes), 0),
    totalBrotliBytes: sizes.reduce((total, file) => total + (file.brotliBytes ?? file.bytes), 0),
    files: sizes,
  };
}

function profileReport(report) {
  const requests = report.audits?.["network-requests"]?.details?.items ?? [];
  const bootup = report.audits?.["bootup-time"]?.details?.items ?? [];
  const mainThread = report.audits?.["mainthread-work-breakdown"]?.details?.items ?? [];
  return {
    fetchTime: report.fetchTime,
    userAgent: report.userAgent,
    configSettings: report.configSettings,
    document: requests
      .filter((item) => item.resourceType === "Document")
      .map(
        ({ url, networkRequestTime, networkEndTime, transferSize, resourceSize, statusCode }) => ({
          url,
          networkRequestTime,
          networkEndTime,
          transferSize,
          resourceSize,
          statusCode,
        }),
      ),
    criticalResources: requests
      .filter((item) =>
        ["Document", "Script", "Stylesheet", "Font", "Other"].includes(item.resourceType),
      )
      .map(
        ({
          url,
          resourceType,
          networkRequestTime,
          networkEndTime,
          transferSize,
          resourceSize,
          statusCode,
        }) => ({
          url,
          resourceType,
          networkRequestTime,
          networkEndTime,
          transferSize,
          resourceSize,
          statusCode,
        }),
      ),
    resourceSummary: report.audits?.["resource-summary"]?.details?.items ?? [],
    bootupTime: bootup
      .map(({ url, scripting, scriptParseCompile, total }) => ({
        url,
        scripting,
        scriptParseCompile,
        total,
      }))
      .sort((left, right) => (right.total ?? 0) - (left.total ?? 0)),
    mainThreadWork: mainThread,
  };
}

assertHarnessConfiguration();
await mkdir(outputDir, { recursive: true });
await mkdir(profilesDir, { recursive: true });
const reportsBefore = await reportNames();
let lighthouseAssertionsPassed = true;
try {
  if (process.env.AUTORESEARCH_SKIP_BUILD === "true") {
    await command(join(root, "node_modules/.bin/lhci"), ["autorun"]);
  } else {
    await command("bun", ["run", "lighthouse"]);
  }
} catch (error) {
  lighthouseAssertionsPassed = false;
  process.stderr.write(
    `Lighthouse command reported a failure; collecting fresh reports: ${error.message}\n`,
  );
}
const reportsAfter = await reportNames();
const reportFiles = [...reportsAfter]
  .filter((name) => !reportsBefore.has(name))
  .map((name) => join(reports, name));
if (reportFiles.length !== repetitions) {
  throw new Error(
    `LHCI produced ${reportFiles.length} fresh JSON reports; expected ${repetitions}`,
  );
}
const reportsData = await Promise.all(
  reportFiles.map((path) => readFile(path, "utf8").then(JSON.parse)),
);

const browserOutput = join(outputDir, `browser-${Date.now()}.json`);
await withBenchmarkServer(async () => {
  const environment = { ...process.env, AUTORESEARCH_RECENT_OUTPUT: browserOutput };
  await command("bun", ["run", "autoresearch:recent-projects"], environment);
});
const browserMeasurement = JSON.parse(await readFile(browserOutput, "utf8"));
const editorUrl = process.env.AUTORESEARCH_EDITOR_URL ?? workload.editorUrl;
const measurements = {
  firstContentfulPaintMs: summary(
    reportsData.map((report) => audit(report, "first-contentful-paint")),
  ),
  largestContentfulPaintMs: summary(
    reportsData.map((report) => audit(report, "largest-contentful-paint")),
  ),
  totalBlockingTimeMs: summary(reportsData.map((report) => audit(report, "total-blocking-time"))),
  initialPageLoadMs: summary(reportsData.map((report) => observedMetric(report, "observedLoad"))),
  applicationUiUsableMs: summary(reportsData.map((report) => audit(report, "interactive"))),
  recentProjectsUsableMs: browserSummary(browserMeasurement.recentProjects),
  homeInteractionResponseMs: browserSummary(browserMeasurement.homeInteraction),
  homeTaskCompletionMs: browserSummary(browserMeasurement.homeTask),
  editorUsableMs: browserSummary(browserMeasurement.editor),
};
const hardware = cpus();
const timestamp = new Date().toISOString();
const baselinePath = join(research, "baseline.json");
const isBaseline = !(await stat(baselinePath).catch(() => null));
const runId = `${isBaseline ? "baseline" : "run"}-${timestamp.replaceAll(/[:.]/g, "-")}`;
const result = {
  schemaVersion: 3,
  timestamp,
  label: process.env.AUTORESEARCH_LABEL ?? null,
  commit: toolVersion("git", ["rev-parse", "HEAD"]),
  sourceState: gitSourceState(),
  correctness: {
    lighthouseAssertionsPassed,
    browserIssuesPassed: browserMeasurement.pageIssues.every(
      (issues) =>
        issues.consoleErrors.length === 0 &&
        issues.pageErrors.length === 0 &&
        issues.requestFailures.length === 0,
    ),
    responsiveLayoutPassed: browserMeasurement.responsive.every(
      (audit) => !audit.layout.horizontalOverflow && audit.layout.rootRect !== null,
    ),
    gitDiffPassed:
      browserMeasurement.gitDiff.markerPresent &&
      browserMeasurement.gitDiff.syntaxCommentCount > 0 &&
      !browserMeasurement.gitDiff.layout.horizontalOverflow,
  },
  environment: {
    hostname: hostname(),
    platform: platform(),
    platformRelease: release(),
    architecture: process.arch,
    cpuModel: hardware[0]?.model ?? "unknown",
    logicalCpuCount: hardware.length,
    totalMemoryBytes: totalmem(),
    freeMemoryBytesAtCollection: freemem(),
    node: process.version,
    bun: toolVersion("bun", ["--version"]),
    dioxus: toolVersion("dx", ["--version"]),
    lighthouse: toolVersion(join(root, "node_modules/.bin/lhci"), ["--version"]),
    rust: toolVersion("rustc", ["--version"]),
    browser: browserMeasurement.browser,
    lighthouseConfig: "lighthouserc.json",
    repetitions,
    browserIsolation: browserMeasurement.browserIsolation,
    releaseBuildSkipped: process.env.AUTORESEARCH_SKIP_BUILD === "true",
  },
  inputs: {
    workload,
    viewport: workload.viewport,
    fixture: workload.fixtureDirectory,
    homeUrl: workload.homeUrl,
    editorUrl,
    recentProjectsReady: workload.recentProjectsReady,
    editorReady: workload.editorReady,
    homeInteraction: workload.homeInteraction,
    responsiveViewports: workload.responsiveViewports,
    lighthouseSettings: reportsData[0]?.configSettings ?? null,
    usabilityMetric: "Lighthouse interactive audit",
    recentProjectsMetric: browserMeasurement.recentProjects.metric,
    homeInteractionMetric: browserMeasurement.homeInteraction.metric,
    homeTaskMetric: browserMeasurement.homeTask.metric,
    editorMetric: browserMeasurement.editor?.metric ?? null,
  },
  rawMeasurements: reportsData.map((report) => ({
    fetchTime: report.fetchTime,
    requestedUrl: report.finalUrl,
    firstContentfulPaintMs: audit(report, "first-contentful-paint"),
    largestContentfulPaintMs: audit(report, "largest-contentful-paint"),
    totalBlockingTimeMs: audit(report, "total-blocking-time"),
    initialPageLoadMs: observedMetric(report, "observedLoad"),
    applicationUiUsableMs: audit(report, "interactive"),
  })),
  browserRawMeasurements: {
    recentProjectsUsableMs: browserMeasurement.recentProjects.rawMeasurementsMs,
    homeInteractionResponseMs: browserMeasurement.homeInteraction.rawMeasurementsMs,
    homeTaskCompletionMs: browserMeasurement.homeTask.rawMeasurementsMs,
    editorUsableMs: browserMeasurement.editor?.rawMeasurementsMs ?? [],
    pageIssues: browserMeasurement.pageIssues,
    responsive: browserMeasurement.responsive,
  },
  medianMeasurements: measurements,
  buildAssetSizes: await assetSizes(),
};
const destination = isBaseline ? baselinePath : join(outputDir, `${runId}.json`);
const profileDestination = join(profilesDir, `${runId}.json`);
await writeFile(destination, `${JSON.stringify(result, null, 2)}\n`);
await writeFile(
  profileDestination,
  `${JSON.stringify(
    {
      schemaVersion: 2,
      timestamp,
      commit: result.commit,
      workload: result.inputs,
      environment: result.environment,
      lighthouseRuns: reportsData.map(profileReport),
      browserRuns: {
        recentProjects: browserMeasurement.recentProjects.profiles,
        editor: browserMeasurement.editor?.profiles ?? [],
        responsive: browserMeasurement.responsive,
        gitDiff: browserMeasurement.gitDiff,
        pageIssues: browserMeasurement.pageIssues,
      },
    },
    null,
    2,
  )}\n`,
);
process.stdout.write(`Wrote ${destination}\n`);
process.stdout.write(`Wrote ${profileDestination}\n`);
