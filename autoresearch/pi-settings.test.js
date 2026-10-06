import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { getDocsPath } from "@earendil-works/pi-coding-agent";
import { parseSettingsMarkdown } from "../scripts/pi-settings-metadata.mjs";

test("Pi settings metadata keeps union types in their Markdown table cell", () => {
  const settings = parseSettingsMarkdown(readFileSync(join(getDocsPath(), "settings.md"), "utf8"));
  expect(settings.get("defaultThinkingLevel").defaultValue).toBe("medium");
  expect(settings.get("defaultThinkingLevel").description).toBe("Startup thinking level.");
  expect(settings.get("steeringMode").defaultValue).toBe("one-at-a-time");
  expect(settings.get("defaultProjectTrust").defaultValue).toBe("ask");
  expect(settings.get("defaultTools").type).toBe("string[]");
});

test("Pi settings metadata preserves escaped description pipes and numeric defaults", () => {
  const settings = parseSettingsMarkdown(
    "| `count` | number | `12` | Read `a` \\| `b`. |\n| `optional` | string | - | Optional. |",
  );
  expect(settings.get("count")).toEqual({
    type: "number",
    defaultValue: "12",
    description: "Read a | b.",
  });
  expect(settings.get("optional").defaultValue).toBe("");
});
