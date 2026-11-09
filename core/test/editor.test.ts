import { describe, it, expect } from "vitest";
import type { Transcript } from "../src/types.js";
import {
  loadTranscript,
  editSentence,
  editParagraph,
  joinSentences,
  splitSentence,
  replaceAll,
  renameSpeaker,
  reassignSentenceSpeaker,
  exportJSON,
  exportText,
} from "../src/editor.js";
import type { EditorState } from "../src/editor.js";

/**
 * A small synthetic transcript with two speakers and three sentences:
 *   SPEAKER_00: "Hello world."      [0.0 - 1.0]
 *   SPEAKER_00: "How are you?"       [1.0 - 2.5]
 *   SPEAKER_01: "I am fine."         [2.5 - 4.0]
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

/** Collect ordered word texts from an export. */
function wordTexts(state: ReturnType<typeof loadTranscript>): string[] {
  return exportJSON(state).words.map((w) => w.word);
}

describe("loadTranscript", () => {
  it("flattens to document-ordered words with resolved speakers", () => {
    const state = loadTranscript(makeTranscript());
    expect(wordTexts(state)).toEqual([
      "Hello",
      "world.",
      "How",
      "are",
      "you?",
      "I",
      "am",
      "fine.",
    ]);
    const json = exportJSON(state);
    expect(json.words[0]!.speaker).toBe("SPEAKER_00");
    expect(json.words[json.words.length - 1]!.speaker).toBe("SPEAKER_01");
  });

  it("groups into three sentences", () => {
    const state = loadTranscript(makeTranscript());
    expect(state.sentences.length).toBe(3);
    expect(state.sentences.map((s) => s.words.map((w) => w.text).join(" "))).toEqual([
      "Hello world.",
      "How are you?",
      "I am fine.",
    ]);
  });

  it("assigns stable, unique sentence ids", () => {
    const state = loadTranscript(makeTranscript());
    const ids = state.sentences.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    // Reloading the same transcript yields the same ids.
    const state2 = loadTranscript(makeTranscript());
    expect(state2.sentences.map((s) => s.id)).toEqual(ids);
  });
});

describe("editSentence", () => {
  it("rebuilds words with monotonic timestamps strictly within the original span", () => {
    const state = loadTranscript(makeTranscript());
    const sid = state.sentences[0]!.id; // span [0.0, 1.0]
    const next = editSentence(state, sid, "Hi there everyone.");

    const sent = next.sentences.find((s) => s.id === sid)!;
    expect(sent.words.map((w) => w.text)).toEqual(["Hi", "there", "everyone."]);

    // first start === span start, last end === span end
    expect(sent.words[0]!.start).toBeCloseTo(0.0, 9);
    expect(sent.words[sent.words.length - 1]!.end).toBeCloseTo(1.0, 9);

    // monotonic non-decreasing
    for (let i = 0; i < sent.words.length; i++) {
      const w = sent.words[i]!;
      expect(w.end).toBeGreaterThanOrEqual(w.start);
      if (i > 0) {
        expect(w.start).toBeGreaterThanOrEqual(sent.words[i - 1]!.end - 1e-9);
      }
      expect(w.start).toBeGreaterThanOrEqual(0.0 - 1e-9);
      expect(w.end).toBeLessThanOrEqual(1.0 + 1e-9);
    }

    // Adding a missed word to "How are you?" [1.0, 2.5] keeps that span.
    const sid2 = state.sentences[1]!.id;
    const grown = editSentence(state, sid2, "How are you doing?").sentences.find(
      (s) => s.id === sid2,
    )!;
    expect(grown.words.map((w) => w.text)).toEqual(["How", "are", "you", "doing?"]);
    expect(grown.words[0]!.start).toBeCloseTo(1.0, 9);
    expect(grown.words.at(-1)!.end).toBeCloseTo(2.5, 9);
  });

  it("emptying a sentence removes it, neighbors intact", () => {
    const state = loadTranscript(makeTranscript());
    const sid = state.sentences[1]!.id; // middle sentence
    const next = editSentence(state, sid, "   ");
    expect(next.sentences.length).toBe(2);
    expect(next.sentences.map((s) => s.id)).not.toContain(sid);
    expect(wordTexts(next)).toEqual(["Hello", "world.", "I", "am", "fine."]);
  });
});

describe("renameSpeaker", () => {
  it("makes every occurrence export under the new name", () => {
    const state = loadTranscript(makeTranscript());
    const next = renameSpeaker(state, "SPEAKER_00", "Alice");
    const json = exportJSON(next);
    const spk00Words = json.words.slice(0, 5);
    expect(spk00Words.every((w) => w.speaker === "Alice")).toBe(true);
    // SPEAKER_01 untouched
    expect(json.words[json.words.length - 1]!.speaker).toBe("SPEAKER_01");
  });
});

describe("reassignSentenceSpeaker", () => {
  it("changes only the target sentence's speaker", () => {
    const state = loadTranscript(makeTranscript());
    const sid = state.sentences[0]!.id;
    const next = reassignSentenceSpeaker(state, sid, "SPEAKER_09");
    const json = exportJSON(next);
    // first sentence (2 words) is now SPEAKER_09
    expect(json.words[0]!.speaker).toBe("SPEAKER_09");
    expect(json.words[1]!.speaker).toBe("SPEAKER_09");
    // second sentence still SPEAKER_00
    expect(json.words[2]!.speaker).toBe("SPEAKER_00");
    // last sentence still SPEAKER_01
    expect(json.words[json.words.length - 1]!.speaker).toBe("SPEAKER_01");
  });

  it("honors a prior rename when reassigning to a renamed positional id", () => {
    let state = loadTranscript(makeTranscript());
    state = renameSpeaker(state, "SPEAKER_01", "Bob");
    const sid = state.sentences[0]!.id;
    const next = reassignSentenceSpeaker(state, sid, "SPEAKER_01");
    const json = exportJSON(next);
    expect(json.words[0]!.speaker).toBe("Bob");
  });
});

describe("exportJSON", () => {
  it("reflects edited text + renamed speakers and rounds times to 3 decimals", () => {
    let state = loadTranscript(makeTranscript());
    state = renameSpeaker(state, "SPEAKER_00", "Alice");
    // Three equal-length words over [0, 1] land on thirds of a second.
    state = editSentence(state, state.sentences[0]!.id, "one two six");
    const json = exportJSON(state);
    expect(json.words.slice(0, 3)).toEqual([
      { word: "one", start: 0, end: 0.333, speaker: "Alice" },
      { word: "two", start: 0.333, end: 0.667, speaker: "Alice" },
      { word: "six", start: 0.667, end: 1, speaker: "Alice" },
    ]);
  });
});

describe("exportText", () => {
  it("emits a speaker header only when the displayed speaker changes", () => {
    let state = loadTranscript(makeTranscript());
    state = renameSpeaker(state, "SPEAKER_00", "Alice");
    state = renameSpeaker(state, "SPEAKER_01", "Bob");
    const text = exportText(state);

    expect(text.endsWith("\n")).toBe(true);
    expect(text.split("\n").map((line) => line.trimEnd())).toEqual([
      "Alice  [00:00]",
      "Hello world. How are you?",
      "",
      "Bob  [00:02]",
      "I am fine.",
      "",
    ]);
  });

  it("inserts a new header when a mid-stream sentence is reassigned to a different speaker", () => {
    let state = loadTranscript(makeTranscript());
    // reassign middle sentence to a third speaker -> 3 speaker changes
    const midId = state.sentences[1]!.id;
    state = reassignSentenceSpeaker(state, midId, "SPEAKER_02");
    const text = exportText(state);
    const headerCount = (text.match(/\[\d\d:\d\d\]/g) ?? []).length;
    expect(headerCount).toBe(3);
  });
});

describe("reducers", () => {
  const ops: [string, (s: EditorState) => unknown][] = [
    ["editSentence", (s) => editSentence(s, s.sentences[0]!.id, "Totally different text.")],
    [
      "editParagraph",
      (s) => editParagraph(s, [s.sentences[0]!.id, s.sentences[1]!.id], "totally different"),
    ],
    ["joinSentences", (s) => joinSentences(s, [s.sentences[0]!.id, s.sentences[1]!.id])],
    ["splitSentence", (s) => splitSentence(s, s.sentences[1]!.id, 1)],
    ["replaceAll", (s) => replaceAll(s, "world", "earth")],
    ["renameSpeaker", (s) => renameSpeaker(s, "SPEAKER_00", "Alice")],
    ["reassignSentenceSpeaker", (s) => reassignSentenceSpeaker(s, s.sentences[0]!.id, "SPEAKER_09")],
  ];

  it.each(ops)("%s does not mutate its input state", (_name, op) => {
    const state = loadTranscript(makeTranscript());
    const snapshot = JSON.stringify(state);
    op(state);
    expect(JSON.stringify(state)).toBe(snapshot);
  });
});
