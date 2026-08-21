#!/usr/bin/env node

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const research = join(root, "autoresearch");

async function loadWorkload() {
  try {
    return JSON.parse(await readFile(join(research, "workload.json"), "utf8"));
  } catch (error) {
    throw new Error("Could not read autoresearch/workload.json", { cause: error });
  }
}

const workload = await loadWorkload();
const repetitions = workload.repetitions;
const url = process.env.AUTORESEARCH_HOME_URL ?? workload.homeUrl;

let playwright;
try {
  playwright = await import("playwright");
} catch (error) {
  throw new Error(
    "Recent Projects measurement requires the optional Playwright package; install it on the benchmark machine before running this command",
    { cause: error },
  );
}

const browser = await playwright.chromium.launch({ headless: true });
const values = [];
try {
  for (let index = 0; index < repetitions; index += 1) {
    const context = await browser.newContext({ viewport: workload.viewport });
    const page = await context.newPage();
    await page.addInitScript(() => {
      window.__syntaxisBenchmarkStart = performance.now();
    });
    await page.goto(url, { waitUntil: "domcontentloaded" });
    await page.waitForSelector(workload.recentProjectsReady.section);
    await page.waitForFunction((loadingSelector) => !document.querySelector(loadingSelector), workload.recentProjectsReady.loadingState);
    values.push(await page.evaluate(() => performance.now() - window.__syntaxisBenchmarkStart));
    await context.close();
  }
} finally {
  await browser.close();
}

values.sort((left, right) => left - right);
const result = {
  schemaVersion: 1,
  timestamp: new Date().toISOString(),
  url,
  viewport: workload.viewport,
  fixture: workload.fixtureDirectory,
  metric: "navigation start to Recent Projects section present and loading state absent",
  rawMeasurementsMs: values,
  medianMs: values[Math.floor(values.length / 2)],
  minMs: values[0],
  maxMs: values.at(-1),
  rangeMs: values.at(-1) - values[0],
};
await mkdir(join(research, "results"), { recursive: true });
const output = process.env.AUTORESEARCH_RECENT_OUTPUT ?? join(research, "results", `recent-projects-${Date.now()}.json`);
await writeFile(output, `${JSON.stringify(result, null, 2)}\n`);
process.stdout.write(`Wrote ${output}\n`);
