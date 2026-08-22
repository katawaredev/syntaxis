import { describe, expect, test } from "bun:test";

import { median, percentile, summary } from "./stats.mjs";

describe("autoresearch statistics", () => {
  test("uses the mean of the middle pair for an even-sized median", () => {
    expect(median([1, 3, 7, 9])).toBe(5);
  });

  test("interpolates percentiles without discarding small samples", () => {
    expect(percentile([10, 20, 30, 40, 50], 0.25)).toBe(20);
    expect(percentile([10, 20, 30, 40], 0.25)).toBe(17.5);
  });

  test("retains collection order while calculating robust spread", () => {
    const result = summary([30, null, 10, Number.NaN, 20]);
    expect(result.values).toEqual([30, 10, 20]);
    expect(result.median).toBe(20);
    expect(result.medianAbsoluteDeviation).toBe(10);
    expect(result.count).toBe(3);
  });

  test("returns a complete empty summary", () => {
    expect(summary([null, Number.NaN])).toMatchObject({
      count: 0,
      median: null,
      values: [],
    });
  });
});
