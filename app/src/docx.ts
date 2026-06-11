/**
 * docx — Word (.docx) export for the review editor.
 *
 * Pure-JS, offline (the `docx` package builds the OOXML zip in-process; no
 * network, so the Rust-only zero-egress gate is unaffected). The document
 * always carries the machine-draft disclaimer; the body is either speaker-turn
 * blocks (when diarized) or plain pause-based paragraphs (when not), mirroring
 * the on-screen editor view so a fee-earner gets the same reading in Word.
 */

import {
  Document,
  Packer,
  Paragraph,
  TextRun,
  HeadingLevel,
} from "docx";
import { isTauri } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { writeFile } from "@tauri-apps/plugin-fs";
import { groupParagraphs } from "@core/editor";
import type { EditorState } from "@core/editor";
import { UNKNOWN_SPEAKER } from "@core/types";
import { DRAFT_DISCLAIMER } from "./draft";

function mmss(t: number): string {
  const s = Math.max(0, t);
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
}

function displayName(state: EditorState, id: string): string {
  return state.renameMap[id] ?? id;
}

function sentenceText(words: { text: string }[]): string {
  return words.map((w) => w.text).join(" ");
}

/** A turn = a run of consecutive sentences sharing one positional speaker id. */
function turnBlocks(state: EditorState): Paragraph[] {
  const out: Paragraph[] = [];
  let curSpk: string | null = null;
  let buffer: string[] = [];
  let turnStart = 0;

  const flush = () => {
    if (curSpk === null) return;
    out.push(
      new Paragraph({
        spacing: { after: 120 },
        children: [
          new TextRun({
            text: `${displayName(state, curSpk)}  [${mmss(turnStart)}]`,
            bold: true,
          }),
          new TextRun({ text: "  " + buffer.join(" ") }),
        ],
      }),
    );
  };

  for (const s of state.sentences) {
    const spk = s.words[0]?.speaker ?? UNKNOWN_SPEAKER;
    if (spk !== curSpk) {
      flush();
      curSpk = spk;
      buffer = [];
      turnStart = s.words[0]?.start ?? 0;
    }
    buffer.push(sentenceText(s.words));
  }
  flush();
  return out;
}

/** Plain pause-based paragraph blocks (no speaker labels). */
function paragraphBlocks(state: EditorState): Paragraph[] {
  return groupParagraphs(state.sentences).map(
    (para) =>
      new Paragraph({
        spacing: { after: 120 },
        children: [
          new TextRun(para.map((s) => sentenceText(s.words)).join(" ")),
        ],
      }),
  );
}

/**
 * Build a .docx Blob from editor state. Heading + disclaimer + either speaker
 * turns (`diarized: true`) or plain paragraphs (`diarized: false`).
 */
export async function buildDocx(
  state: EditorState,
  opts: { diarized: boolean },
): Promise<Blob> {
  const body = opts.diarized ? turnBlocks(state) : paragraphBlocks(state);
  const doc = new Document({
    sections: [
      {
        children: [
          new Paragraph({
            heading: HeadingLevel.HEADING_1,
            children: [new TextRun("Transcript")],
          }),
          new Paragraph({
            spacing: { after: 200 },
            children: [new TextRun({ text: DRAFT_DISCLAIMER, italics: true })],
          }),
          ...body,
        ],
      },
    ],
  });
  return Packer.toBlob(doc);
}

/** Browser fallback: trigger a download of `blob` as `name`. */
function browserDownload(blob: Blob, name: string): void {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  URL.revokeObjectURL(a.href);
}

/**
 * Build the .docx and save it. In the Tauri app, open the native save dialog
 * and write via plugin-fs (the dialog grants the chosen path's scope). In a
 * plain browser (dev/tests) fall back to a download. Returns the chosen path,
 * or `null` if the user cancelled.
 */
export async function saveDocx(
  state: EditorState,
  opts: { diarized: boolean },
): Promise<string | null> {
  const blob = await buildDocx(state, opts);
  const fileName = "transcript_edited.docx";

  if (!isTauri()) {
    browserDownload(blob, fileName);
    return null;
  }

  const path = await save({
    defaultPath: fileName,
    filters: [{ name: "Word document", extensions: ["docx"] }],
  });
  if (!path) return null;

  const bytes = new Uint8Array(await blob.arrayBuffer());
  await writeFile(path, bytes);
  return path;
}
