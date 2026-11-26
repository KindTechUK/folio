// Issue #28: an edit re-times only the words it changes; untouched words keep their
// native timing and the record of what the model wrote.
import { describe, it, expect } from "vitest";

import { loadTranscript, editSentence, replaceAll } from "../src/editor.js";
import type { Transcript } from "../src/types.js";

// "I met Shivaun on Tuesday." where the vocabulary file turned "Siobhan" into
// "Shivaun", and a long pause comes before "Tuesday.".
function makeTranscript(): Transcript {
  return {
    segments: [
      {
        start: 0.0,
        end: 4.6,
        text: "I met Shivaun on Tuesday.",
        speaker: "SPEAKER_00",
        words: [
          { word: "I", start: 0.0, end: 0.2, speaker: "SPEAKER_00" },
          { word: "met", start: 0.3, end: 0.6, speaker: "SPEAKER_00" },
          {
            word: "Shivaun",
            start: 0.7,
            end: 1.4,
            speaker: "SPEAKER_00",
            original: "Siobhan",
          },
          { word: "on", start: 1.5, end: 1.6, speaker: "SPEAKER_00" },
          { word: "Tuesday.", start: 4.0, end: 4.6, speaker: "SPEAKER_00" },
        ],
      },
    ],
  };
}

const I = { text: "I", start: 0.0, end: 0.2, speaker: "SPEAKER_00" };
const MET = { text: "met", start: 0.3, end: 0.6, speaker: "SPEAKER_00" };
const SHIVAUN = {
  text: "Shivaun",
  start: 0.7,
  end: 1.4,
  speaker: "SPEAKER_00",
  original: "Siobhan",
};
const ON = { text: "on", start: 1.5, end: 1.6, speaker: "SPEAKER_00" };
const TUESDAY = { text: "Tuesday.", start: 4.0, end: 4.6, speaker: "SPEAKER_00" };

describe("edits keep untouched words", () => {
  it("Find & Replace of one word leaves every other word's timing and original", () => {
    const state = loadTranscript(makeTranscript());
    const { state: next, count } = replaceAll(state, "Tuesday", "Wednesday");

    expect(count).toBe(1);
    const words = next.sentences[0]!.words;
    expect(words.map((w) => w.text)).toEqual(["I", "met", "Shivaun", "on", "Wednesday."]);
    expect(words.slice(0, 4)).toEqual([I, MET, SHIVAUN, ON]);
  });

  it("editSentence that changes one word re-times only that word, within its neighbours' gap", () => {
    const state = loadTranscript(makeTranscript());
    const sid = state.sentences[0]!.id;
    const words = editSentence(state, sid, "I met Siobhán on Tuesday.").sentences[0]!.words;

    expect(words.map((w) => w.text)).toEqual(["I", "met", "Siobhán", "on", "Tuesday."]);
    expect([words[0], words[1], words[3], words[4]]).toEqual([I, MET, ON, TUESDAY]);
    // "met" ends at 0.6 and "on" starts at 1.5.
    expect(words[2]!.start).toBeGreaterThanOrEqual(0.6);
    expect(words[2]!.end).toBeLessThanOrEqual(1.5);
    expect(words[2]!.end).toBeGreaterThanOrEqual(words[2]!.start);
  });

  it("an edit that inserts a word places it between its neighbours' times", () => {
    const state = loadTranscript(makeTranscript());
    const sid = state.sentences[0]!.id;
    const words = editSentence(state, sid, "I met Shivaun again on Tuesday.").sentences[0]!
      .words;

    expect(words.map((w) => w.text)).toEqual(["I", "met", "Shivaun", "again", "on", "Tuesday."]);
    expect(words[2]).toEqual(SHIVAUN);
    expect(words[4]).toEqual(ON);
    // "Shivaun" ends at 1.4 and "on" starts at 1.5.
    expect(words[3]!.start).toBeGreaterThanOrEqual(1.4);
    expect(words[3]!.end).toBeLessThanOrEqual(1.5);
    expect(words[3]!.end).toBeGreaterThanOrEqual(words[3]!.start);
  });
});
