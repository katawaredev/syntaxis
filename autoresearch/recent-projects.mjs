#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import puppeteer from "puppeteer-core";

const root = resolve(import.meta.dirname, "..");
const research = join(root, "autoresearch");
const workload = JSON.parse(await readFile(join(research, "workload.json"), "utf8"));
const repetitions = workload.repetitions;
const url = process.env.AUTORESEARCH_HOME_URL ?? workload.homeUrl;
const editorUrl = process.env.AUTORESEARCH_EDITOR_URL ?? workload.editorUrl;
const executablePath = process.env.AUTORESEARCH_BROWSER_PATH ?? "chromium";
const resolvedExecutable = executablePath.includes("/")
  ? executablePath
  : execFileSync("which", [executablePath], { encoding: "utf8" }).trim();
const authorization = `Bearer ${process.env.LHCI_API_TOKEN ?? "syntaxis-local-lighthouse-token-0001"}`;

function summary(values) {
  const sorted = [...values].sort((left, right) => left - right);
  const mean = sorted.reduce((total, value) => total + value, 0) / sorted.length;
  const variance = sorted.reduce((total, value) => total + (value - mean) ** 2, 0) / sorted.length;
  return {
    rawMeasurementsMs: values,
    medianMs: sorted[Math.floor(sorted.length / 2)],
    minMs: sorted[0],
    maxMs: sorted.at(-1),
    rangeMs: sorted.at(-1) - sorted[0],
    varianceMs2: variance,
    standardDeviationMs: Math.sqrt(variance),
  };
}

async function measure(page, targetUrl, ready) {
  await page.goto(targetUrl, { waitUntil: "domcontentloaded" });
  for (const selector of ready.present) await page.waitForSelector(selector);
  for (const selector of ready.absent) {
    await page.waitForFunction((candidate) => !document.querySelector(candidate), {}, selector);
  }
  return page.evaluate(() => ({
    usableMs: performance.now(),
    navigation: performance.getEntriesByType("navigation").map((entry) => ({
      domContentLoadedMs: entry.domContentLoadedEventEnd,
      loadEventMs: entry.loadEventEnd,
      responseStartMs: entry.responseStart,
      responseEndMs: entry.responseEnd,
      transferSize: entry.transferSize,
    })),
    resources: performance
      .getEntriesByType("resource")
      .filter((entry) => entry.name.includes("/api/"))
      .map((entry) => ({
        url: entry.name,
        startTimeMs: entry.startTime,
        responseEndMs: entry.responseEnd,
        durationMs: entry.duration,
        transferSize: entry.transferSize,
      })),
  }));
}

const browser = await puppeteer.launch({ headless: true, executablePath: resolvedExecutable });
const recentValues = [];
const editorValues = [];
const recentProfiles = [];
const editorProfiles = [];
try {
  for (let index = 0; index < repetitions; index += 1) {
    const context = await browser.createBrowserContext();
    const page = await context.newPage();
    await page.setViewport({
      width: workload.viewport.width,
      height: workload.viewport.height,
      deviceScaleFactor: workload.viewport.deviceScaleFactor,
      isMobile: true,
      hasTouch: true,
    });
    await page.setExtraHTTPHeaders({ Authorization: authorization });
    const measurement = await measure(page, url, {
      present: [workload.recentProjectsReady.section],
      absent: [workload.recentProjectsReady.loadingState],
    });
    recentValues.push(measurement.usableMs);
    recentProfiles.push(measurement);
    await context.close();
  }

  if (editorUrl) {
    for (let index = 0; index < repetitions; index += 1) {
      const context = await browser.createBrowserContext();
      const page = await context.newPage();
      await page.setViewport({
        width: workload.viewport.width,
        height: workload.viewport.height,
        deviceScaleFactor: workload.viewport.deviceScaleFactor,
        isMobile: true,
        hasTouch: true,
      });
      await page.setExtraHTTPHeaders({ Authorization: authorization });
      const measurement = await measure(page, editorUrl, {
        present: [workload.editorReady.mount, workload.editorReady.editable],
        absent: [],
      });
      editorValues.push(measurement.usableMs);
      editorProfiles.push(measurement);
      await context.close();
    }
  }
} finally {
  await browser.close();
}

const result = {
  schemaVersion: 2,
  timestamp: new Date().toISOString(),
  browser: execFileSync(resolvedExecutable, ["--version"], { encoding: "utf8" }).trim(),
  url,
  editorUrl,
  viewport: workload.viewport,
  fixture: workload.fixtureDirectory,
  recentProjects: {
    metric: "navigation start to Recent Projects section present and loading state absent",
    selectors: workload.recentProjectsReady,
    profiles: recentProfiles,
    ...summary(recentValues),
  },
  editor: editorValues.length
    ? {
        metric: "navigation start to mounted editable CodeMirror textbox",
        selectors: workload.editorReady,
        profiles: editorProfiles,
        ...summary(editorValues),
      }
    : null,
};
await mkdir(join(research, "results"), { recursive: true });
const output =
  process.env.AUTORESEARCH_RECENT_OUTPUT ?? join(research, "results", `browser-${Date.now()}.json`);
await writeFile(output, `${JSON.stringify(result, null, 2)}\n`);
process.stdout.write(`Wrote ${output}\n`);
