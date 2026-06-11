import { describe, it, expect } from "vitest";
import { activeWordIndex, flatWords } from "./highlight";
import { loadTranscript } from "@core/editor";
import type { Transcript } from "@core/types";

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

describe("flatWords", () => {
  it("flattens sentences into document order", () => {
    const t: Transcript = {
      segments: [
        {
          start: 0,
          end: 2,
          text: "Hi there. Bye now.",
          speaker: "SPEAKER_00",
          words: [
            { word: "Hi", start: 0, end: 0.5, speaker: "SPEAKER_00" },
            { word: "there.", start: 0.5, end: 1, speaker: "SPEAKER_00" },
            { word: "Bye", start: 1, end: 1.5, speaker: "SPEAKER_00" },
            { word: "now.", start: 1.5, end: 2, speaker: "SPEAKER_00" },
          ],
        },
      ],
    };
    const state = loadTranscript(t);
    const flat = flatWords(state);
    expect(flat.map((w) => w.text)).toEqual(["Hi", "there.", "Bye", "now."]);
    expect(flat.map((w) => w.start)).toEqual([0, 0.5, 1, 1.5]);
  });
});
