#!/usr/bin/env node

import { mkdir, readFile, realpath, stat, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const research = join(root, "autoresearch");
const workload = JSON.parse(await readFile(join(research, "workload.json"), "utf8"));
const fixture = await realpath(join(root, workload.fixtureDirectory));
const fixtureStat = await stat(fixture);
if (!fixtureStat.isDirectory()) throw new Error(`Fixture is not a directory: ${fixture}`);

const source = await readFile(join(fixture, "src/main.rs"));
const runtime = join(research, "runtime");
const data = join(runtime, "data");
const timestamp = 1_700_000_000_000;
const name = basename(fixture);
const registry = {
  version: 1,
  workspaces: [
    {
      id: "autoresearch-fixture-00000000-0000-0000-0000-000000000001",
      slug: name
        .toLowerCase()
        .replaceAll(/[^a-z0-9]+/g, "-")
        .replaceAll(/^-|-$/g, ""),
      name,
      root: fixture,
      icon: { kind: "symbol", name: "rust" },
      profile: {
        technologies: [],
        languages: [{ name: "Rust", bytes: source.byteLength }],
      },
      registered_at_unix_ms: timestamp,
      last_opened_unix_ms: timestamp,
      last_section: "files",
    },
  ],
};

await mkdir(data, { recursive: true });
await writeFile(join(data, "workspaces.json"), `${JSON.stringify(registry, null, 2)}\n`);
process.stdout.write(`${JSON.stringify({ fixture, data, slug: registry.workspaces[0].slug })}\n`);
