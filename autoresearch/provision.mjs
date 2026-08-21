#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { cp, mkdir, readFile, realpath, rm, stat, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const research = join(root, "autoresearch");
const workload = JSON.parse(await readFile(join(research, "workload.json"), "utf8"));
const sourceFixture = await realpath(join(root, workload.fixtureDirectory));
const fixtureStat = await stat(sourceFixture);
if (!fixtureStat.isDirectory()) {
  throw new Error(`Fixture is not a directory: ${sourceFixture}`);
}

const runtime = join(research, "runtime");
const data = join(runtime, "data");
const fixture = join(runtime, "fixture");
await mkdir(runtime, { recursive: true });
await rm(fixture, { recursive: true, force: true });
await cp(sourceFixture, fixture, { recursive: true });

const gitEnvironment = {
  ...process.env,
  GIT_AUTHOR_DATE: "2023-11-14T22:13:20Z",
  GIT_COMMITTER_DATE: "2023-11-14T22:13:20Z",
};
execFileSync("git", ["init", "--quiet", "--initial-branch=main", fixture]);
execFileSync("git", ["-C", fixture, "add", "--all"]);
execFileSync(
  "git",
  [
    "-C",
    fixture,
    "-c",
    "commit.gpgSign=false",
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "user.name=Syntaxis Autoresearch",
    "-c",
    "user.email=autoresearch@localhost",
    "commit",
    "--quiet",
    "--message=Initialize benchmark fixture",
  ],
  { env: gitEnvironment },
);

const source = await readFile(join(fixture, "src/main.rs"));
const timestamp = 1_700_000_000_000;
const name = basename(sourceFixture);
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
process.stdout.write(
  `${JSON.stringify({ sourceFixture, fixture, data, slug: registry.workspaces[0].slug })}\n`,
);
