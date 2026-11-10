import { describe, it, expect } from "vitest";
import { activeWordIndex } from "./highlight";

describe("activeWordIndex", () => {
  const starts = [0, 1, 2.5, 4, 4]; // non-decreasing, with a duplicate

  it("returns -1 before the first word", () => {
    expect(activeWordIndex(starts, -1)).toBe(-1);
    expect(activeWordIndex(starts, -0.001)).toBe(-1);
  });

  it("returns -1 for an empty list", () => {
    expect(activeWordIndex([], 5)).toBe(-1);
  });

  it("selects the word whose start <= t (exact boundary inclusive)", () => {
    expect(activeWordIndex(starts, 0)).toBe(0);
    expect(activeWordIndex(starts, 1)).toBe(1);
    expect(activeWordIndex(starts, 2.5)).toBe(2);
  });

  it("selects the last started word between boundaries", () => {
    expect(activeWordIndex(starts, 0.5)).toBe(0);
    expect(activeWordIndex(starts, 2.49)).toBe(1);
    expect(activeWordIndex(starts, 3.9)).toBe(2);
  });

  it("returns the last index past the end (and last of equal starts)", () => {
    expect(activeWordIndex(starts, 4)).toBe(4);
    expect(activeWordIndex(starts, 100)).toBe(4);
  });
});
