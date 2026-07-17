#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import esbuild from "esbuild";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");
const assetsDir = resolve(root, "assets/terminal");
const bridgeSource = resolve(assetsDir, "bridge-source.js");
const bundleDest = resolve(assetsDir, "terminal.bundle.js");

const pkg = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const wtermVersion =
  pkg.dependencies?.["@wterm/dom"] ??
  pkg.devDependencies?.["@wterm/dom"] ??
  "unknown";

// Embed wterm CSS so the bridge can inject it at first mount without
// requiring a separate <link> tag or Rust-side changes.
const cssPath = resolve(root, "node_modules/@wterm/dom/src/terminal.css");
const css = readFileSync(cssPath, "utf8");

// Prepend a CSS injection snippet to the bridge source.  esbuild will
// inline the string literal into the bundle.
const cssInjection =
  `(()=>{if(typeof document!=="undefined"&&!document.getElementById("wterm-css")){` +
  `const s=document.createElement("style");s.id="wterm-css";` +
  `s.textContent=${JSON.stringify(css)};document.head.appendChild(s);` +
  `}})();\n`;

const bridgeCode = readFileSync(bridgeSource, "utf8");

const result = await esbuild.build({
  stdin: {
    contents: cssInjection + bridgeCode,
    resolveDir: root,
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

console.log(`Built terminal bundle with @wterm/dom ${wtermVersion}`);
