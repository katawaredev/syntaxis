#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { extname, join, resolve, sep } from "node:path";

import { strToU8, zipSync } from "fflate";
import puppeteer from "puppeteer-core";

let staticServer;
let baseUrl = process.env.GUEST_URL ?? "http://127.0.0.1:4174";
if (process.env.GUEST_DIST) {
  const root = resolve(process.env.GUEST_DIST);
  staticServer = createServer(async (request, response) => {
    try {
      const pathname = decodeURIComponent(new URL(request.url ?? "/", "http://guest").pathname);
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
    throw new Error("Could not start guest smoke server.");
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
const fixtureDirectory = await mkdtemp(join(tmpdir(), "syntaxis-guest-smoke-"));
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
    "access-control-allow-headers": "authorization,content-type",
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
  const prompt = providerRequests.at(-1)?.body.messages?.at(-1)?.content;
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
          "data: [DONE]",
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
    throw new Error(`Guest route ${path} rendered an application failure.`);
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
  await page.waitForFunction(() => globalThis.SyntaxisGuestArchive?.version === 1, {
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
  await page.waitForFunction(() => globalThis.SyntaxisGuestBash?.version === 1, {
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
  await page.waitForFunction(() => globalThis.SyntaxisGuestGit?.version === 1, {
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
        throw new Error("Guest AI did not render a safe fallback for an unsupported deep link.");
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
    throw new Error("Guest AI settings did not reach the supported provider section.");
  }

  stage = "AI provider settings";
  await page.waitForSelector("#ai-endpoint", { timeout: 30_000 });
  await setInputValue(page, "#ai-endpoint", "https://provider.invalid/v1/chat/completions");
  await setInputValue(page, "#ai-model", "smoke-model");
  await setInputValue(page, "#ai-credential", "smoke-key");
  await clickButton(page, "Save settings");
  await page.waitForFunction(() => document.body.innerText.includes("AI provider settings saved"), {
    timeout: 30_000,
  });
  stage = "AI conversation startup";
  await clickButton(page, "Chat");
  await page.waitForSelector("[aria-label='AI assistant']", { timeout: 30_000 });
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
    throw new Error("Guest AI response did not render a progressive delta before completion.");
  }
  if (providerRequests.length !== 1 || providerRequests[0].body.stream !== true) {
    throw new Error("Guest AI did not make the expected streaming provider request.");
  }
  if (providerRequests[0].authorization !== "Bearer smoke-key") {
    throw new Error("Guest AI did not send the in-memory credential to the selected provider.");
  }

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
    () =>
      document.body.innerText.includes("The provider response exceeds the 1 MiB browser limit."),
    { timeout: 30_000 },
  );
  if (providerRequests.some((request) => request.body.stream !== true)) {
    throw new Error("A guest AI request did not enable provider streaming.");
  }

  if (serverRequests.length > 0) {
    throw new Error(`Guest local flows attempted server APIs:\n${serverRequests.join("\n")}`);
  }
  if (browserErrors.length > 0) {
    throw new Error(`Guest browser errors:\n${browserErrors.join("\n")}`);
  }
  console.log("Guest shared-shell smoke passed.");
} catch (error) {
  const body = await page
    .$eval("body", (element) => element.innerText)
    .catch(() => "<unavailable>");
  console.error(`Guest smoke failed during ${stage}.\n${body.slice(0, 4_000)}`);
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
      (candidate) => candidate.textContent?.trim() === text,
    );
    button?.click();
    return Boolean(button);
  }, label);
  if (!clicked) throw new Error(`Guest action was not rendered: ${label}`);
}

async function clickLink(page, label) {
  const clicked = await page.evaluate((text) => {
    const link = [...document.querySelectorAll("a")].find(
      (candidate) => candidate.textContent?.trim() === text,
    );
    link?.click();
    return Boolean(link);
  }, label);
  if (!clicked) throw new Error(`Guest navigation was not rendered: ${label}`);
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
