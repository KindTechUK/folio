import { describe, it, expect } from "vitest";
import fraudsterHead from "./fixtures/fraudster_head.json" with { type: "json" };
import { flattenWords, splitSentences, buildTurns } from "../src/transcript.js";
import type { Segment, Transcript } from "../src/types.js";
import { UNKNOWN_SPEAKER } from "../src/types.js";

describe("flattenWords", () => {
  it("resolves word speaker: word > segment > previous word > UNKNOWN", () => {
    const segments: Segment[] = [
      {
        start: 0,
        end: 1,
        text: "",
        speaker: "SPEAKER_A",
        words: [
          { word: "alpha", start: 0, end: 0.5 }, // inherits segment speaker
          { word: "beta", start: 0.5, end: 1, speaker: "SPEAKER_B" }, // own speaker
        ],
      },
    ];
    const flat = flattenWords(segments);
    expect(flat.map((w) => w.text)).toEqual(["alpha", "beta"]);
    expect(flat[0]!.speaker).toBe("SPEAKER_A");
    expect(flat[1]!.speaker).toBe("SPEAKER_B");
  });

  it("falls back to UNKNOWN when no speaker is ever seen", () => {
    const segments: Segment[] = [
      { start: 0, end: 1, text: "", words: [{ word: "hi", start: 0, end: 1 }] },
    ];
    const flat = flattenWords(segments);
    expect(flat[0]!.speaker).toBe(UNKNOWN_SPEAKER);
  });

  it("inherits previous word's speaker when neither word nor segment provide one", () => {
    const segments: Segment[] = [
      {
        start: 0,
        end: 2,
        text: "",
        words: [
          { word: "one", start: 0, end: 1, speaker: "SPEAKER_X" },
          { word: "two", start: 1, end: 2 }, // no speaker, no seg speaker -> inherit X
        ],
      },
    ];
    const flat = flattenWords(segments);
    expect(flat[1]!.speaker).toBe("SPEAKER_X");
  });

  it("inherits last_end for missing start, and start for missing end", () => {
    const segments: Segment[] = [
      {
        start: 0,
        end: 5,
        text: "",
        speaker: "S",
        words: [
          { word: "first", start: 1, end: 2 },
          // missing start -> last_end (2); missing end -> start (2)
          { word: "second" } as any,
        ],
      },
    ];
    const flat = flattenWords(segments);
    expect(flat[1]!.start).toBe(2);
    expect(flat[1]!.end).toBe(2);
  });

  it("drops empty/whitespace tokens", () => {
    const segments: Segment[] = [
      {
        start: 0,
        end: 3,
        text: "",
        speaker: "S",
        words: [
          { word: "  ", start: 0, end: 1 },
          { word: "", start: 1, end: 2 },
          { word: " kept ", start: 2, end: 3 },
        ],
      },
    ];
    const flat = flattenWords(segments);
    expect(flat.map((w) => w.text)).toEqual(["kept"]);
  });

  it("falls back to a single block from segment.text when there are no words", () => {
    const segments: Segment[] = [
      { start: 4, end: 9, text: "  block text  ", speaker: "SPEAKER_T" },
    ];
    const flat = flattenWords(segments);
    expect(flat).toHaveLength(1);
    expect(flat[0]!.text).toBe("block text");
    expect(flat[0]!.start).toBe(4);
    expect(flat[0]!.end).toBe(9);
    expect(flat[0]!.speaker).toBe("SPEAKER_T");
  });

  it("uses last_speaker/last_end for a wordless segment with missing fields", () => {
    const segments: Segment[] = [
      { start: 0, end: 3, text: "", speaker: "S0", words: [{ word: "hi", start: 0, end: 3 }] },
      { text: "more" } as any, // no speaker/start/end -> last_speaker S0, start=last_end 3, end=start
    ];
    const flat = flattenWords(segments);
    expect(flat[1]!.text).toBe("more");
    expect(flat[1]!.speaker).toBe("S0");
    expect(flat[1]!.start).toBe(3);
    expect(flat[1]!.end).toBe(3);
  });

  it("skips a segment whose text is empty/whitespace and has no words", () => {
    const segments: Segment[] = [
      { start: 0, end: 1, text: "   ", speaker: "S" },
      { start: 1, end: 2, text: "real", speaker: "S" },
    ];
    const flat = flattenWords(segments);
    expect(flat.map((w) => w.text)).toEqual(["real"]);
  });
});

describe("splitSentences", () => {
  const fw = (text: string) => ({ text, start: 0, end: 1, speaker: "S" });

  it("splits on . ! ?", () => {
    const sents = splitSentences([fw("hello."), fw("world!"), fw("ok"), fw("why?")]);
    expect(sents.map((s) => s.map((w) => w.text).join(" "))).toEqual([
      "hello.",
      "world!",
      "ok why?",
    ]);
  });

  it("splits when end punctuation is followed by closing quotes/brackets", () => {
    const sents = splitSentences([fw("he"), fw('said."'), fw("then"), fw("done!)")]);
    expect(sents).toHaveLength(2);
    expect(sents[0]!.map((w) => w.text).join(" ")).toBe('he said."');
    expect(sents[1]!.map((w) => w.text).join(" ")).toBe("then done!)");
  });

  it("emits a trailing fragment without end punctuation as its own sentence", () => {
    const sents = splitSentences([fw("done."), fw("trailing"), fw("words")]);
    expect(sents).toHaveLength(2);
    expect(sents[1]!.map((w) => w.text).join(" ")).toBe("trailing words");
  });

  it("returns no sentences for empty input", () => {
    expect(splitSentences([])).toEqual([]);
  });
});

describe("buildTurns", () => {
  it("assigns a sentence to the duration-weighted majority speaker", () => {
    // A: one long word (1.0s) vs B: two short words (0.1s each) -> A wins
    const segments: Segment[] = [
      {
        start: 0,
        end: 1.2,
        text: "",
        words: [
          { word: "loooong", start: 0, end: 1.0, speaker: "A" },
          { word: "b1", start: 1.0, end: 1.1, speaker: "B" },
          { word: "b2.", start: 1.1, end: 1.2, speaker: "B" },
        ],
      },
    ];
    const turns = buildTurns(segments);
    expect(turns).toHaveLength(1);
    expect(turns[0]!.speaker).toBe("A");
  });

  it("resolves a tie to the first-seen speaker (insertion order)", () => {
    // Equal durations; A seen first.
    const segments: Segment[] = [
      {
        start: 0,
        end: 2,
        text: "",
        words: [
          { word: "a", start: 0, end: 1, speaker: "A" },
          { word: "b.", start: 1, end: 2, speaker: "B" },
        ],
      },
    ];
    const turns = buildTurns(segments);
    expect(turns[0]!.speaker).toBe("A");
  });

  it("merges consecutive same-speaker sentences and splits on speaker change", () => {
    const segments: Segment[] = [
      {
        start: 0,
        end: 4,
        text: "",
        words: [
          { word: "one.", start: 0, end: 1, speaker: "A" },
          { word: "two.", start: 1, end: 2, speaker: "A" },
          { word: "three.", start: 2, end: 3, speaker: "B" },
          { word: "four.", start: 3, end: 4, speaker: "A" },
        ],
      },
    ];
    const turns = buildTurns(segments);
    expect(turns).toHaveLength(3);
    expect(turns[0]!.speaker).toBe("A");
    expect(turns[0]!.text).toBe("one. two.");
    expect(turns[0]!.start).toBe(0);
    expect(turns[0]!.end).toBe(2);
    expect(turns[1]!.speaker).toBe("B");
    expect(turns[1]!.text).toBe("three.");
    expect(turns[2]!.speaker).toBe("A");
    expect(turns[2]!.text).toBe("four.");
  });

  it("collapses whitespace and tidies spacing before punctuation", () => {
    const segments: Segment[] = [
      {
        start: 0,
        end: 2,
        text: "",
        words: [
          { word: "hello", start: 0, end: 1, speaker: "A" },
          { word: ".", start: 1, end: 2, speaker: "A" },
        ],
      },
    ];
    const turns = buildTurns(segments);
    expect(turns[0]!.text).toBe("hello.");
  });

  it("collapses multiple spaces between tokens (a  b -> a b)", () => {
    const segments: Segment[] = [
      {
        start: 0,
        end: 1,
        text: "a  b.",
        speaker: "A",
      },
    ];
    const turns = buildTurns(segments);
    expect(turns[0]!.text).toBe("a b.");
  });
});

describe("buildTurns real-data pin (fraudster_head)", () => {
  const fixture = fraudsterHead as Transcript;

  it("produces a stable, non-empty turn list", () => {
    const turns = buildTurns(fixture.segments);
    expect(turns.length).toBeGreaterThan(0);
    // non-decreasing start times
    for (let i = 1; i < turns.length; i++) {
      expect(turns[i]!.start).toBeGreaterThanOrEqual(turns[i - 1]!.start);
    }
    // merged same-speaker runs: no two adjacent turns share a speaker
    for (let i = 1; i < turns.length; i++) {
      expect(turns[i]!.speaker).not.toBe(turns[i - 1]!.speaker);
    }
    // every turn has text and a resolved speaker
    for (const t of turns) {
      expect(t.text.length).toBeGreaterThan(0);
      expect(t.speaker.length).toBeGreaterThan(0);
    }
  });

  it("is deterministic across runs", () => {
    expect(buildTurns(fixture.segments)).toEqual(buildTurns(fixture.segments));
  });
});
