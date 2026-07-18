import { describe, expect, test } from "bun:test";
import { parseSourceLocations } from "./source-links.js";

describe("parseSourceLocations", () => {
  test.each([
    ["  --> src/main.rs:42:17", "src/main.rs", 42, 17, null, null],
    ["error at src/main.rs:42", "src/main.rs", 42, null, null, null],
    ["src/main.rs:42:17-25", "src/main.rs", 42, 17, 42, 25],
    ["src/main.rs:42:17-44:3", "src/main.rs", 42, 17, 44, 3],
    ["tests/parser_test.py:8:2: failed", "tests/parser_test.py", 8, 2, null, null],
    ["src/app.js:7:3: Unexpected token", "src/app.js", 7, 3, null, null],
    ["src/main.ts(12,5): error TS2322", "src/main.ts", 12, 5, null, null],
    ["./cmd/server.go:19:4: warning", "./cmd/server.go", 19, 4, null, null],
  ])("parses %s", (text, path, line, column, endLine, endColumn) => {
    expect(parseSourceLocations(text)).toMatchObject([
      { path, line, column, endLine, endColumn },
    ]);
  });

  test("finds multiple locations in output order", () => {
    expect(parseSourceLocations("src/a.rs:1:2 and src/b.rs:3:4").map(link => link.path))
      .toEqual(["src/a.rs", "src/b.rs"]);
  });

  test.each([
    "https://example.com/file.js:5:2",
    "typescript@5.0.0",
    "ordinary terminal output",
  ])("does not link %s", text => {
    expect(parseSourceLocations(text)).toEqual([]);
  });
});
