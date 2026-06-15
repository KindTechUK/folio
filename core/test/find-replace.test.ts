import { describe, it, expect } from "vitest";
import type { Transcript } from "../src/types.js";
import { loadTranscript, replaceAll } from "../src/editor.js";

/**
 * A small synthetic transcript exercising find/replace:
 *   SPEAKER_00: "John Smith spoke."       [0.0 - 1.5]
 *   SPEAKER_00: "Then john replied."       [1.5 - 3.0]
 *   SPEAKER_01: "Smithson was quiet."      [3.0 - 4.5]
 */
function makeTranscript(): Transcript {
  return {
    segments: [
      {
        start: 0.0,
        end: 1.5,
        text: "John Smith spoke.",
        speaker: "SPEAKER_00",
        words: [
          { word: "John", start: 0.0, end: 0.5, speaker: "SPEAKER_00" },
          { word: "Smith", start: 0.5, end: 1.0, speaker: "SPEAKER_00" },
          { word: "spoke.", start: 1.0, end: 1.5, speaker: "SPEAKER_00" },
        ],
      },
      {
        start: 1.5,
        end: 3.0,
        text: "Then john replied.",
        speaker: "SPEAKER_00",
        words: [
          { word: "Then", start: 1.5, end: 2.0, speaker: "SPEAKER_00" },
          { word: "john", start: 2.0, end: 2.5, speaker: "SPEAKER_00" },
          { word: "replied.", start: 2.5, end: 3.0, speaker: "SPEAKER_00" },
        ],
      },
      {
        start: 3.0,
        end: 4.5,
        text: "Smithson was quiet.",
        speaker: "SPEAKER_01",
        words: [
          { word: "Smithson", start: 3.0, end: 3.5, speaker: "SPEAKER_01" },
          { word: "was", start: 3.5, end: 4.0, speaker: "SPEAKER_01" },
          { word: "quiet.", start: 4.0, end: 4.5, speaker: "SPEAKER_01" },
        ],
      },
    ],
  };
}

function sentenceText(
  state: ReturnType<typeof loadTranscript>,
  id: string,
): string {
  return state.sentences
    .find((s) => s.id === id)!
    .words.map((w) => w.text)
    .join(" ");
}

describe("replaceAll", () => {
  it("case-insensitive multi-sentence replace touches every sentence and counts all matches", () => {
    const state = loadTranscript(makeTranscript());
    // "john" appears as "John" (sent 0) and "john" (sent 1) -> 2 matches.
    const { state: next, count } = replaceAll(state, "john", "Jane");
    expect(count).toBe(2);
    expect(sentenceText(next, next.sentences[0]!.id)).toBe("Jane Smith spoke.");
    expect(sentenceText(next, next.sentences[1]!.id)).toBe("Then Jane replied.");
    // untouched sentence unchanged
    expect(sentenceText(next, next.sentences[2]!.id)).toBe("Smithson was quiet.");
  });

  it("caseSensitive only replaces exact-case occurrences", () => {
    const state = loadTranscript(makeTranscript());
    const { state: next, count } = replaceAll(state, "john", "Jane", {
      caseSensitive: true,
    });
    // Only the lowercase "john" in sentence 1 matches.
    expect(count).toBe(1);
    expect(sentenceText(next, next.sentences[0]!.id)).toBe("John Smith spoke.");
    expect(sentenceText(next, next.sentences[1]!.id)).toBe("Then Jane replied.");
  });

  it("wholeWord does not touch Smithson but a non-wholeWord search does", () => {
    const state = loadTranscript(makeTranscript());

    const whole = replaceAll(state, "Smith", "Jones", { wholeWord: true });
    // Only "Smith" (sent 0) matches, not "Smithson" (sent 2).
    expect(whole.count).toBe(1);
    expect(sentenceText(whole.state, whole.state.sentences[0]!.id)).toBe(
      "John Jones spoke.",
    );
    expect(sentenceText(whole.state, whole.state.sentences[2]!.id)).toBe(
      "Smithson was quiet.",
    );

    const loose = replaceAll(state, "Smith", "Jones");
    // "Smith" (sent 0) + the "Smith" inside "Smithson" (sent 2) -> 2 matches.
    expect(loose.count).toBe(2);
    expect(sentenceText(loose.state, loose.state.sentences[2]!.id)).toBe(
      "Jonesson was quiet.",
    );
  });

  it("matches a multi-word find across the joined-space boundary", () => {
    const state = loadTranscript(makeTranscript());
    const { state: next, count } = replaceAll(state, "John Smith", "J. Smith");
    expect(count).toBe(1);
    expect(sentenceText(next, next.sentences[0]!.id)).toBe("J. Smith spoke.");
  });

  it("empty find leaves the state unchanged with count 0", () => {
    const state = loadTranscript(makeTranscript());
    const { state: next, count } = replaceAll(state, "", "anything");
    expect(count).toBe(0);
    expect(next).toBe(state);
  });

  it("zero matches leaves content unchanged with count 0", () => {
    const state = loadTranscript(makeTranscript());
    const snapshot = JSON.stringify(state);
    const { state: next, count } = replaceAll(state, "absent", "x");
    expect(count).toBe(0);
    expect(JSON.stringify(next)).toBe(snapshot);
  });

  it("re-spreads replaced words within the sentence's original span and inserts $ literals verbatim", () => {
    const state = loadTranscript(makeTranscript());
    const sid = state.sentences[0]!.id; // span [0.0, 1.5]
    // caseSensitive isolates the single "John" in sentence 0 (not lowercase "john").
    const { state: next, count } = replaceAll(state, "John", "$1 Cost", {
      caseSensitive: true,
    });
    expect(count).toBe(1);
    const sent = next.sentences.find((s) => s.id === sid)!;
    // The literal "$1" is inserted, not interpreted as a regex group.
    expect(sent.words.map((w) => w.text)).toEqual([
      "$1",
      "Cost",
      "Smith",
      "spoke.",
    ]);
    // Timings stay within the original [0.0, 1.5] span and are monotonic.
    expect(sent.words[0]!.start).toBeCloseTo(0.0, 9);
    expect(sent.words[sent.words.length - 1]!.end).toBeCloseTo(1.5, 9);
    for (let i = 0; i < sent.words.length; i++) {
      const w = sent.words[i]!;
      expect(w.end).toBeGreaterThanOrEqual(w.start);
      expect(w.start).toBeGreaterThanOrEqual(0.0 - 1e-9);
      expect(w.end).toBeLessThanOrEqual(1.5 + 1e-9);
      if (i > 0) {
        expect(w.start).toBeGreaterThanOrEqual(sent.words[i - 1]!.end - 1e-9);
      }
    }
  });

  it("never deletes a sentence even if the replacement empties its text", () => {
    const state = loadTranscript(makeTranscript());
    const before = state.sentences.length;
    // Replace the only matching token's content with whitespace in a way that,
    // for the matched sentences, could tokenize to nothing. Use a find that
    // matches the entire joined text of sentence 1 ("Then john replied.").
    const { state: next } = replaceAll(state, "Then john replied.", "   ");
    expect(next.sentences.length).toBe(before);
    // Sentence kept its original words (no-delete rule).
    expect(sentenceText(next, next.sentences[1]!.id)).toBe("Then john replied.");
  });

  it("does not mutate the input state", () => {
    const state = loadTranscript(makeTranscript());
    const snapshot = JSON.stringify(state);
    replaceAll(state, "john", "Jane");
    expect(JSON.stringify(state)).toBe(snapshot);
  });
});
