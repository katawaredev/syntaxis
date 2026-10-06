import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { build } from "esbuild";

// Preserve the SDK's relative worker/glue/WASM URLs inside one Manganis folder asset.
export async function buildWasmerAssets(root, cacheKey) {
  const source = resolve(root, "node_modules/@wasmer/sdk");
  const destination = resolve(root, "crates/runtime-browser/assets/browser-wasmer-sdk");
  const stamp = resolve(destination, "bundle.stamp");
  if (
    existsSync(stamp) &&
    existsSync(resolve(destination, "dist/index.js")) &&
    existsSync(resolve(destination, "dist/browser-worker.js")) &&
    existsSync(resolve(destination, "pkg/wasmer_sdk_js_bg.wasm")) &&
    readFileSync(stamp, "utf8").trim() === cacheKey
  )
    return;
  mkdirSync(destination, { recursive: true });
  cpSync(resolve(source, "pkg"), resolve(destination, "pkg"), { recursive: true });
  cpSync(resolve(source, "LICENSE"), resolve(destination, "LICENSE"));
  cpSync(
    resolve(root, "node_modules/@mercuryworkshop/wisp-js/LICENSE"),
    resolve(destination, "wisp-js.LICENSE"),
  );
  await build({
    entryPoints: [resolve(source, "dist/index.js"), resolve(source, "dist/browser-worker.js")],
    outdir: resolve(destination, "dist"),
    bundle: true,
    format: "esm",
    platform: "browser",
    target: "es2022",
    external: ["../pkg/*"],
    minify: true,
    logLevel: "info",
  });
  writeFileSync(stamp, `${cacheKey}\n`);
}
