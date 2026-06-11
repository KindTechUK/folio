import { describe, it, expect } from "vitest";
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

/** First two bytes of a zip (and thus a .docx) are "PK". */
async function startsWithPK(blob: Blob): Promise<boolean> {
  const head = new Uint8Array(await blob.slice(0, 2).arrayBuffer());
  return head[0] === 0x50 && head[1] === 0x4b;
}

describe("buildDocx", () => {
  const state = loadTranscript(FIXTURE);

  it("returns a non-empty .docx (PK zip) in diarized mode", async () => {
    const blob = await buildDocx(state, { diarized: true });
    expect(blob.size).toBeGreaterThan(0);
    expect(await startsWithPK(blob)).toBe(true);
  });

  it("returns a non-empty .docx (PK zip) in paragraph mode", async () => {
    const blob = await buildDocx(state, { diarized: false });
    expect(blob.size).toBeGreaterThan(0);
    expect(await startsWithPK(blob)).toBe(true);
  });

  it("does not throw on empty state", async () => {
    await expect(
      buildDocx({ sentences: [], renameMap: {} }, { diarized: false }),
    ).resolves.toBeInstanceOf(Blob);
  });
});
