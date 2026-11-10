// Runs a real recorded sidecar slice through the editor reducers and exports.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";

import type { Transcript } from "../src/index.js";
import {
  loadTranscript,
  editSentence,
  renameSpeaker,
  reassignSentenceSpeaker,
  exportJSON,
  exportText,
} from "../src/index.js";

const fixture = fileURLToPath(new URL("./fixtures/fraudster_head.json", import.meta.url));
const transcript = JSON.parse(readFileSync(fixture, "utf8")) as Transcript;

describe("e2e: real sidecar through the editor", () => {
  it("loads, edits, renames, reassigns, and exports (editor module composes)", () => {
    const s0 = loadTranscript(transcript);
    expect(s0.sentences.length).toBeGreaterThan(0);

    const firstId = s0.sentences[0]!.id;
    const origSpeaker = s0.sentences[0]!.words[0]!.speaker;
    const span = {
      start: s0.sentences[0]!.words[0]!.start,
      end: s0.sentences[0]!.words.at(-1)!.end,
    };

    // 1) edit the first sentence: retype it; timestamps re-spread within the original span
    const s1 = editSentence(s0, firstId, "the witness clearly stated otherwise here.");
    const edited = s1.sentences.find((x) => x.id === firstId)!;
    expect(edited.words.map((w) => w.text).join(" ")).toContain("witness");
    expect(edited.words[0]!.start).toBeCloseTo(span.start, 3);
    expect(edited.words.at(-1)!.end).toBeCloseTo(span.end, 3);
    for (let i = 1; i < edited.words.length; i++) {
      expect(edited.words[i]!.start).toBeGreaterThanOrEqual(edited.words[i - 1]!.start);
    }
    // input state was not mutated
    expect(s0.sentences.find((x) => x.id === firstId)!.words.map((w) => w.text).join(" "))
      .not.toContain("witness");

    // 2) global rename: every occurrence of the first speaker displays as "Witness"
    const s2 = renameSpeaker(s1, origSpeaker, "Witness");
    const json2 = exportJSON(s2);
    expect(json2.words.some((w) => w.speaker === "Witness")).toBe(true);
    expect(json2.words.some((w) => w.speaker === origSpeaker)).toBe(false);

    // 3) per-sentence reassignment changes ONLY that sentence
    const targetId = s2.sentences.at(-1)!.id;
    const beforeOthers = s2.sentences
      .filter((x) => x.id !== targetId)
      .map((x) => x.words.map((w) => w.speaker).join(","));
    const s3 = reassignSentenceSpeaker(s2, targetId, "SPEAKER_99");
    expect(s3.sentences.find((x) => x.id === targetId)!.words.every((w) => w.speaker === "SPEAKER_99"))
      .toBe(true);
    const afterOthers = s3.sentences
      .filter((x) => x.id !== targetId)
      .map((x) => x.words.map((w) => w.speaker).join(","));
    expect(afterOthers).toEqual(beforeOthers);

    // 4) exports are non-empty and reflect the live edits
    const text = exportText(s3);
    expect(text).toContain("Witness");
    expect(text.trim().length).toBeGreaterThan(0);
    expect(exportJSON(s3).words.length).toBeGreaterThan(0);
  });
});
