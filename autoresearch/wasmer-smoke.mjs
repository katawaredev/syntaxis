import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, resolve, sep } from "node:path";
import puppeteer from "puppeteer-core";

const root = resolve(import.meta.dirname, "../crates/runtime-browser/assets");
const server = createServer(async (request, response) => {
  const headers = {
    "Cross-Origin-Opener-Policy": "same-origin",
    "Cross-Origin-Embedder-Policy": "require-corp",
    "Content-Security-Policy":
      "default-src 'self'; connect-src 'self' https:; script-src 'self' 'unsafe-eval' 'wasm-unsafe-eval'; worker-src 'self' blob:",
  };
  if (request.url === "/favicon.ico") {
    response.writeHead(204, headers).end();
    return;
  }
  if (request.url === "/") {
    response.writeHead(200, { ...headers, "content-type": "text/html" });
    response.end(
      '<!doctype html><title>Wasmer smoke</title><script src="/browser-terminal.bundle.js"></script>',
    );
    return;
  }
  try {
    const path = resolve(
      root,
      `.${decodeURIComponent(new URL(request.url, "http://localhost").pathname)}`,
    );
    if (!path.startsWith(`${root}${sep}`)) throw new Error("Invalid asset path");
    const content = await readFile(path);
    const type = extname(path) === ".wasm" ? "application/wasm" : "application/javascript";
    response.writeHead(200, { ...headers, "content-type": type });
    response.end(content);
  } catch {
    response.writeHead(404, headers).end();
  }
});
await new Promise((accept) => server.listen(0, "127.0.0.1", accept));
let browser;
try {
  browser = await puppeteer.launch({
    headless: true,
    executablePath: process.env.AUTORESEARCH_BROWSER_PATH ?? "/usr/bin/chromium",
  });
  const page = await browser.newPage();
  page.setDefaultTimeout(180_000);
  page.on("console", (message) => {
    if (message.type() === "error") console.error(message.text());
  });
  page.on("pageerror", (error) => console.error(error.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.waitForFunction(() => globalThis.SyntaxisBrowserBash);
  const cdp = await page.createCDPSession();
  await cdp.send("Network.enable");
  const sockets = [];
  cdp.on("Network.webSocketCreated", ({ url }) => sockets.push(url));
  await page.evaluate(() => {
    globalThis.SyntaxisWasmerAssets = "/browser-wasmer-sdk";
  });
  const execute = async (command, snapshot = { directories: [], files: [] }) => {
    console.log(`Running ${command}`);
    const result = await page.evaluate(
      (command, snapshot) => globalThis.SyntaxisBrowserBash.execute(command, snapshot),
      command,
      snapshot,
    );
    assert.equal(result.exitCode, 0, JSON.stringify(result));
    return result;
  };
  assert.match((await execute("wasmer status")).stdout, /threads available/);
  const manifest = {
    directories: [".syntaxis"],
    files: [
      {
        path: ".syntaxis/wasmer.json",
        content: Array.from(
          new TextEncoder().encode(
            JSON.stringify({
              tools: {
                check: {
                  package: "python/python@=3.13.20",
                  args: ["-c", "print('project check')"],
                },
              },
            }),
          ),
        ),
      },
    ],
  };
  const requestsBeforeTools = [];
  const trackTools = (request) => requestsBeforeTools.push(request.url());
  page.on("request", trackTools);
  assert.match((await execute("wasmer tools", manifest)).stdout, /check: python\/python@=3.13.20/);
  page.off("request", trackTools);
  assert.deepEqual(
    requestsBeforeTools,
    [],
    "Tool discovery must not initialize the SDK or fetch packages",
  );
  const prepared = await execute("wasmer prepare python check", manifest);
  assert.match(prepared.stdout, /Prepared check/);
  assert.deepEqual(prepared.snapshot, manifest);
  const cached = await execute("wasmer prepare python", manifest);
  assert.match(cached.stdout, /0 bytes \(cache hit\)/);
  await page.reload();
  await page.waitForFunction(() => globalThis.SyntaxisBrowserBash);
  await page.evaluate(() => {
    globalThis.SyntaxisWasmerAssets = "/browser-wasmer-sdk";
  });
  assert.match((await execute("wasmer prepare python", manifest)).stdout, /0 bytes \(cache hit\)/);
  assert.equal((await execute("wasmer run check", manifest)).stdout, "project check\n");
  console.log("Project tool discovery, preparation and cached execution passed");
  // A raw WASI module with one empty _start function. No registry or proxy needed.
  const wasm = [
    0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 19, 2, 6, 95, 115,
    116, 97, 114, 116, 0, 0, 6, 109, 101, 109, 111, 114, 121, 2, 0, 10, 4, 1, 2, 0, 11,
  ];
  await execute("wasmer run ./empty.wasm", {
    directories: ["empty"],
    files: [{ path: "empty.wasm", content: wasm }],
  });
  console.log("Local WASI passed");
  const original = {
    directories: ["empty"],
    files: [{ path: "input.txt", content: Array.from(new TextEncoder().encode("café 🐍\n")) }],
  };
  const python = await execute(
    'wasmer run python -- -c \'from pathlib import Path; print(Path("input.txt").read_text(), end=""); Path("output.bin").write_bytes(bytes([0,128,255])); Path("made").mkdir(); Path("input.txt").unlink()\'',
    original,
  );
  assert.equal(python.stdout, "café 🐍\n");
  assert.deepEqual(
    python.snapshot.files.find((file) => file.path === "output.bin").content,
    [0, 128, 255],
  );
  assert(!python.snapshot.files.some((file) => file.path === "input.txt"));
  assert(python.snapshot.directories.includes("empty"));
  assert(python.snapshot.directories.includes("made"));
  console.log("Python filesystem passed");
  const pipe = await execute(
    "printf 'café 🐍' | wasmer run python -- -c 'import sys; sys.stdout.buffer.write(sys.stdin.buffer.read())' > pipe.txt; cat pipe.txt",
  );
  assert.equal(pipe.stdout, "café 🐍");
  const binaryPipe = await execute(
    "wasmer run python -- -c 'import sys; sys.stdout.buffer.write(bytes([0,128,255]))' > binary.bin",
  );
  assert.deepEqual(
    binaryPipe.snapshot.files.find((file) => file.path === "binary.bin").content,
    [0, 128, 255],
  );
  const grep = await execute("wasmer run rg -- café input.txt", original);
  assert.match(grep.stdout, /café 🐍/);
  console.log("Ripgrep and Unicode/binary pipes passed");
  assert.equal((await execute("wasmer run bash -- -c 'echo bash-guest'")).stdout, "bash-guest\n");
  assert.equal((await execute("wasmer run node -- -e 'console.log(6 * 7)'")).stdout, "42\n");
  assert.equal(
    (
      await execute(
        'wasmer run node -- -e \'require("fs").writeFileSync("node.txt", "local JS"); console.log(require("fs").readFileSync("node.txt", "utf8"))\'',
      )
    ).stdout,
    "local JS\n",
  );
  console.log("Bash and Edge.js passed");
  const invalid = await page.evaluate(() =>
    globalThis.SyntaxisBrowserBash.execute("wasmer run ./invalid.wasm", {
      directories: [],
      files: [{ path: "invalid.wasm", content: [0] }],
    }),
  );
  assert.equal(invalid.exitCode, 1);
  assert.equal((await execute("wasmer run python -- -c 'print(42)'")).stdout, "42\n");
  const nonzero = await page.evaluate(() =>
    globalThis.SyntaxisBrowserBash.execute(
      "wasmer run python -- -c 'import sys; print(\"failed\"); sys.exit(7)'",
      { directories: [], files: [] },
    ),
  );
  assert.equal(nonzero.exitCode, 7);
  assert.equal(nonzero.stdout, "failed\n");
  const tooLarge = await page.evaluate(() =>
    globalThis.SyntaxisBrowserBash.execute(
      'wasmer run python -- -c \'from pathlib import Path; Path("too-large.bin").write_bytes(b"x" * (8*1024*1024+1))\'',
      { directories: [], files: [] },
    ),
  );
  assert.equal(tooLarge.exitCode, 1);
  assert.deepEqual(tooLarge.snapshot.files, []);
  console.log("Malformed guest recovery, exit codes and file bounds passed");
  const outputCap = await page.evaluate(() =>
    globalThis.SyntaxisBrowserBash.execute("wasmer run python -- -c 'print(\"x\" * 1048576)'", {
      directories: [],
      files: [],
    }),
  );
  assert.equal(
    outputCap.exitCode,
    0,
    JSON.stringify({ exitCode: outputCap.exitCode, stderr: outputCap.stderr }),
  );
  assert(outputCap.stdout.length <= 512 * 1024);
  assert.match(outputCap.stderr, /output truncated/);
  console.log("Output bound passed");
  const cancel = await page.evaluate(async () => {
    const pending = globalThis.SyntaxisBrowserBash.execute(
      'wasmer run python -- -c \'import time; from pathlib import Path; Path("cancelled.txt").write_text("discard me"); time.sleep(60)\'',
      { directories: [], files: [] },
    );
    setTimeout(() => globalThis.SyntaxisBrowserBash.cancel(), 1500);
    return pending;
  });
  assert.equal(cancel.exitCode, 130);
  assert.deepEqual(cancel.snapshot.files, []);
  console.log("Cancellation discards writes passed");
  const timeout = await page.evaluate(() =>
    globalThis.SyntaxisBrowserBash.execute(
      "wasmer run python -- -c 'import time; time.sleep(60)'",
      { directories: [], files: [] },
    ),
  );
  assert.equal(timeout.exitCode, 124, JSON.stringify(timeout));
  console.log("Guest timeout passed");
  assert.deepEqual(sockets, [], "Guest execution must not open a WISP connection");
  await page.evaluate(() =>
    Object.defineProperty(globalThis, "crossOriginIsolated", { value: false }),
  );
  const unavailable = await page.evaluate(() =>
    globalThis.SyntaxisBrowserBash.execute("wasmer run python -- -V", {
      directories: [],
      files: [],
    }),
  );
  assert.equal(unavailable.exitCode, 1);
  assert.match(unavailable.stderr, /cross-origin isolation/);
  assert.equal((await execute("echo fallback")).stdout, "fallback\n");
  console.log("Missing isolation and shell fallback passed");
} finally {
  await browser?.close();
  server.close();
}
