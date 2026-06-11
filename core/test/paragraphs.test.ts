import { describe, it, expect } from "vitest";
import { groupParagraphs } from "../src/editor.js";
import type { EditorSentence } from "../src/editor.js";

/** Build a one-word sentence spanning [start, end] (speaker irrelevant here). */
function sent(id: string, start: number, end: number): EditorSentence {
  return {
    id,
    words: [{ text: id, start, end, speaker: "SPEAKER_00" }],
  };
}

describe("groupParagraphs", () => {
  it("empty input -> []", () => {
    expect(groupParagraphs([])).toEqual([]);
  });

  it("splits on a gap larger than the threshold", () => {
    // gap between s0 (ends 1.0) and s1 (starts 2.0) = 1.0 > 0.75 default.
    const s = [sent("a", 0, 1.0), sent("b", 2.0, 3.0)];
    const paras = groupParagraphs(s);
    expect(paras.map((p) => p.map((x) => x.id))).toEqual([["a"], ["b"]]);
  });

  it("keeps sentences together when the gap is below the threshold", () => {
    // gap 0.2 < 0.75 -> one paragraph.
    const s = [sent("a", 0, 1.0), sent("b", 1.2, 2.0)];
    const paras = groupParagraphs(s);
    expect(paras.length).toBe(1);
    expect(paras[0]!.map((x) => x.id)).toEqual(["a", "b"]);
  });

  it("forces a break after maxSentences even with no pauses", () => {
    // 7 contiguous sentences (no gaps), maxSentences=3 -> 3 + 3 + 1.
    const s = Array.from({ length: 7 }, (_, i) =>
      sent(`s${i}`, i, i + 1),
    );
    const paras = groupParagraphs(s, 0.75, 3);
    expect(paras.map((p) => p.length)).toEqual([3, 3, 1]);
  });

  it("uses default gap 0.75 and maxSentences 6", () => {
    // 8 contiguous sentences, defaults -> 6 + 2.
    const s = Array.from({ length: 8 }, (_, i) => sent(`s${i}`, i, i + 1));
    const paras = groupParagraphs(s);
    expect(paras.map((p) => p.length)).toEqual([6, 2]);
  });
});
