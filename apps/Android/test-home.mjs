// Shared-UI smoke test with an isolated host backend and a simulated Android port.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import puppeteer from "puppeteer-core";

const temp = await mkdtemp(join(tmpdir(), "syntaxis-android-home-"));
const listener = createServer();
await new Promise((done) => listener.listen(0, "127.0.0.1", done));
const port = listener.address().port;
await new Promise((done) => listener.close(done));
const origin = `http://127.0.0.1:${port}`;
await mkdir(join(temp, "projects"));
const backend = spawn(resolve("target/dx/syntaxis-server/debug/web/server"), [], {
  cwd: temp,
  env: {
    ...process.env,
    IP: "127.0.0.1",
    PORT: String(port),
    SYNTAXIS_AUTH_DISABLED: "true",
    SYNTAXIS_DATA_DIR: join(temp, "state"),
    SYNTAXIS_PROJECTS_ROOT: join(temp, "projects"),
    SYNTAXIS_WORKSPACE_ROOTS: join(temp, "projects"),
    DIOXUS_PUBLIC_PATH: resolve("target/dx/syntaxis-server/debug/web/public"),
  },
  stdio: "ignore",
});
let browser;
try {
  for (let attempt = 0; attempt < 100; attempt++) {
    const response = await fetch(`${origin}/health`).catch(() => null);
    if (response?.status === 204) break;
    if (attempt === 99) throw new Error("Isolated debug backend did not start");
    await new Promise((done) => setTimeout(done, 100));
  }
  const created = await fetch(`${origin}/api/projects/create`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ path: "Same name" }),
  });
  assert.equal(created.status, 200, await created.clone().text());
  const local = await created.json();
  const remote = {
    ...local,
    root: "/srv/remote/Same name",
    last_opened_unix_ms: local.last_opened_unix_ms + 1000,
  };
  browser = await puppeteer.launch({
    executablePath: process.env.AUTORESEARCH_BROWSER_PATH || "/usr/bin/chromium",
    headless: true,
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 960, height: 600, deviceScaleFactor: 1 });
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  await page.evaluateOnNewDocument((project) => {
    const events = new EventTarget();
    window.androidCommands = [];
    window.androidRemote = true;
    window.SyntaxisAndroid = {
      addEventListener: (...args) => events.addEventListener(...args),
      removeEventListener: (...args) => events.removeEventListener(...args),
      postMessage: (text) => {
        const request = JSON.parse(text);
        window.androidCommands.push(request);
        const result =
          request.action === "state"
            ? {
                remoteConfigured: window.androidRemote,
                currentRemote: false,
                projects: window.androidRemote && request.projects ? [project] : [],
                error: null,
              }
            : null;
        queueMicrotask(() =>
          events.dispatchEvent(
            new MessageEvent("message", { data: JSON.stringify({ id: request.id, result }) }),
          ),
        );
      },
    };
  }, remote);
  await page.goto(origin, { waitUntil: "networkidle0" });
  await page.waitForFunction(() => document.body.innerText.includes("/srv/remote/Same name"));
  const text = await page.$eval("body", (body) => body.innerText);
  assert.ok(text.includes("Local") && text.includes("Remote"));
  assert.ok(text.includes(local.root));
  assert.ok(!text.includes("Sign out"));
  await page.screenshot({ path: "/tmp/syntaxis-android-home.png", fullPage: true });
  await page.evaluate(() =>
    [...document.querySelectorAll("button")]
      .find((button) => button.innerText.includes("/srv/remote/Same name"))
      .click(),
  );
  assert.ok(
    await page.evaluate(() =>
      window.androidCommands.some(
        (request) =>
          request.action === "open" && request.remote && request.path.startsWith("/workspaces/"),
      ),
    ),
  );
  await page.evaluate(() =>
    document.querySelector('summary[aria-label="Manage runtime storage"]').click(),
  );
  await page.evaluate(() =>
    [...document.querySelectorAll("button")]
      .find((button) => button.innerText === "Remote settings")
      .click(),
  );
  await page.waitForFunction(() =>
    window.androidCommands.some((request) => request.action === "configure"),
  );
  await page.goto(`${origin}/new-project`, { waitUntil: "networkidle0" });
  await page.waitForFunction(() =>
    [...document.querySelectorAll("label")].some((label) =>
      label.innerText.includes("Local project"),
    ),
  );
  await page.evaluate(() =>
    [...document.querySelectorAll("label")]
      .find((label) => label.innerText.includes("Local project"))
      .querySelector("input")
      .click(),
  );
  await page.waitForFunction(() =>
    window.androidCommands.some(
      (request) => request.action === "open" && request.remote && request.path === "/new-project",
    ),
  );
  await page.goto(`${origin}/clone-project`, { waitUntil: "networkidle0" });
  await page.waitForFunction(() =>
    [...document.querySelectorAll("label")].some((label) =>
      label.innerText.includes("Local project"),
    ),
  );
  await page.evaluate(() =>
    [...document.querySelectorAll("label")]
      .find((label) => label.innerText.includes("Local project"))
      .querySelector("input")
      .click(),
  );
  await page.waitForFunction(() =>
    window.androidCommands.some(
      (request) => request.action === "open" && request.remote && request.path === "/clone-project",
    ),
  );
  // A new page without remote configuration must keep the choice hidden.
  const localPage = await browser.newPage();
  await localPage.evaluateOnNewDocument(() => {
    const events = new EventTarget();
    window.SyntaxisAndroid = {
      addEventListener: (...args) => events.addEventListener(...args),
      removeEventListener: (...args) => events.removeEventListener(...args),
      postMessage: (text) => {
        const { id } = JSON.parse(text);
        queueMicrotask(() =>
          events.dispatchEvent(
            new MessageEvent("message", {
              data: JSON.stringify({
                id,
                result: {
                  remoteConfigured: false,
                  currentRemote: false,
                  projects: [],
                  error: null,
                },
              }),
            }),
          ),
        );
      },
    };
  });
  await localPage.goto(origin, { waitUntil: "networkidle0" });
  await localPage.waitForFunction(() => document.body.textContent.includes("Add Remote"));
  await localPage.goto(`${origin}/new-project`, { waitUntil: "networkidle0" });
  await localPage.waitForSelector("#new-project-path");
  assert.ok(!(await localPage.$eval("body", (body) => body.innerText)).includes("Local project"));
  assert.deepEqual(errors, []);
  console.log(
    "Android shared-home smoke checks passed (mixed identities, remote setup, create/clone destination, local-only default).",
  );
} finally {
  await browser?.close();
  backend.kill("SIGTERM");
  await new Promise((done) => {
    if (backend.exitCode !== null) done();
    else backend.once("exit", done);
  });
  await rm(temp, { recursive: true, force: true });
}
