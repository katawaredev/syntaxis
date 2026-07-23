#!/usr/bin/env node

import { createHash } from "node:crypto";
import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const root = resolve(dirname(scriptPath), "..");
const packageRoot = dirname(fileURLToPath(import.meta.resolve("@earendil-works/pi-coding-agent/package.json")));
const manifest = JSON.parse(readFileSync(resolve(packageRoot, "package.json"), "utf8"));
const declarations = readFileSync(resolve(packageRoot, "dist/core/settings-manager.d.ts"), "utf8");
const documentation = readFileSync(resolve(packageRoot, "docs/settings.md"), "utf8");
const outputPath = resolve(root, "src/ai/generated_settings.rs");

// This is a product policy, not duplicated setting metadata. Metadata below is
// scraped from the pinned Pi docs. TUI-only controls (theme, editor/cursor
// layout, tree navigation, changelog/startup presentation, terminal rendering,
// model cycling, and Markdown rendering) are intentionally absent.
const exposed = [
  ["Model & thinking", "defaultProvider"],
  ["Model & thinking", "defaultModel"],
  ["Model & thinking", "defaultThinkingLevel", "select:off|minimal|low|medium|high|xhigh|max"],
  ["Conversation", "compaction.enabled"],
  ["Reliability", "retry.enabled"],
  ["Message delivery", "steeringMode", "select:one-at-a-time|all"],
  ["Message delivery", "followUpMode", "select:one-at-a-time|all"],
  ["Images", "images.blockImages"],
  ["Security & privacy", "defaultProjectTrust", "select:ask|always|never"],
  ["Security & privacy", "enableInstallTelemetry"],
];

const setters = new Map([
  ["defaultProvider", "setDefaultProvider"],
  ["defaultModel", "setDefaultModel"],
  ["defaultThinkingLevel", "setDefaultThinkingLevel"],
  ["compaction.enabled", "setCompactionEnabled"],
  ["retry.enabled", "setRetryEnabled"],
  ["steeringMode", "setSteeringMode"],
  ["followUpMode", "setFollowUpMode"],
  ["images.blockImages", "setBlockImages"],
  ["defaultProjectTrust", "setDefaultProjectTrust"],
  ["enableInstallTelemetry", "setEnableInstallTelemetry"],
]);

const labels = new Map([
  ["compaction.enabled", "Auto-compaction"],
  ["retry.enabled", "Automatic retries"],
]);

const documented = new Map();
for (const line of documentation.split("\n")) {
  const match = line.match(/^\|\s*`([^`]+)`\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*(.*?)\s*\|$/);
  if (!match) continue;
  const [, path, type, rawDefault, description] = match;
  let defaultValue = rawDefault.trim() === "-" ? "" : rawDefault.trim().replace(/^`(.*)`$/, "$1");
  if (type.trim() === "string") defaultValue = defaultValue.replace(/^"(.*)"$/, "$1");
  if (type.trim() === "number" && !/^\d+$/.test(defaultValue)) defaultValue = "";
  documented.set(path, { type: type.trim(), defaultValue, description: description.trim().replaceAll("`", "") });
}

const label = path => path
  .split(".")
  .at(-1)
  .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
  .replace(/^./, value => value.toUpperCase());

const settings = exposed.map(([section, path, control]) => {
  const metadata = documented.get(path);
  if (!metadata) throw new Error(`Pi ${manifest.version} docs no longer describe setting ${path}`);
  const inferred = metadata.type === "boolean"
    ? "toggle"
    : metadata.type === "number"
      ? "number"
      : metadata.type === "string[]"
        ? "string-array"
        : "text";
  return [
    section,
    path,
    labels.get(path) ?? label(path),
    metadata.description,
    control ?? inferred,
    metadata.defaultValue,
  ];
});

for (const [, path] of settings) {
  const topLevel = path.split(".")[0];
  if (!new RegExp(`\\n\\s*${topLevel}\\?:`).test(declarations)) {
    throw new Error(`Pi ${manifest.version} no longer declares setting ${path}`);
  }
  const setter = setters.get(path);
  if (!setter || !new RegExp(`\\n\\s*${setter}\\(`).test(declarations)) {
    throw new Error(`Pi ${manifest.version} no longer exposes a supported setter for ${path}`);
  }
}

const cacheKey = createHash("sha256")
  .update("syntaxis-pi-settings-v2\0")
  .update(readFileSync(scriptPath))
  .update(manifest.version)
  .update(declarations)
  .update(documentation)
  .digest("hex");

if (existsSync(outputPath)) {
  const header = readFileSync(outputPath, "utf8").slice(0, 220);
  if (header.includes(`pi-settings-cache-key: ${cacheKey}`)) {
    console.log(`Pi settings metadata is current (${cacheKey.slice(0, 12)})`);
    process.exit(0);
  }
}

const rust = value => JSON.stringify(value);
const rows = settings.map(([section, path, label, description, control, defaultValue]) => {
  let kind;
  if (control === "toggle") kind = "PiSettingKind::Toggle";
  else if (control === "number") kind = "PiSettingKind::Number";
  else if (control === "text") kind = "PiSettingKind::Text";
  else if (control === "string-array") kind = "PiSettingKind::StringArray";
  else kind = `PiSettingKind::Select(&[${control.slice(7).split("|").map(rust).join(", ")}])`;
  return `    PiSettingDefinition { section: ${rust(section)}, path: ${rust(path)}, label: ${rust(label)}, description: ${rust(description)}, kind: ${kind}, default_value: ${rust(defaultValue)}, setter: ${rust(setters.get(path))} },`;
});

const generated = `// @generated by scripts/generate-pi-settings.mjs; do not edit.\n// pi-settings-cache-key: ${cacheKey}\n// pi-coding-agent: ${manifest.version}\n\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum PiSettingKind {\n    Toggle,\n    Select(&'static [&'static str]),\n    Number,\n    Text,\n    StringArray,\n}\n\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub struct PiSettingDefinition {\n    pub section: &'static str,\n    pub path: &'static str,\n    pub label: &'static str,\n    pub description: &'static str,\n    pub kind: PiSettingKind,\n    pub default_value: &'static str,\n    pub setter: &'static str,\n}\n\n#[cfg_attr(not(feature = "server"), allow(dead_code))]\npub const PI_SETTINGS_SCHEMA_VERSION: &str = ${rust(manifest.version)};\n\npub const PI_SETTING_DEFINITIONS: &[PiSettingDefinition] = &[\n${rows.join("\n")}\n];\n`;

const temporaryPath = `${outputPath}.tmp`;
writeFileSync(temporaryPath, generated);
renameSync(temporaryPath, outputPath);
console.log(`Generated Pi ${manifest.version} settings metadata (${cacheKey.slice(0, 12)})`);
