import { describe, it, expect } from "vitest";
import type { Transcript } from "../src/types.js";
import {
  loadTranscript,
  editParagraph,
  joinSentences,
  splitSentence,
} from "../src/editor.js";

/**
 * Three sentences, all one speaker:
 *   SPEAKER_00: "Hello world."   [0.0 - 1.0]
 *   SPEAKER_00: "How are you?"    [1.0 - 2.5]
 *   SPEAKER_00: "I am fine."      [2.5 - 4.0]
 */
function makeTranscript(): Transcript {
  return {
    segments: [
      {
        start: 0.0,
        end: 2.5,
        text: "Hello world. How are you?",
        speaker: "SPEAKER_00",
        words: [
          { word: "Hello", start: 0.0, end: 0.5, speaker: "SPEAKER_00" },
          { word: "world.", start: 0.5, end: 1.0, speaker: "SPEAKER_00" },
          { word: "How", start: 1.0, end: 1.4, speaker: "SPEAKER_00" },
          { word: "are", start: 1.4, end: 1.9, speaker: "SPEAKER_00" },
          { word: "you?", start: 1.9, end: 2.5, speaker: "SPEAKER_00" },
        ],
      },
      {
        start: 2.5,
        end: 4.0,
        text: "I am fine.",
        speaker: "SPEAKER_01",
        words: [
          { word: "I", start: 2.5, end: 2.9, speaker: "SPEAKER_01" },
          { word: "am", start: 2.9, end: 3.4, speaker: "SPEAKER_01" },
          { word: "fine.", start: 3.4, end: 4.0, speaker: "SPEAKER_01" },
        ],
      },
    ],
  };
}

describe("editParagraph", () => {
  it("re-spreads new text across the run's anchor span and collapses to one sentence", () => {
    const state = loadTranscript(makeTranscript());
    // First two sentences form one paragraph: anchor [0.0, 2.5].
    const ids = [state.sentences[0]!.id, state.sentences[1]!.id];
    const next = editParagraph(state, ids, "Greetings everyone how do you do today?");

    // Run collapsed to one sentence keeping the first sentence's id.
    expect(next.sentences.length).toBe(2);
    const para = next.sentences[0]!;
    expect(para.id).toBe(state.sentences[0]!.id);
    expect(para.words.map((w) => w.text)).toEqual([
      "Greetings",
      "everyone",
      "how",
      "do",
      "you",
      "do",
      "today?",
    ]);

    // Anchor span held: first start === 0.0, last end === 2.5.
    expect(para.words[0]!.start).toBeCloseTo(0.0, 9);
    expect(para.words.at(-1)!.end).toBeCloseTo(2.5, 9);

    // Monotonic non-decreasing inside the span.
    for (let i = 0; i < para.words.length; i++) {
      const w = para.words[i]!;
      expect(w.end).toBeGreaterThanOrEqual(w.start - 1e-9);
      if (i > 0) {
        expect(w.start).toBeGreaterThanOrEqual(para.words[i - 1]!.end - 1e-9);
      }
      expect(w.start).toBeGreaterThanOrEqual(0.0 - 1e-9);
      expect(w.end).toBeLessThanOrEqual(2.5 + 1e-9);
    }

    // The untouched third sentence is intact.
    expect(next.sentences[1]!.words.map((w) => w.text)).toEqual([
      "I",
      "am",
      "fine.",
    ]);
  });

  it("preserves the run's speaker (first word's speaker)", () => {
    const state = loadTranscript(makeTranscript());
    const ids = [state.sentences[0]!.id, state.sentences[1]!.id];
    const next = editParagraph(state, ids, "All new words here.");
    expect(next.sentences[0]!.words.every((w) => w.speaker === "SPEAKER_00")).toBe(
      true,
    );
  });

  it("blank text deletes the whole run", () => {
    const state = loadTranscript(makeTranscript());
    const ids = [state.sentences[0]!.id, state.sentences[1]!.id];
    const next = editParagraph(state, ids, "   ");
    expect(next.sentences.length).toBe(1);
    expect(next.sentences[0]!.words.map((w) => w.text)).toEqual([
      "I",
      "am",
      "fine.",
    ]);
  });

  it("non-contiguous ids return state unchanged", () => {
    const state = loadTranscript(makeTranscript());
    // sent-0 and sent-2 are not contiguous (sent-1 between them).
    const ids = [state.sentences[0]!.id, state.sentences[2]!.id];
    const before = JSON.stringify(state);
    const next = editParagraph(state, ids, "should not apply");
    expect(JSON.stringify(next)).toBe(before);
  });

  it("unknown ids return state unchanged", () => {
    const state = loadTranscript(makeTranscript());
    const before = JSON.stringify(state);
    const next = editParagraph(state, ["nope-1"], "should not apply");
    expect(JSON.stringify(next)).toBe(before);
  });

  it("does not mutate the input state", () => {
    const state = loadTranscript(makeTranscript());
    const snapshot = JSON.stringify(state);
    editParagraph(
      state,
      [state.sentences[0]!.id, state.sentences[1]!.id],
      "totally different content",
    );
    expect(JSON.stringify(state)).toBe(snapshot);
  });
});

describe("joinSentences", () => {
  it("concatenates a contiguous run into one sentence with unchanged timings", () => {
    const state = loadTranscript(makeTranscript());
    const ids = [state.sentences[0]!.id, state.sentences[1]!.id];
    const next = joinSentences(state, ids);

    expect(next.sentences.length).toBe(2);
    const joined = next.sentences[0]!;
    expect(joined.id).toBe(state.sentences[0]!.id);
    // Words concatenated in document order.
    expect(joined.words.map((w) => w.text)).toEqual([
      "Hello",
      "world.",
      "How",
      "are",
      "you?",
    ]);
    // Timings unchanged from the originals.
    expect(joined.words.map((w) => [w.start, w.end])).toEqual([
      [0.0, 0.5],
      [0.5, 1.0],
      [1.0, 1.4],
      [1.4, 1.9],
      [1.9, 2.5],
    ]);
  });

  it("only touches the run; later sentences are untouched", () => {
    const state = loadTranscript(makeTranscript());
    const ids = [state.sentences[0]!.id, state.sentences[1]!.id];
    const next = joinSentences(state, ids);
    const tail = next.sentences[1]!;
    expect(tail.id).toBe(state.sentences[2]!.id);
    expect(tail.words.map((w) => w.text)).toEqual(["I", "am", "fine."]);
  });

  it("non-contiguous ids return state unchanged", () => {
    const state = loadTranscript(makeTranscript());
    const ids = [state.sentences[0]!.id, state.sentences[2]!.id];
    const before = JSON.stringify(state);
    const next = joinSentences(state, ids);
    expect(JSON.stringify(next)).toBe(before);
  });

  it("does not mutate the input state", () => {
    const state = loadTranscript(makeTranscript());
    const snapshot = JSON.stringify(state);
    joinSentences(state, [state.sentences[0]!.id, state.sentences[1]!.id]);
    expect(JSON.stringify(state)).toBe(snapshot);
  });
});

describe("splitSentence", () => {
  it("splits one sentence at the boundary, both halves keep their timings", () => {
    const state = loadTranscript(makeTranscript());
    // sent-1 = "How are you?" (3 words: 1.0-1.4, 1.4-1.9, 1.9-2.5). Split at index 1.
    const sid = state.sentences[1]!.id;
    const next = splitSentence(state, sid, 1);

    expect(next.sentences.length).toBe(4);
    const head = next.sentences[1]!;
    const tail = next.sentences[2]!;
    expect(head.id).toBe(sid);
    expect(tail.id).toBe(`${sid}-b`);

    expect(head.words.map((w) => w.text)).toEqual(["How"]);
    expect(tail.words.map((w) => w.text)).toEqual(["are", "you?"]);

    // Timings intact (no re-spread).
    expect(head.words.map((w) => [w.start, w.end])).toEqual([[1.0, 1.4]]);
    expect(tail.words.map((w) => [w.start, w.end])).toEqual([
      [1.4, 1.9],
      [1.9, 2.5],
    ]);
  });

  it("keeps document order / monotonic starts after split", () => {
    const state = loadTranscript(makeTranscript());
    const sid = state.sentences[1]!.id;
    const next = splitSentence(state, sid, 1);
    const flat = next.sentences.flatMap((s) => s.words);
    for (let i = 1; i < flat.length; i++) {
      expect(flat[i]!.start).toBeGreaterThanOrEqual(flat[i - 1]!.start - 1e-9);
    }
  });

  it("out-of-range index is a no-op", () => {
    const state = loadTranscript(makeTranscript());
    const sid = state.sentences[1]!.id; // 3 words
    const before = JSON.stringify(state);
    expect(JSON.stringify(splitSentence(state, sid, 0))).toBe(before);
    expect(JSON.stringify(splitSentence(state, sid, 3))).toBe(before);
    expect(JSON.stringify(splitSentence(state, sid, 99))).toBe(before);
  });

  it("unknown id is a no-op", () => {
    const state = loadTranscript(makeTranscript());
    const before = JSON.stringify(state);
    expect(JSON.stringify(splitSentence(state, "nope", 1))).toBe(before);
  });

  it("does not mutate the input state", () => {
    const state = loadTranscript(makeTranscript());
    const snapshot = JSON.stringify(state);
    splitSentence(state, state.sentences[0]!.id, 2);
    expect(JSON.stringify(state)).toBe(snapshot);
  });
});
