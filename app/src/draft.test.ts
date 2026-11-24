import { describe, it, expect } from "vitest";
import { loadTranscript } from "@core/editor";
import type { Transcript } from "@core/types";
import { exportTextWithDisclaimer, exportJSONWithDisclaimer } from "./draft";

const FIXTURE: Transcript = {
  segments: [
    {
      start: 0,
      end: 2,
      text: "The witness arrived.",
      speaker: "SPEAKER_00",
      words: [
        { word: "The", start: 0, end: 0.5, speaker: "SPEAKER_00" },
        { word: "witness", start: 0.5, end: 1.2, speaker: "SPEAKER_00" },
        { word: "arrived.", start: 1.2, end: 2, speaker: "SPEAKER_00" },
      ],
    },
  ],
};

describe("draft exports", () => {
  const state = loadTranscript(FIXTURE);

  it("prefixes the text export with the disclaimer as a commented header", () => {
    const lines = exportTextWithDisclaimer(state, true).split("\n");
    expect(lines.slice(0, 3)).toEqual([
      "# MACHINE-GENERATED DRAFT — automatically transcribed and diarized",
      "# Verify against the source audio before relying on it",
      "# Speaker labels and wording may be inaccurate.",
    ]);
    expect(lines.slice(3)).toContain("The witness arrived.");
  });

  it("adds the disclaimer to the JSON export", () => {
    const json = exportJSONWithDisclaimer(state, true);
    expect(json.disclaimer).toBe(
      "MACHINE-GENERATED DRAFT — automatically transcribed and diarized. " +
        "Verify against the source audio before relying on it. Speaker labels and " +
        "wording may be inaccurate.",
    );
    expect(json.words.map((w) => w.word)).toEqual(["The", "witness", "arrived."]);
  });
});
