#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { extname, join, resolve, sep } from "node:path";

import { strToU8, zipSync } from "fflate";
import puppeteer from "puppeteer-core";

import { checkAiNavigation } from "./ai-navigation-smoke.mjs";

let staticServer;
let baseUrl = process.env.BROWSER_URL ?? "http://127.0.0.1:4174";
if (process.env.BROWSER_DIST) {
  const root = resolve(process.env.BROWSER_DIST);
  staticServer = createServer(async (request, response) => {
    try {
      const pathname = decodeURIComponent(new URL(request.url ?? "/", "http://browser").pathname);
      let path = resolve(root, `.${pathname}`);
      if (path !== root && !path.startsWith(`${root}${sep}`)) {
        response.writeHead(403).end("Forbidden");
        return;
      }
      const details = await stat(path).catch(() => undefined);
      if (details?.isDirectory()) path = resolve(path, "index.html");
      if (!details || details.isDirectory()) {
        const candidate = await stat(path).catch(() => undefined);
        if (!candidate?.isFile()) path = resolve(root, "index.html");
      }
      const content = await readFile(path);
      response.writeHead(200, {
        "content-type": contentType(path),
        "cache-control": "no-store",
      });
      response.end(content);
    } catch (error) {
      response.writeHead(500).end(String(error));
    }
  });
  await new Promise((accept, reject) => {
    staticServer.once("error", reject);
    staticServer.listen(0, "127.0.0.1", accept);
  });
  const address = staticServer.address();
  if (!address || typeof address === "string")
    throw new Error("Could not start browser smoke server.");
  baseUrl = `http://127.0.0.1:${address.port}`;
}
const browserCommand = process.env.AUTORESEARCH_BROWSER_PATH ?? "chromium";
const executablePath = browserCommand.includes("/")
  ? browserCommand
  : execFileSync("which", [browserCommand], { encoding: "utf8" }).trim();
const browser = await puppeteer.launch({ headless: true, executablePath });
const page = await browser.newPage();
const browserErrors = [];
const serverRequests = [];
const providerRequests = [];
const previewNetworkRequests = [];
const previewPolicyBlocks = [];
const fixtureDirectory = await mkdtemp(join(tmpdir(), "syntaxis-browser-smoke-"));
const archivePath = join(fixtureDirectory, "workspace.zip");
await writeFile(
  archivePath,
  zipSync({
    "README.md": strToU8("# Browser workspace\n"),
    "index.html": strToU8(
      '<!doctype html><link rel="stylesheet" href="style.css"><main id="preview-smoke">Preview smoke</main><img src="https://preview.invalid/pixel.png">',
    ),
    "style.css": strToU8(
      '@import url("https://preview.invalid/leak.css"); #preview-smoke { color: rgb(12, 34, 56); }\n',
    ),
  }),
);

page.on("console", (message) => {
  if (message.type() !== "error") return;
  if (
    message.text().includes("preview.invalid") &&
    message.text().includes("Content Security Policy")
  ) {
    previewPolicyBlocks.push(message.text());
  } else {
    browserErrors.push(message.text());
  }
});
page.on("pageerror", (error) => browserErrors.push(error.message));
await page.setRequestInterception(true);
page.on("request", async (request) => {
  const url = new URL(request.url());
  if (url.origin === new URL(baseUrl).origin && url.pathname.startsWith("/api/")) {
    serverRequests.push(`${request.method()} ${url.pathname}`);
  }
  if (url.hostname === "preview.invalid") {
    previewNetworkRequests.push(`${request.method()} ${url.pathname}`);
    await request.respond({ status: 204 });
    return;
  }
  if (url.hostname !== "provider.invalid") {
    await request.continue();
    return;
  }
  const corsHeaders = {
    "access-control-allow-origin": "*",
    "access-control-allow-headers":
      request.headers()["access-control-request-headers"] ?? "authorization,content-type",
    "access-control-allow-methods": "POST,OPTIONS",
  };
  if (request.method() === "OPTIONS") {
    await request.respond({ status: 204, headers: corsHeaders });
    return;
  }
  providerRequests.push({
    authorization: request.headers().authorization,
    body: JSON.parse(request.postData() ?? "{}"),
  });
  const content = providerRequests.at(-1)?.body.messages?.at(-1)?.content;
  const prompt =
    typeof content === "string"
      ? content
      : content
          ?.filter((part) => part.type === "text")
          .map((part) => part.text)
          .join("");
  if (prompt === "Cancel me") {
    await new Promise((resolve) => setTimeout(resolve, 2_000));
  }
  const body =
    prompt === "Exceed limit"
      ? `data: ${JSON.stringify({ choices: [{ delta: { content: "x".repeat(1_048_576) } }] })}\n\n`
      : [
          'data: {"choices":[{"delta":{"content":"hel"}}]}',
          "",
          'data: {"choices":[{"delta":{"content":"lo!"}}]}',
          "",
          'data: {"choices":[{"delta":{},"finish_reason":"stop"}]}',
          "",
          "data: [DONE]",
          "",
          "",
        ].join("\n");
  await request
    .respond({
      status: 200,
      contentType: "text/event-stream",
      headers: { ...corsHeaders, "cache-control": "no-store" },
      body,
    })
    .catch((error) => {
      if (prompt !== "Cancel me") throw error;
    });
});

async function open(path, ready) {
  await page.goto(new URL(path, baseUrl), { waitUntil: "networkidle0", timeout: 120_000 });
  await page.waitForSelector(ready, { timeout: 30_000 });
  const body = await page.$eval("body", (element) => element.innerText);
  if (/panicked at|application error|unreachable executed/i.test(body)) {
    throw new Error(`Browser route ${path} rendered an application failure.`);
  }
}

let stage = "home";
try {
  await open("/", "section[aria-labelledby='recent-title']");
  const browserWorkspace = await page.waitForSelector("button[aria-label='Browser workspace']", {
    timeout: 30_000,
  });
  if (!browserWorkspace) throw new Error("Browser workspace action was not rendered.");
  const archiveInput = await page.waitForSelector('input[name="workspace-archive"]');
  if (!archiveInput) throw new Error("Workspace ZIP import was not rendered.");
  await archiveInput.uploadFile(archivePath);
  await page.waitForFunction(() => location.pathname.endsWith("/files"), { timeout: 30_000 });
  await page.waitForSelector("#workspace-main-content", { timeout: 30_000 });
  await page.waitForFunction(() => globalThis.SyntaxisBrowserArchive?.version === 1, {
    timeout: 30_000,
  });

  stage = "Files search, open, and static Preview";
  await clickButton(page, "Search");
  await page.waitForSelector('input[aria-label="Search workspace"]', { timeout: 30_000 });
  await setInputValue(page, 'input[aria-label="Search workspace"]', "Preview smoke");
  await page.$eval('input[aria-label="Search workspace"]', (input) =>
    input.closest("form")?.requestSubmit(),
  );
  await page.waitForFunction(
    () =>
      [...document.querySelectorAll('[role="treeitem"]')].some((item) =>
        item.textContent?.includes("index.html"),
      ),
    { timeout: 30_000 },
  );
  await page.evaluate(() => {
    const item = [...document.querySelectorAll('[role="treeitem"]')].find((candidate) =>
      candidate.textContent?.includes("index.html"),
    );
    item?.click();
  });
  await page.waitForFunction(
    () =>
      [...document.querySelectorAll('[role="treeitem"]')].some(
        (item) => item.textContent?.includes("index.html") && item.ariaSelected === "true",
      ),
    { timeout: 30_000 },
  );
  await clickLink(page, "Preview");
  const previewFrameElement = await page.waitForSelector(
    'iframe[title="Workspace preview"][src^="blob:"]',
    {
      timeout: 30_000,
    },
  );
  if (!previewFrameElement) throw new Error("Browser Preview did not render its blob lease.");
  const previewFrame = await previewFrameElement.contentFrame();
  if (!previewFrame) throw new Error("Browser Preview iframe did not create a browsing context.");
  await previewFrame.waitForSelector("#preview-smoke", {
    timeout: 30_000,
  });
  await previewFrame.waitForFunction(
    () => getComputedStyle(document.querySelector("#preview-smoke")).color === "rgb(12, 34, 56)",
    { timeout: 30_000 },
  );
  const previewResult = await previewFrame.$eval("#preview-smoke", (element) => ({
    color: getComputedStyle(element).color,
    text: element.textContent,
  }));
  if (previewResult.text !== "Preview smoke" || previewResult.color !== "rgb(12, 34, 56)") {
    throw new Error(
      `Browser Preview did not render the imported document and asset: ${JSON.stringify(previewResult)}`,
    );
  }
  if (previewNetworkRequests.length > 0) {
    throw new Error(
      `Browser Preview attempted external requests:\n${previewNetworkRequests.join("\n")}`,
    );
  }
  if (previewPolicyBlocks.length === 0) {
    throw new Error("Browser Preview did not enforce its no-network content policy.");
  }

  stage = "Terminal execute and cancel";
  await open("/workspaces/browser/terminal", "[aria-label='Browser terminal']");
  await page.waitForFunction(() => globalThis.SyntaxisBrowserBash?.version === 1, {
    timeout: 30_000,
  });
  await page.click('button[aria-label="New terminal"]');
  await page.waitForSelector("#terminal-name", { timeout: 30_000 });
  await setInputValue(page, "#terminal-name", "smoke shell");
  await clickButton(page, "Create terminal");
  const terminalInput = 'section[aria-label="Browser terminal"] form input:not([disabled])';
  await page.waitForSelector(terminalInput, { timeout: 30_000 });
  await setInputValue(page, terminalInput, "echo terminal-smoke");
  await page.$eval('section[aria-label="Browser terminal"] form', (form) => form.requestSubmit());
  await page.waitForFunction(() => document.body.innerText.includes("terminal-smoke"), {
    timeout: 30_000,
  });
  await setInputValue(page, terminalInput, "sleep 5");
  await page.$eval('section[aria-label="Browser terminal"] form', (form) => form.requestSubmit());
  await page.waitForSelector('button[aria-label="Stop command"]', { timeout: 30_000 });
  await page.click('button[aria-label="Stop command"]');
  await page.waitForFunction(() => document.body.innerText.includes("Command cancelled."), {
    timeout: 30_000,
  });

  stage = "Git initialization";
  await open("/workspaces/browser/git", "#workspace-main-content");
  await page.waitForFunction(() => globalThis.SyntaxisBrowserGit?.version === 1, {
    timeout: 30_000,
  });
  await page.waitForFunction(() => document.body.innerText.includes("Initialize repository"), {
    timeout: 30_000,
  });
  await clickButton(page, "Initialize repository");
  await page.waitForFunction(() => document.body.innerText.includes("Initialized Git repository"), {
    timeout: 30_000,
  });

  stage = "shared routes and bridge versions";
  const routes = [
    ["/workspaces/browser/preview", "[aria-label='HTML preview']"],
    ["/workspaces/browser/ai", "[aria-label='AI assistant']"],
    ["/workspaces/browser/ai/settings/extensions", "[aria-label='AI settings']"],
    ["/workspaces/browser/ai/settings/provider-accounts", "[aria-label='AI settings']"],
  ];
  for (const [path, ready, bridge] of routes) {
    await open(path, ready);
    if (path.endsWith("/extensions")) {
      const body = await page.$eval("body", (element) => element.innerText);
      if (!body.includes("This settings surface is not available in the current runtime.")) {
        throw new Error("Browser AI did not render a safe fallback for an unsupported deep link.");
      }
    }
    if (bridge) {
      await page.waitForFunction(
        (globalName) => globalThis[globalName]?.version === 1,
        { timeout: 30_000 },
        bridge,
      );
    }
  }
  const providerSettingsBody = await page.$eval("body", (element) => element.innerText);
  if (!providerSettingsBody.includes("Provider accounts")) {
    throw new Error("Browser AI settings did not reach the supported provider section.");
  }

  stage = "AI provider settings";
  await page.waitForSelector("#ai-endpoint", { timeout: 30_000 });
  await setInputValue(page, "#ai-endpoint", "https://provider.invalid/v1/chat/completions");
  await clickButton(page, "Save endpoint");
  await page.click('[data-provider-id="custom"] button');
  await page.waitForSelector('[role="dialog"] input[type="password"]', { timeout: 30_000 });
  await setInputValue(page, '[role="dialog"] input[type="password"]', "smoke-key");
  await clickButton(page, "Continue");
  await page.waitForFunction(
    () =>
      document.querySelector('[role="dialog"]')?.textContent.includes("API key saved in this tab."),
    { timeout: 30_000 },
  );
  await clickButton(page, "Close");
  await clickButton(page, "General");
  await page.waitForSelector('#ai-model option[value="custom/"]', { timeout: 30_000 });
  await page.select("#ai-model", "custom/");
  await setInputValue(page, "#ai-custom-model", "smoke-model");
  await clickButton(page, "Save settings");
  await page.waitForFunction(() => document.body.innerText.includes("AI provider settings saved"), {
    timeout: 30_000,
  });
  stage = "AI conversation startup";
  await clickButton(page, "Chat");
  await page.waitForSelector("[aria-label='AI assistant']", { timeout: 30_000 });
  await clickButton(page, "New chat");
  await page.waitForSelector("#syntaxis-ai-composer", { timeout: 30_000 });
  await setTextAreaValue(page, "#syntaxis-ai-composer", "Stream a reply");
  await page.waitForFunction(
    () => !document.querySelector('button[aria-label="Send message"]')?.disabled,
    { timeout: 30_000 },
  );
  await page.evaluate(() => {
    const log = document.querySelector('[role="log"]');
    globalThis.__syntaxisAiSmokeFrames = [];
    new MutationObserver(() => {
      globalThis.__syntaxisAiSmokeFrames.push(log?.innerText ?? "");
    }).observe(log, { childList: true, characterData: true, subtree: true });
  });
  stage = "AI progressive response";
  await page.click('button[aria-label="Send message"]');
  await page.waitForFunction(
    () => document.querySelector('[role="log"]')?.innerText.includes("hello!"),
    { timeout: 30_000 },
  );
  const streamFrames = await page.evaluate(() => globalThis.__syntaxisAiSmokeFrames);
  if (!streamFrames.some((frame) => frame.includes("hel") && !frame.includes("hello!"))) {
    throw new Error("Browser AI response did not render a progressive delta before completion.");
  }
  if (providerRequests.length !== 1 || providerRequests[0].body.stream !== true) {
    throw new Error("Browser AI did not make the expected streaming provider request.");
  }
  if (providerRequests[0].authorization !== "Bearer smoke-key") {
    throw new Error("Browser AI did not send the in-memory credential to the selected provider.");
  }

  stage = "AI message actions and usage";
  await page.waitForSelector('button[aria-label="Copy response"]', { timeout: 30_000 });
  if (await page.evaluate(() => matchMedia("(hover: hover)").matches)) {
    await page.hover("[data-agent-response]");
  } else {
    // Headless browsers without hover support reveal actions through keyboard focus.
    await page.focus('button[aria-label="Copy response"]');
  }
  await page.waitForFunction(
    () => {
      const button = document.querySelector('button[aria-label="Copy response"]');
      return button && Number(getComputedStyle(button.parentElement).opacity) === 1;
    },
    { timeout: 30_000 },
  );
  await page.evaluate(() => {
    const id = document.querySelector("[data-agent-response]").dataset.agentResponse;
    window.dispatchEvent(
      new CustomEvent("syntaxis-ai-read-aloud", {
        detail: { kind: "availability", available: true },
      }),
    );
    window.dispatchEvent(
      new CustomEvent("syntaxis-ai-read-aloud", {
        detail: { kind: "start", id },
      }),
    );
  });
  await page.waitForSelector('button[aria-label="Stop reading response"]', { timeout: 30_000 });
  await page.evaluate(() => {
    const id = document.querySelector("[data-agent-response]").dataset.agentResponse;
    window.dispatchEvent(
      new CustomEvent("syntaxis-ai-read-aloud", {
        detail: { kind: "end", id },
      }),
    );
  });
  await page.waitForSelector('button[aria-label="Read response aloud"]', { timeout: 30_000 });
  await page.evaluate(() => {
    const branch = document.querySelector(
      'button[aria-label="Edit this prompt and branch from here"]',
    );
    if (!branch || branch.closest("article") || branch.textContent.trim()) {
      throw new Error("The icon-only branch action must sit outside the prompt bubble.");
    }
    if (!branch.querySelector(".rotate-180")) {
      throw new Error("The Split icon must be rotated 180 degrees.");
    }
    const header = document.querySelector('[aria-label="AI assistant"] header');
    if (header?.querySelector('[aria-label="AI settings"], [aria-label="Compact context"]')) {
      throw new Error("The chat header must not duplicate settings or compaction controls.");
    }
    for (const button of document.querySelectorAll(
      'button[aria-label="Copy response"], form button[aria-label]',
    )) {
      if (button.type !== "button" && button.getAttribute("aria-label") !== "Send message") {
        throw new Error("An icon action must not submit the composer.");
      }
    }
  });
  await page.click('button[aria-label="Session usage"]');
  await page.waitForSelector("#ai-session-usage-content", { timeout: 30_000 });
  await page.evaluate(() => {
    const popup = document.querySelector("#ai-session-usage-content");
    if (!popup?.textContent.includes("Session usage")) {
      throw new Error("The session usage popover did not open.");
    }
    if (popup.textContent.includes("Compact context")) {
      throw new Error("Browser chat must not offer unsupported compaction.");
    }
  });
  await page.focus("#ai-session-usage-trigger");
  await page.keyboard.press("Escape");
  await page.waitForSelector("#ai-session-usage-content", { hidden: true, timeout: 30_000 });

  stage = "AI cancellation";
  await setTextAreaValue(page, "#syntaxis-ai-composer", "Cancel me");
  await page.waitForFunction(
    () => !document.querySelector('button[aria-label="Send message"]')?.disabled,
    { timeout: 30_000 },
  );
  await page.click('button[aria-label="Send message"]');
  await waitForProviderRequests(providerRequests, 2);
  await page.waitForFunction(
    () => Boolean(document.querySelector('button[aria-label="Cancel response"]')),
    { timeout: 30_000 },
  );
  await page.click('button[aria-label="Cancel response"]');
  await page.waitForFunction(
    () => document.body.innerText.includes("The AI request was cancelled."),
    {
      timeout: 30_000,
    },
  );

  stage = "AI response limit";
  await setTextAreaValue(page, "#syntaxis-ai-composer", "Exceed limit");
  await page.waitForFunction(
    () => !document.querySelector('button[aria-label="Send message"]')?.disabled,
    { timeout: 30_000 },
  );
  await page.click('button[aria-label="Send message"]');
  await waitForProviderRequests(providerRequests, 3);
  await page.waitForFunction(
    () => document.body.innerText.includes("The browser AI response limit was reached."),
    { timeout: 30_000 },
  );
  if (providerRequests.some((request) => request.body.stream !== true)) {
    throw new Error("A browser AI request did not enable provider streaming.");
  }

  stage = "AI draft persistence";
  const previousSession = new URL(page.url()).searchParams.get("sessionId");
  if (!previousSession) throw new Error("The active conversation was not reflected in the URL.");
  await setTextAreaValue(page, "#syntaxis-ai-composer", "Keep this unsent draft");
  await page.waitForFunction(
    () => !document.querySelector('button[aria-label="Send message"]')?.disabled,
    { timeout: 30_000 },
  );
  await page.waitForFunction(
    (sessionId) =>
      Object.keys(localStorage).some(
        (key) =>
          key.startsWith("syntaxis:ai-draft:") &&
          key.endsWith(`:${sessionId}`) &&
          localStorage.getItem(key) === "Keep this unsent draft",
      ),
    { timeout: 30_000 },
    previousSession,
  );
  stage = "AI navigation, deletion, and action-menu dismissal";
  await checkAiNavigation(page, clickButton, clickLink);

  if (serverRequests.length > 0) {
    throw new Error(`Browser local flows attempted server APIs:\n${serverRequests.join("\n")}`);
  }
  if (browserErrors.length > 0) {
    throw new Error(`Browser errors:\n${browserErrors.join("\n")}`);
  }
  console.log("Browser shared-shell smoke passed.");
} catch (error) {
  const body = await page
    .$eval("body", (element) => element.innerText)
    .catch(() => "<unavailable>");
  console.error(`Browser smoke failed during ${stage}.\n${body.slice(0, 4_000)}`);
  throw error;
} finally {
  await browser.close();
  if (staticServer) await new Promise((accept) => staticServer.close(accept));
  await rm(fixtureDirectory, { recursive: true, force: true });
}

async function setInputValue(page, selector, value) {
  await page.$eval(
    selector,
    (element, next) => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(element, next);
      element.dispatchEvent(new Event("input", { bubbles: true }));
    },
    value,
  );
}

async function setTextAreaValue(page, selector, value) {
  await page.$eval(
    selector,
    (element, next) => {
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
      setter?.call(element, next);
      element.dispatchEvent(new Event("input", { bubbles: true }));
    },
    value,
  );
}

async function clickButton(page, label) {
  const clicked = await page.evaluate((text) => {
    const button = [...document.querySelectorAll("button")].find(
      (candidate) =>
        (candidate.getAttribute("aria-label") ?? candidate.textContent?.trim()) === text,
    );
    button?.click();
    return Boolean(button);
  }, label);
  if (!clicked) throw new Error(`Browser action was not rendered: ${label}`);
}

async function clickLink(page, label) {
  const clicked = await page.evaluate((text) => {
    const link = [...document.querySelectorAll("a")].find(
      (candidate) => candidate.textContent?.trim() === text,
    );
    link?.click();
    return Boolean(link);
  }, label);
  if (!clicked) throw new Error(`Browser navigation was not rendered: ${label}`);
}

async function waitForProviderRequests(requests, expected) {
  const deadline = Date.now() + 30_000;
  while (requests.length < expected) {
    if (Date.now() >= deadline) {
      throw new Error(`Expected ${expected} provider requests, received ${requests.length}.`);
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

function contentType(path) {
  return (
    {
      ".css": "text/css; charset=utf-8",
      ".html": "text/html; charset=utf-8",
      ".ico": "image/x-icon",
      ".js": "text/javascript; charset=utf-8",
      ".json": "application/json; charset=utf-8",
      ".png": "image/png",
      ".svg": "image/svg+xml",
      ".wasm": "application/wasm",
      ".woff2": "font/woff2",
    }[extname(path)] ?? "application/octet-stream"
  );
}
