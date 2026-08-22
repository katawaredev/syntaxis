import { execFileSync } from "node:child_process";
import { describe, expect, test } from "bun:test";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const provision = join(root, "autoresearch/provision.mjs");

function runProvision() {
  return JSON.parse(
    execFileSync(process.execPath, [provision], {
      cwd: root,
      encoding: "utf8",
    }),
  );
}

describe("autoresearch fixture provisioning", () => {
  test("creates a clean deterministic Git repository outside the source fixture", () => {
    const first = runProvision();
    const firstCommit = execFileSync("git", ["-C", first.fixture, "rev-parse", "HEAD"], {
      encoding: "utf8",
    }).trim();
    const second = runProvision();
    const secondCommit = execFileSync("git", ["-C", second.fixture, "rev-parse", "HEAD"], {
      encoding: "utf8",
    }).trim();
    const status = execFileSync("git", ["-C", second.fixture, "status", "--porcelain=v1"], {
      encoding: "utf8",
    });

    expect(first.fixture).not.toBe(first.sourceFixture);
    expect(first.fixture.startsWith(join(root, "autoresearch/runtime"))).toBe(true);
    expect(firstCommit).toBe(secondCommit);
    expect(status).toBe("");
  });
});
