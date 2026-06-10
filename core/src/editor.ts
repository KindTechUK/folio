/**
 * editor — a headless, DOM-free reducer for the review editor.
 *
 * Pure data in, pure data out: the Tauri/Vibe frontend drives this state model and
 * we unit-test behavior without a browser. This is a faithful port of the edit logic
 * embedded in the `karaoke_editor.html` prototype.
 *
 * Word timestamps are a playback aid, not an edit surface. When a sentence's text is
 * edited we re-spread its ORIGINAL [start, end] span across the new tokens, weighted by
 * character length (per the prototype's `rebuild`).
 */

import type { Transcript, SpeakerId } from "./types.js";
import { UNKNOWN_SPEAKER } from "./types.js";
import { flattenWords, splitSentences } from "./transcript.js";

/** A single editable word inside a sentence. `speaker` is the positional (origin) id. */
export interface EditorWord {
  text: string;
  start: number;
  end: number;
  speaker: SpeakerId;
}

/**
 * A sentence: a stable id plus its document-ordered words. All words in a sentence
 * share one speaker (sentence-granular diarization). `id` is positional/stable across
 * reloads of the same transcript (`sent-<index>`).
 */
export interface EditorSentence {
  id: string;
  words: EditorWord[];
}

/**
 * The editor state. `sentences` is the live, document-ordered content. `renameMap`
 * maps a positional speaker id to a chosen display name (global, positional rename).
 */
export interface EditorState {
  sentences: EditorSentence[];
  renameMap: Record<SpeakerId, string>;
}

/** Sentence end test, ported verbatim from the prototype. */
const isEnd = (w: string): boolean => /[.!?]["')\]]*$/.test(w);

/** Resolve a positional speaker id to its display name. */
function displayName(renameMap: Record<SpeakerId, string>, id: SpeakerId): string {
  return renameMap[id] ?? id;
}

/** Deep-clone a sentence so mutations never leak back into the input state. */
function cloneSentence(s: EditorSentence): EditorSentence {
  return { id: s.id, words: s.words.map((w) => ({ ...w })) };
}

/** Clone the whole state (sentences + rename map) for immutable updates. */
function cloneState(state: EditorState): EditorState {
  return {
    sentences: state.sentences.map(cloneSentence),
    renameMap: { ...state.renameMap },
  };
}

/**
 * Load a transcript into editor state. Reuses `flattenWords`/`splitSentences` from
 * `./transcript` (resolved speaker = word.speaker || segment speaker || previous ||
 * UNKNOWN), then groups the flat words into sentences with stable positional ids.
 */
export function loadTranscript(t: Transcript): EditorState {
  const flat = flattenWords(t.segments);
  const grouped = splitSentences(flat);
  const sentences: EditorSentence[] = grouped.map((words, i) => ({
    id: `sent-${i}`,
    words: words.map((w) => ({
      text: w.text,
      start: w.start,
      end: w.end,
      speaker: w.speaker || UNKNOWN_SPEAKER,
    })),
  }));
  return { sentences, renameMap: {} };
}

/**
 * Re-tokenize `newText` and re-spread the sentence's original [start, end] span across
 * the new words by character length (prototype `rebuild`). Blank/whitespace text DELETES
 * the sentence. Returns a NEW state; the input is not mutated.
 */
export function editSentence(
  state: EditorState,
  sentenceId: string,
  newText: string,
): EditorState {
  const next = cloneState(state);
  const idx = next.sentences.findIndex((s) => s.id === sentenceId);
  if (idx === -1) return next;

  const sentence = next.sentences[idx]!;
  const orig = sentence.words;
  const s = orig.length > 0 ? orig[0]!.start : 0;
  const e = orig.length > 0 ? orig[orig.length - 1]!.end : 0;
  const speaker = orig.length > 0 ? orig[0]!.speaker : UNKNOWN_SPEAKER;

  const toks = newText
    .replace(/\s+/g, " ")
    .trim()
    .split(" ")
    .filter(Boolean);

  if (toks.length === 0) {
    // Sentence deleted.
    next.sentences.splice(idx, 1);
    return next;
  }

  const span = e - s > 0 ? e - s : Math.max(0.3 * toks.length, 0.3);
  const total = toks.join("").length || 1;
  let acc = 0;
  const rebuilt: EditorWord[] = [];
  for (const tok of toks) {
    const st = s + span * (acc / total);
    acc += tok.length;
    const en = s + span * (acc / total);
    rebuilt.push({ text: tok, start: st, end: en, speaker });
  }
  sentence.words = rebuilt;
  return next;
}

/**
 * Global, positional speaker rename: all words whose positional id is `origId` will
 * DISPLAY as `displayNameValue`. Returns a NEW state; input not mutated.
 */
export function renameSpeaker(
  state: EditorState,
  origId: SpeakerId,
  displayNameValue: string,
): EditorState {
  const next = cloneState(state);
  next.renameMap[origId] = displayNameValue;
  return next;
}

/**
 * Reassign the speaker of ONLY the target sentence's words (per-sentence diarization
 * fix). `newSpeakerId` is a positional id, so a prior `renameSpeaker` for that id still
 * applies at display time. Returns a NEW state; input not mutated.
 */
export function reassignSentenceSpeaker(
  state: EditorState,
  sentenceId: string,
  newSpeakerId: SpeakerId,
): EditorState {
  const next = cloneState(state);
  const sentence = next.sentences.find((s) => s.id === sentenceId);
  if (!sentence) return next;
  for (const w of sentence.words) {
    w.speaker = newSpeakerId;
  }
  return next;
}

/** All live words in document order. */
function orderedWords(state: EditorState): EditorWord[] {
  const out: EditorWord[] = [];
  for (const s of state.sentences) {
    for (const w of s.words) out.push(w);
  }
  return out;
}

/**
 * Export to the prototype JSON shape: `{ words: [{ word, start, end, speaker }] }`.
 * start/end rounded to 3 decimals; speaker is the DISPLAY name.
 */
export function exportJSON(state: EditorState): {
  words: { word: string; start: number; end: number; speaker: string }[];
} {
  const words = orderedWords(state).map((w) => ({
    word: w.text,
    start: +w.start.toFixed(3),
    end: +w.end.toFixed(3),
    speaker: displayName(state.renameMap, w.speaker),
  }));
  return { words };
}

/** Format seconds as mm:ss (prototype). */
function mmss(t: number): string {
  const total = Math.floor(t);
  const mm = Math.floor(total / 60);
  const ss = total % 60;
  return `${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
}

/**
 * Export to the prototype script/text format. Walk words in order; whenever the DISPLAY
 * speaker changes, emit a header `\n\n${speaker}  [${mm:ss}]\n`, then append each word +
 * ' '. Trim, end with a newline.
 */
export function exportText(state: EditorState): string {
  const words = orderedWords(state);
  let out = "";
  let cur: string | null = null;
  for (const w of words) {
    const spk = displayName(state.renameMap, w.speaker);
    if (spk !== cur) {
      out += `\n\n${spk}  [${mmss(w.start)}]\n`;
      cur = spk;
    }
    out += w.text + " ";
  }
  return out.trim() + "\n";
}
