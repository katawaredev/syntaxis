#!/usr/bin/env node

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const __dirname = dirname(scriptPath);
const root = resolve(__dirname, "..");
const assetsDir = resolve(root, "assets/terminal");
const bridgeSource = resolve(assetsDir, "bridge-source.js");
const sourceLinks = resolve(assetsDir, "source-links.js");
const bundleDest = resolve(assetsDir, "terminal.bundle.js");
const stampDest = resolve(assetsDir, "terminal.bundle.stamp");

function packageVersion(name) {
  const manifest = resolve(root, "node_modules", name, "package.json");
  return JSON.parse(readFileSync(manifest, "utf8")).version;
}

const versions = {
  "@xterm/addon-fit": packageVersion("@xterm/addon-fit"),
  "@xterm/xterm": packageVersion("@xterm/xterm"),
  esbuild: packageVersion("esbuild"),
};

// Embed xterm CSS so the bridge can inject it at first mount without
// requiring a separate <link> tag or Rust-side changes.
const cssPath = resolve(root, "node_modules/@xterm/xterm/css/xterm.css");
const css = readFileSync(cssPath, "utf8");
const bridgeCode = readFileSync(bridgeSource, "utf8");
const cacheKey = createHash("sha256")
  .update("syntaxis-terminal-v1\0")
  .update(readFileSync(scriptPath))
  .update(bridgeCode)
  .update(readFileSync(sourceLinks))
  .update(css)
  .update(JSON.stringify(versions))
  .digest("hex");

if (
  existsSync(bundleDest) &&
  existsSync(stampDest) &&
  readFileSync(stampDest, "utf8").trim() === cacheKey
) {
  console.log(`Terminal bundle is current (${cacheKey.slice(0, 12)})`);
  process.exit(0);
}

// Prepend a CSS injection snippet to the bridge source.  esbuild will
// inline the string literal into the bundle.
const cssInjection =
  `(()=>{if(typeof document!=="undefined"&&!document.getElementById("xterm-css")){` +
  `const s=document.createElement("style");s.id="xterm-css";` +
  `s.textContent=${JSON.stringify(css + `
.xterm-host .xterm .xterm-rows span[style*="text-decoration: underline"] {
  text-decoration-color: var(--primary) !important;
  text-underline-offset: 2px;
}
`)};document.head.appendChild(s);` +
  `}})();\n`;

const { default: esbuild } = await import("esbuild");

const result = await esbuild.build({
  stdin: {
    contents: cssInjection + bridgeCode,
    resolveDir: assetsDir,
    loader: "js",
  },
  bundle: true,
  format: "iife",
  outfile: bundleDest,
  platform: "browser",
  target: "es2020",
  minify: true,
  sourcemap: false,
  logLevel: "info",
});

if (result.errors.length > 0) {
  process.exit(1);
}

writeFileSync(stampDest, `${cacheKey}\n`);
console.log(`Built terminal bundle with @xterm/xterm ${versions["@xterm/xterm"]} (${cacheKey.slice(0, 12)})`);
