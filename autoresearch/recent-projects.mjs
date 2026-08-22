#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import puppeteer from "puppeteer-core";

import { summary } from "./stats.mjs";

const root = resolve(import.meta.dirname, "..");
const research = join(root, "autoresearch");
const workload = JSON.parse(await readFile(join(research, "workload.json"), "utf8"));
const repetitions = Number(process.env.AUTORESEARCH_REPETITIONS ?? workload.repetitions);
const url = process.env.AUTORESEARCH_HOME_URL ?? workload.homeUrl;
const editorUrl = process.env.AUTORESEARCH_EDITOR_URL ?? workload.editorUrl;
const executablePath = process.env.AUTORESEARCH_BROWSER_PATH ?? "chromium";
const resolvedExecutable = executablePath.includes("/")
  ? executablePath
  : execFileSync("which", [executablePath], { encoding: "utf8" }).trim();
const authorization = `Bearer ${process.env.LHCI_API_TOKEN ?? "syntaxis-local-lighthouse-token-0001"}`;

function measurementSummary(values) {
  const result = summary(values);
  return {
    rawMeasurementsMs: result.values,
    count: result.count,
    medianMs: result.median,
    minMs: result.min,
    maxMs: result.max,
    rangeMs: result.range,
    p25Ms: result.p25,
    p75Ms: result.p75,
    p95Ms: result.p95,
    varianceMs2: result.variance,
    standardDeviationMs: result.standardDeviation,
    medianAbsoluteDeviationMs: result.medianAbsoluteDeviation,
  };
}

async function configurePage(page, viewport) {
  page.setDefaultNavigationTimeout(workload.navigationTimeoutMs);
  page.setDefaultTimeout(workload.readinessTimeoutMs);
  await page.setViewport({
    width: viewport.width,
    height: viewport.height,
    deviceScaleFactor: viewport.deviceScaleFactor,
    isMobile: viewport.formFactor === "mobile",
    hasTouch: viewport.formFactor === "mobile",
  });
  await page.setExtraHTTPHeaders({ Authorization: authorization });
  await page.evaluateOnNewDocument(() => {
    window.__syntaxisAutoresearchLongTasks = [];
    try {
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          window.__syntaxisAutoresearchLongTasks.push({
            startTimeMs: entry.startTime,
            durationMs: entry.duration,
          });
        }
      }).observe({ type: "longtask", buffered: true });
    } catch {
      // Long Tasks are supplementary; unsupported browsers still run the workload.
    }
  });
}

function observePageIssues(page) {
  const issues = { consoleErrors: [], pageErrors: [], requestFailures: [] };
  page.on("console", (message) => {
    if (message.type() === "error") issues.consoleErrors.push(message.text());
  });
  page.on("pageerror", (error) => issues.pageErrors.push(error.message));
  page.on("requestfailed", (request) => {
    issues.requestFailures.push({
      url: request.url(),
      error: request.failure()?.errorText ?? "unknown request failure",
    });
  });
  return issues;
}

async function waitUntilReady(page, ready) {
  for (const selector of ready.present) await page.waitForSelector(selector);
  for (const selector of ready.absent) {
    await page.waitForFunction((candidate) => !document.querySelector(candidate), {}, selector);
  }
}

async function measureInteraction(page, interaction) {
  if (!interaction) return null;
  return page.evaluate(
    ({ trigger, triggerText, ready, timeoutMs }) =>
      new Promise((resolvePromise, reject) => {
        const candidate = [...document.querySelectorAll(trigger)].find(
          (element) =>
            element.getAttribute("aria-label") === triggerText ||
            element.querySelector("strong")?.textContent?.trim() === triggerText ||
            element.textContent?.trim() === triggerText,
        );
        if (!(candidate instanceof HTMLElement)) {
          reject(new Error(`Interaction trigger not found: ${triggerText}`));
          return;
        }

        let observer;
        let frame = 0;
        let finished = false;
        const started = performance.now();
        const timeout = window.setTimeout(() => {
          observer?.disconnect();
          reject(new Error(`Interaction did not reach ${ready} within ${timeoutMs} ms`));
        }, timeoutMs);
        const finishIfReady = () => {
          if (finished || !document.querySelector(ready)) return;
          finished = true;
          observer?.disconnect();
          window.clearTimeout(timeout);
          frame = requestAnimationFrame(() => {
            frame = requestAnimationFrame(() => resolvePromise(performance.now() - started));
          });
        };
        observer = new MutationObserver(finishIfReady);
        observer.observe(document.documentElement, { childList: true, subtree: true });
        candidate.click();
        finishIfReady();

        window.addEventListener(
          "pagehide",
          () => {
            observer?.disconnect();
            window.clearTimeout(timeout);
            if (frame) cancelAnimationFrame(frame);
          },
          { once: true },
        );
      }),
    { ...interaction, timeoutMs: workload.readinessTimeoutMs },
  );
}

async function measure(page, targetUrl, ready, interaction = null) {
  await page.goto(targetUrl, { waitUntil: "domcontentloaded" });
  await waitUntilReady(page, ready);
  const usableMs = await page.evaluate(() => performance.now());
  const interactionResponseMs = await measureInteraction(page, interaction);
  const profile = await page.evaluate(() => {
    const longTasks = window.__syntaxisAutoresearchLongTasks ?? [];
    return {
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
      longTasks,
      longTaskTotalMs: longTasks.reduce((total, task) => total + task.durationMs, 0),
      longestTaskMs: Math.max(0, ...longTasks.map((task) => task.durationMs)),
    };
  });
  return { usableMs, interactionResponseMs, ...profile };
}

async function coldMeasurement(target) {
  const browser = await puppeteer.launch({ headless: true, executablePath: resolvedExecutable });
  const page = await browser.newPage();
  const issues = observePageIssues(page);
  try {
    await configurePage(page, workload.viewport);
    return {
      measurement: await measure(page, target.url, target.ready, target.interaction),
      issues,
    };
  } finally {
    await browser.close();
  }
}

async function responsiveAudit() {
  const browser = await puppeteer.launch({ headless: true, executablePath: resolvedExecutable });
  const audits = [];
  try {
    for (const viewport of workload.responsiveViewports ?? []) {
      const page = await browser.newPage();
      const issues = observePageIssues(page);
      await configurePage(page, viewport);
      const routes = [
        {
          name: "home",
          url,
          ready: {
            present: [workload.recentProjectsReady.section],
            absent: [workload.recentProjectsReady.loadingState],
          },
        },
      ];
      if (editorUrl) {
        routes.push({
          name: "editor",
          url: editorUrl,
          ready: {
            present: [workload.editorReady.mount, workload.editorReady.editable],
            absent: [],
          },
        });
      }
      for (const route of routes) {
        await page.goto(route.url, { waitUntil: "domcontentloaded" });
        await waitUntilReady(page, route.ready);
        const layout = await page.evaluate(() => {
          const root = document.querySelector(".app-viewport");
          const rect = root?.getBoundingClientRect();
          return {
            innerWidth,
            innerHeight,
            documentWidth: document.documentElement.scrollWidth,
            documentHeight: document.documentElement.scrollHeight,
            rootRect: rect
              ? { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }
              : null,
            horizontalOverflow: document.documentElement.scrollWidth > innerWidth + 1,
          };
        });
        audits.push({ viewport, route: route.name, layout, issues: structuredClone(issues) });
      }
      await page.close();
    }
  } finally {
    await browser.close();
  }
  return audits;
}

async function gitDiffAudit() {
  const fixtureFile = join(research, "runtime/fixture/src/main.rs");
  const original = await readFile(fixtureFile, "utf8");
  const marker = "// Autoresearch route-local syntax highlighting smoke.";
  const parsedEditorUrl = new URL(editorUrl);
  const workspaceParts = parsedEditorUrl.pathname.split("/");
  const workspaceIndex = workspaceParts.indexOf("workspaces");
  if (workspaceIndex < 0 || !workspaceParts[workspaceIndex + 1]) {
    throw new Error(`Could not derive Git route from editor URL: ${editorUrl}`);
  }
  parsedEditorUrl.pathname = `/workspaces/${workspaceParts[workspaceIndex + 1]}/git`;
  parsedEditorUrl.search = "";

  let browser;
  try {
    await writeFile(fixtureFile, `${original.trimEnd()}\n${marker}\n`);
    browser = await puppeteer.launch({ headless: true, executablePath: resolvedExecutable });
    const page = await browser.newPage();
    const issues = observePageIssues(page);
    await configurePage(page, workload.viewport);
    await page.goto(parsedEditorUrl.href, { waitUntil: "domcontentloaded" });
    await page.waitForFunction(
      (path) =>
        [...document.querySelectorAll("button")].some((button) =>
          button.textContent?.includes(path),
        ),
      {},
      "src/main.rs",
    );
    await page.evaluate(() => {
      const button = [...document.querySelectorAll("button")].find((candidate) =>
        candidate.textContent?.includes("src/main.rs"),
      );
      if (!(button instanceof HTMLElement)) throw new Error("Changed fixture file not found");
      button.click();
    });
    await page.waitForSelector(".dxc-diff-editor");
    await page.waitForSelector(".dxc-syntax-comment");
    const rendered = await page.evaluate((expectedMarker) => {
      const root = document.querySelector(".app-viewport");
      const rect = root?.getBoundingClientRect();
      return {
        highlightedMs: performance.now(),
        markerPresent: document
          .querySelector(".dxc-diff-editor")
          ?.textContent?.includes(expectedMarker),
        syntaxCommentCount: document.querySelectorAll(".dxc-syntax-comment").length,
        layout: {
          innerWidth,
          documentWidth: document.documentElement.scrollWidth,
          rootRect: rect
            ? { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }
            : null,
          horizontalOverflow: document.documentElement.scrollWidth > innerWidth + 1,
        },
      };
    }, marker);
    return { url: parsedEditorUrl.href, path: "src/main.rs", ...rendered, issues };
  } finally {
    await browser?.close();
    await writeFile(fixtureFile, original);
  }
}

const recentValues = [];
const interactionValues = [];
const homeTaskValues = [];
const editorValues = [];
const recentProfiles = [];
const editorProfiles = [];
const pageIssues = [];
const targets = [
  {
    name: "recent-projects",
    url,
    ready: {
      present: [workload.recentProjectsReady.section],
      absent: [workload.recentProjectsReady.loadingState],
    },
    interaction: workload.homeInteraction,
  },
];
if (editorUrl) {
  targets.push({
    name: "editor",
    url: editorUrl,
    ready: {
      present: [workload.editorReady.mount, workload.editorReady.editable],
      absent: [],
    },
  });
}

for (let index = 0; index < repetitions; index += 1) {
  const orderedTargets = index % 2 === 0 ? targets : [...targets].reverse();
  for (const target of orderedTargets) {
    const { measurement, issues } = await coldMeasurement(target);
    pageIssues.push({ target: target.name, repetition: index + 1, ...issues });
    if (target.name === "recent-projects") {
      recentValues.push(measurement.usableMs);
      interactionValues.push(measurement.interactionResponseMs);
      homeTaskValues.push(measurement.usableMs + measurement.interactionResponseMs);
      recentProfiles.push(measurement);
    } else {
      editorValues.push(measurement.usableMs);
      editorProfiles.push(measurement);
    }
  }
}

const responsive = await responsiveAudit();
const gitDiff = await gitDiffAudit();
pageIssues.push({ target: "git-diff", repetition: 1, ...gitDiff.issues });
const result = {
  schemaVersion: 3,
  timestamp: new Date().toISOString(),
  browser: execFileSync(resolvedExecutable, ["--version"], { encoding: "utf8" }).trim(),
  browserIsolation: "a fresh Chromium process for every timed navigation",
  url,
  editorUrl,
  viewport: workload.viewport,
  fixture: workload.fixtureDirectory,
  repetitions,
  pageIssues,
  responsive,
  gitDiff,
  recentProjects: {
    metric: "navigation start to Recent Projects section present and loading state absent",
    selectors: workload.recentProjectsReady,
    profiles: recentProfiles,
    ...measurementSummary(recentValues),
  },
  homeInteraction: {
    metric: "New project click to interactive dialog painted",
    selectors: workload.homeInteraction,
    ...measurementSummary(interactionValues),
  },
  homeTask: {
    metric: "navigation start to Recent Projects ready, followed by New project dialog painted",
    selectors: {
      recentProjects: workload.recentProjectsReady,
      interaction: workload.homeInteraction,
    },
    ...measurementSummary(homeTaskValues),
  },
  editor: editorValues.length
    ? {
        metric: "navigation start to mounted editable CodeMirror textbox",
        selectors: workload.editorReady,
        profiles: editorProfiles,
        ...measurementSummary(editorValues),
      }
    : null,
};
await mkdir(join(research, "results"), { recursive: true });
const output =
  process.env.AUTORESEARCH_RECENT_OUTPUT ?? join(research, "results", `browser-${Date.now()}.json`);
await writeFile(output, `${JSON.stringify(result, null, 2)}\n`);
process.stdout.write(`Wrote ${output}\n`);
