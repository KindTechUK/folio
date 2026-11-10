import { describe, it, expect } from "vitest";
import { assignSpeakers } from "../src/diarization.js";
import { UNKNOWN_SPEAKER } from "../src/types.js";
import type { Word, SpeakerSegment } from "../src/types.js";

const segs: SpeakerSegment[] = [
  { speaker: "SPEAKER_00", start: 0, end: 10 },
  { speaker: "SPEAKER_01", start: 12, end: 20 },
];

describe("assignSpeakers", () => {
  it("assigns a word wholly inside one segment to that speaker", () => {
    const words: Word[] = [{ word: "hi", start: 2, end: 4 }];
    const out = assignSpeakers(words, segs);
    expect(out[0]!.speaker).toBe("SPEAKER_00");
  });

  it("assigns a straddling word to the segment with greater overlap", () => {
    // overlaps SPEAKER_00 [9,10]=1, SPEAKER_01 [12,14]=2 -> SPEAKER_01
    const words: Word[] = [{ word: "x", start: 9, end: 14 }];
    const out = assignSpeakers(words, segs);
    expect(out[0]!.speaker).toBe("SPEAKER_01");
  });

  it("assigns a word in a gap to the nearest speaker", () => {
    // gap is [10,12]; word [10.4,10.6] midpoint 10.5: nearer to seg0 (edge 10) than seg1 (edge 12)
    const words: Word[] = [{ word: "x", start: 10.4, end: 10.6 }];
    const out = assignSpeakers(words, segs);
    expect(out[0]!.speaker).toBe("SPEAKER_00");
    // word closer to seg1
    const words2: Word[] = [{ word: "y", start: 11.4, end: 11.6 }];
    const out2 = assignSpeakers(words2, segs);
    expect(out2[0]!.speaker).toBe("SPEAKER_01");
    // zero-duration word at 10.3 -> nearer seg0
    const words3: Word[] = [{ word: "z", start: 10.3, end: 10.3 }];
    const out3 = assignSpeakers(words3, segs);
    expect(out3[0]!.speaker).toBe("SPEAKER_00");
  });

  it("resolves a word exactly on a boundary deterministically to the earlier segment", () => {
    // adjacent segments sharing boundary at 5
    const adj: SpeakerSegment[] = [
      { speaker: "A", start: 0, end: 5 },
      { speaker: "B", start: 5, end: 10 },
    ];
    // zero-duration word exactly at boundary -> point-containment both, earlier wins
    const words: Word[] = [{ word: "b", start: 5, end: 5 }];
    const out = assignSpeakers(words, adj);
    expect(out[0]!.speaker).toBe("A");
  });

  it("resolves an equal-overlap tie to the earlier segment in input order", () => {
    // word [4,6]: overlap with A [0,5] is 1, with B [5,10] is 1 -> tie
    const tieSegs: SpeakerSegment[] = [
      { speaker: "A", start: 0, end: 5 },
      { speaker: "B", start: 5, end: 10 },
    ];
    const words: Word[] = [{ word: "t", start: 4, end: 6 }];
    const out = assignSpeakers(words, tieSegs);
    expect(out[0]!.speaker).toBe("A");
  });

  it("assigns UNKNOWN to all words when speakerSegments is empty", () => {
    const words: Word[] = [
      { word: "a", start: 0, end: 1 },
      { word: "b", start: 1, end: 2 },
    ];
    const out = assignSpeakers(words, []);
    expect(out.map((w) => w.speaker)).toEqual([UNKNOWN_SPEAKER, UNKNOWN_SPEAKER]);
  });

  it("assigns a zero-duration word inside a segment to that segment", () => {
    const words: Word[] = [{ word: "z", start: 5, end: 5 }];
    const out = assignSpeakers(words, segs);
    expect(out[0]!.speaker).toBe("SPEAKER_00");
  });

  it("does not mutate the input words", () => {
    const words: Word[] = [{ word: "hi", start: 2, end: 4 }];
    const snapshot = JSON.parse(JSON.stringify(words));
    assignSpeakers(words, segs);
    expect(words).toEqual(snapshot);
    expect(words[0]!.speaker).toBeUndefined();
  });

  it("preserves other word fields", () => {
    const words: Word[] = [{ word: "hi", start: 2, end: 4, score: 0.9 }];
    const out = assignSpeakers(words, segs);
    expect(out[0]).toMatchObject({ word: "hi", start: 2, end: 4, score: 0.9, speaker: "SPEAKER_00" });
  });
});
