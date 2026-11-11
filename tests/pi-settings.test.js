import { expect, test } from "bun:test";
import { parseSettingsMarkdown } from "../scripts/pi-settings-metadata.mjs";

test("Pi settings metadata parses escaped table pipes, union types and numeric defaults", () => {
  const settings = parseSettingsMarkdown(
    '| `count` | number | `12` | Read `a` \\| `b`. |\n| `optional` | string | - | Optional. |\n| `mode` | `"one"` \\| `"all"` | `"one"` | Delivery mode. |',
  );
  expect(settings.get("count")).toEqual({
    type: "number",
    defaultValue: "12",
    description: "Read a | b.",
  });
  expect(settings.get("optional").defaultValue).toBe("");
  expect(settings.get("mode")).toEqual({
    type: '"one" | "all"',
    defaultValue: "one",
    description: "Delivery mode.",
  });
});
