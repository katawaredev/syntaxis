import { readFile } from "node:fs/promises";
import { describe, expect, test } from "bun:test";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const indexHtml = join(root, "apps/main/index.html");

describe("mobile viewport policy", () => {
  test("keeps automatic zoom disabled", async () => {
    const html = await readFile(indexHtml, "utf8");
    const viewport = html.match(/<meta\s+name="viewport"\s+content="([^"]+)"/)?.[1];

    expect(viewport).toBeDefined();
    expect(viewport).toContain("maximum-scale=1");
  });
});
