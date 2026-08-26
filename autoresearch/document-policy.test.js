import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { describe, expect, test } from "bun:test";

const root = resolve(import.meta.dirname, "..");

describe("document viewport policy", () => {
  test.each(["index.html", "src/auth/login.html"])(
    "%s retains the mobile Safari code-editor auto-zoom workaround",
    async (path) => {
      const document = await readFile(join(root, path), "utf8");
      const viewport = document.match(/<meta\s+name=["']viewport["'][^>]*>/i)?.[0];

      expect(viewport).toBeDefined();
      expect(viewport).toContain("maximum-scale=1");
    },
  );
});
