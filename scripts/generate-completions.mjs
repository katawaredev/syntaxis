#!/usr/bin/env node

import { createHash } from "node:crypto";
import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const root = resolve(dirname(scriptPath), "..");
const outputPath = resolve(root, "crates/editor/src/generated_completions.rs");
const packageNames = [
  "@codemirror/autocomplete",
  "@codemirror/lang-css",
  "@codemirror/lang-html",
  "@codemirror/lang-javascript",
  "@codemirror/lang-sql",
  "@codemirror/state",
  "mdn-data",
  "typescript",
];

function packageVersion(name) {
  const manifest = resolve(root, "node_modules", name, "package.json");
  return JSON.parse(readFileSync(manifest, "utf8")).version;
}

const versions = Object.fromEntries(packageNames.map(name => [name, packageVersion(name)]));
const cacheKey = createHash("sha256")
  .update("syntaxis-completions-v1\0")
  .update(readFileSync(scriptPath))
  .update(JSON.stringify(versions))
  .digest("hex");

if (existsSync(outputPath)) {
  const header = readFileSync(outputPath, "utf8").slice(0, 180);
  if (header.includes(`completion-cache-key: ${cacheKey}`)) {
    console.log(`Completion dictionaries are current (${cacheKey.slice(0, 12)})`);
    process.exit(0);
  }
}

const [autocompleteModule, cssModule, htmlModule, javascriptModule, sqlModule, stateModule, mdnModule, tsModule] =
  await Promise.all([
    import("@codemirror/autocomplete"),
    import("@codemirror/lang-css"),
    import("@codemirror/lang-html"),
    import("@codemirror/lang-javascript"),
    import("@codemirror/lang-sql"),
    import("@codemirror/state"),
    import("mdn-data"),
    import("typescript"),
  ]);
const { CompletionContext } = autocompleteModule;
const { css, cssCompletionSource } = cssModule;
const { html, htmlCompletionSource } = htmlModule;
const { snippets: javascriptSnippets } = javascriptModule;
const { StandardSQL, keywordCompletionSource, sql } = sqlModule;
const { EditorState } = stateModule;
const mdn = mdnModule.default;
const ts = tsModule.default;

function completionContext(marked, extension) {
  const position = marked.indexOf("|");
  if (position < 0) throw new Error(`Completion fixture has no cursor marker: ${marked}`);
  const document = marked.replace("|", "");
  const state = EditorState.create({ doc: document, extensions: [extension] });
  return new CompletionContext(state, position, true);
}

async function labelsFrom(source, marked, extension) {
  const result = await source(completionContext(marked, extension));
  return result?.options?.map(option => option.label) ?? [];
}

function normalized(words) {
  return [...new Set(words)]
    .filter(word => typeof word === "string")
    .map(word => word.trim())
    .filter(word => word.length > 0 && word.length <= 80 && !/\s/.test(word))
    .sort((left, right) => left.localeCompare(right));
}

function topLevelNames(sourceFile) {
  const values = [];
  const types = [];
  const addName = (target, name) => {
    if (name && ts.isIdentifier(name)) target.push(name.text);
  };
  for (const statement of sourceFile.statements) {
    if (ts.isVariableStatement(statement)) {
      for (const declaration of statement.declarationList.declarations) {
        addName(values, declaration.name);
      }
    } else if (ts.isFunctionDeclaration(statement)) {
      addName(values, statement.name);
    } else if (
      ts.isClassDeclaration(statement) ||
      ts.isEnumDeclaration(statement) ||
      ts.isModuleDeclaration(statement)
    ) {
      addName(values, statement.name);
      addName(types, statement.name);
    } else if (ts.isInterfaceDeclaration(statement) || ts.isTypeAliasDeclaration(statement)) {
      addName(types, statement.name);
    }
  }
  return { values, types };
}

function typescriptGlobalNames() {
  const libDirectory = dirname(ts.getDefaultLibFilePath({ target: ts.ScriptTarget.ES2024 }));
  const pending = [
    "lib.es2024.d.ts",
    "lib.dom.d.ts",
    "lib.dom.iterable.d.ts",
    "lib.dom.asynciterable.d.ts",
  ];
  const visited = new Set();
  const values = [];
  const types = [];

  while (pending.length > 0) {
    const fileName = pending.pop();
    if (!fileName || visited.has(fileName)) continue;
    visited.add(fileName);
    const path = resolve(libDirectory, fileName);
    const source = readFileSync(path, "utf8");
    const parsed = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, false);
    const names = topLevelNames(parsed);
    values.push(...names.values);
    types.push(...names.types);
    for (const reference of ts.preProcessFile(source).libReferenceDirectives) {
      pending.push(`lib.${reference.fileName}.d.ts`);
    }
  }

  return {
    values: normalized(values),
    types: normalized([...values, ...types]),
  };
}

const htmlFixtures = [
  "<|",
  "<a |>",
  "<button |>",
  "<form |>",
  "<img |>",
  "<input |>",
  "<link |>",
  "<meta |>",
  "<script |>",
  "<select |>",
  "<table |>",
  "<textarea |>",
  "<video |>",
  '<input type="|">',
  '<button type="|">',
  '<form method="|">',
];
const htmlWords = [];
for (const fixture of htmlFixtures) {
  htmlWords.push(...await labelsFrom(htmlCompletionSource, fixture, html()));
}

const cssWords = [
  ...Object.keys(mdn.css.properties).filter(name => !name.includes("*")),
  ...await labelsFrom(cssCompletionSource, "a { color: r|; }", css()),
  ...await labelsFrom(cssCompletionSource, "@m|", css()),
];
const sqlWords = await labelsFrom(
  keywordCompletionSource(StandardSQL),
  "sel|",
  sql({ dialect: StandardSQL }),
);
const typescriptGlobals = typescriptGlobalNames();
const dictionaries = {
  javascript: normalized([
    ...typescriptGlobals.values,
    ...javascriptSnippets.map(snippet => snippet.label),
  ]),
  typescript: normalized([
    ...typescriptGlobals.types,
    ...javascriptSnippets.map(snippet => snippet.label),
  ]),
  html: normalized(htmlWords),
  css: normalized(cssWords),
  sql: normalized(sqlWords),
};

function rustString(value) {
  return JSON.stringify(value);
}

function rustSlice(name, words) {
  const rows = [];
  for (let index = 0; index < words.length; index += 6) {
    rows.push(`    ${words.slice(index, index + 6).map(rustString).join(", ")},`);
  }
  return `#[rustfmt::skip]\nconst ${name}: &[&str] = &[\n${rows.join("\n")}\n];`;
}

const versionSummary = Object.entries(versions)
  .map(([name, version]) => `${name}@${version}`)
  .join(", ");
const generated = `// @generated by scripts/generate-completions.mjs; do not edit.\n// completion-cache-key: ${cacheKey}\n// packages: ${versionSummary}\n\n${rustSlice("JAVASCRIPT", dictionaries.javascript)}\n\n${rustSlice("TYPESCRIPT", dictionaries.typescript)}\n\n${rustSlice("HTML", dictionaries.html)}\n\n${rustSlice("CSS", dictionaries.css)}\n\n${rustSlice("SQL", dictionaries.sql)}\n\n/// Static completion candidates extracted from pinned web-language packages.\npub fn generated_completion_words(language: &str) -> &'static [&'static str] {\n    match language {\n        "javascript" => JAVASCRIPT,\n        "typescript" | "tsx" => TYPESCRIPT,\n        "html" => HTML,\n        "css" | "scss" => CSS,\n        "sql" => SQL,\n        _ => &[],\n    }\n}\n`;

const temporaryPath = `${outputPath}.tmp`;
writeFileSync(temporaryPath, generated);
renameSync(temporaryPath, outputPath);
const candidateCount = Object.values(dictionaries).reduce((sum, words) => sum + words.length, 0);
console.log(`Generated completion dictionaries (${candidateCount} candidates, ${cacheKey.slice(0, 12)})`);
