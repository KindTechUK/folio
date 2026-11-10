import { describe, it, expect } from "vitest";
import JSZip from "jszip";
import { buildDocx } from "./docx";
import { loadTranscript } from "@core/editor";
import type { Transcript } from "@core/types";

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
    {
      start: 2,
      end: 4,
      text: "I object loudly.",
      speaker: "SPEAKER_01",
      words: [
        { word: "I", start: 2, end: 2.3, speaker: "SPEAKER_01" },
        { word: "object", start: 2.3, end: 3.1, speaker: "SPEAKER_01" },
        { word: "loudly.", start: 3.1, end: 4, speaker: "SPEAKER_01" },
      ],
    },
  ],
};

/** Concatenated text of every run in word/document.xml. */
async function documentText(blob: Blob): Promise<string> {
  const zip = await JSZip.loadAsync(await blob.arrayBuffer());
  const xml = await zip.file("word/document.xml")!.async("string");
  return [...xml.matchAll(/<w:t(?:\s[^>]*)?>([^<]*)<\/w:t>/g)].map((m) => m[1]).join("");
}

describe("buildDocx", () => {
  const state = loadTranscript(FIXTURE);

  it("writes speaker-labelled turns in diarized mode", async () => {
    const text = await documentText(await buildDocx(state, { diarized: true }));
    expect(text).toContain("MACHINE-GENERATED DRAFT");
    expect(text).toContain("SPEAKER_00");
    expect(text).toContain("The witness arrived.");
    expect(text).toContain("SPEAKER_01");
    expect(text).toContain("I object loudly.");
  });

  it("writes plain paragraphs with no speaker labels in paragraph mode", async () => {
    const text = await documentText(await buildDocx(state, { diarized: false }));
    expect(text).toContain("MACHINE-GENERATED DRAFT");
    expect(text).toContain("The witness arrived. I object loudly.");
    expect(text).not.toContain("SPEAKER_");
  });

  it("does not throw on empty state", async () => {
    await expect(
      buildDocx({ sentences: [], renameMap: {} }, { diarized: false }),
    ).resolves.toBeInstanceOf(Blob);
  });
});
