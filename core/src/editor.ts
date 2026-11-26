/**
 * Headless, DOM-free reducer for the review editor. Every exported operation
 * returns a NEW state and never mutates its input.
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
  /** What the model wrote, when the vocabulary file replaced it. */
  original?: string;
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

/** Resolve a positional speaker id to its display name. */
function displayName(renameMap: Record<SpeakerId, string>, id: SpeakerId): string {
  return renameMap[id] ?? id;
}

/** Deep-clone a sentence so mutations never leak back into the input state. */
function cloneSentence(s: EditorSentence): EditorSentence {
  return { id: s.id, words: s.words.map((w) => ({ ...w })) };
}

function cloneState(state: EditorState): EditorState {
  return {
    sentences: state.sentences.map(cloneSentence),
    renameMap: { ...state.renameMap },
  };
}

/** Load a transcript into editor state: one sentence per `splitSentences` group, ids `sent-<i>`. */
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
      ...(w.original != null ? { original: w.original } : {}),
    })),
  }));
  return { sentences, renameMap: {} };
}

/** Tokenize free text into words: collapse whitespace, trim, drop empties. */
function tokenize(text: string): string[] {
  return text
    .replace(/\s+/g, " ")
    .trim()
    .split(" ")
    .filter(Boolean);
}

/** Spread `span` seconds from `start` across `tokens` by character length. */
function spread(
  start: number,
  span: number,
  tokens: string[],
  speaker: SpeakerId,
): EditorWord[] {
  const total = tokens.join("").length || 1;
  let acc = 0;
  const rebuilt: EditorWord[] = [];
  for (const tok of tokens) {
    const st = start + span * (acc / total);
    acc += tok.length;
    const en = start + span * (acc / total);
    rebuilt.push({ text: tok, start: st, end: en, speaker });
  }
  return rebuilt;
}

/**
 * Spread `[spanStart, spanEnd]` across `tokens` by character length, all assigned
 * `speaker`. A degenerate span (end <= start) becomes 0.3s per token so timings stay
 * monotonic.
 */
function respread(
  spanStart: number,
  spanEnd: number,
  tokens: string[],
  speaker: SpeakerId,
): EditorWord[] {
  const span =
    spanEnd - spanStart > 0
      ? spanEnd - spanStart
      : Math.max(0.3 * tokens.length, 0.3);
  return spread(spanStart, span, tokens, speaker);
}

/**
 * `[oldIndex, newIndex]` pairs for the words an edit kept: a longest common
 * subsequence of the two word lists, compared by exact text.
 */
function keptPairs(oldTexts: string[], newTexts: string[]): Array<[number, number]> {
  // Most edits touch a few words, so match the shared prefix and suffix directly.
  let head = 0;
  while (
    head < oldTexts.length &&
    head < newTexts.length &&
    oldTexts[head] === newTexts[head]
  ) {
    head++;
  }
  let tail = 0;
  while (
    tail < oldTexts.length - head &&
    tail < newTexts.length - head &&
    oldTexts[oldTexts.length - 1 - tail] === newTexts[newTexts.length - 1 - tail]
  ) {
    tail++;
  }

  const a = oldTexts.slice(head, oldTexts.length - tail);
  const b = newTexts.slice(head, newTexts.length - tail);
  // lcs[i][j]: length of the longest common subsequence of a[i..] and b[j..].
  const lcs = Array.from({ length: a.length + 1 }, () => new Int32Array(b.length + 1));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i]![j] =
        a[i] === b[j] ? lcs[i + 1]![j + 1]! + 1 : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
    }
  }

  const pairs: Array<[number, number]> = [];
  for (let k = 0; k < head; k++) pairs.push([k, k]);
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      pairs.push([head + i, head + j]);
      i++;
      j++;
    } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) {
      i++;
    } else {
      j++;
    }
  }
  for (let k = tail; k > 0; k--) {
    pairs.push([oldTexts.length - k, newTexts.length - k]);
  }
  return pairs;
}

/**
 * The words for edited text `tokens` that replaces `old`, all assigned `speaker`.
 * Words the edit kept stay as they were: their timing and `original`. A run of new
 * words that replaces old words takes those words' time; a run inserted between kept
 * words takes the gap between them (no duration if there is none). With nothing kept,
 * the new words spread across `[spanStart, spanEnd]`.
 */
function retime(
  old: EditorWord[],
  tokens: string[],
  spanStart: number,
  spanEnd: number,
  speaker: SpeakerId,
): EditorWord[] {
  const pairs = keptPairs(
    old.map((w) => w.text),
    tokens,
  );
  if (pairs.length === 0) return respread(spanStart, spanEnd, tokens, speaker);

  const words: EditorWord[] = [];
  let oldAt = 0;
  let newAt = 0;
  // A sentinel pair past both ends flushes the run after the last kept word.
  for (const [keptOld, keptNew] of [...pairs, [old.length, tokens.length] as [number, number]]) {
    const added = tokens.slice(newAt, keptNew);
    if (added.length > 0) {
      const replaced = keptOld > oldAt;
      const from = replaced
        ? old[oldAt]!.start
        : oldAt > 0
          ? old[oldAt - 1]!.end
          : spanStart;
      const to = replaced
        ? old[keptOld - 1]!.end
        : keptOld < old.length
          ? old[keptOld]!.start
          : spanEnd;
      words.push(...spread(from, Math.max(to - from, 0), added, speaker));
    }
    if (keptOld < old.length) words.push({ ...old[keptOld]!, speaker });
    oldAt = keptOld + 1;
    newAt = keptNew + 1;
  }
  return words;
}

/**
 * Replace a sentence's text. Words the edit didn't touch keep their timing and
 * `original`; changed and inserted words are timed from their neighbours (see
 * `retime`). Blank text deletes the sentence.
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

  const toks = tokenize(newText);

  if (toks.length === 0) {
    next.sentences.splice(idx, 1);
    return next;
  }

  sentence.words = retime(orig, toks, s, e, speaker);
  return next;
}

/** Indexes of `sentenceIds` if they name consecutive sentences in document order, else null. */
function contiguousRun(
  sentences: EditorSentence[],
  sentenceIds: string[],
): number[] | null {
  if (sentenceIds.length === 0) return null;
  const indices: number[] = [];
  for (const id of sentenceIds) {
    const idx = sentences.findIndex((s) => s.id === id);
    if (idx === -1) return null;
    indices.push(idx);
  }
  for (let i = 1; i < indices.length; i++) {
    if (indices[i]! !== indices[i - 1]! + 1) return null;
  }
  return indices;
}

/**
 * Paragraph-scope edit. Re-tokenize `newText` and time it against the run's words
 * (see `retime`): untouched words keep their timing and `original`, and with nothing
 * kept the text spreads across the run's ANCHOR span — the first listed sentence's
 * first-word `start` to the last listed sentence's last-word `end`. The whole
 * contiguous run collapses to ONE sentence whose id is the FIRST sentence's id (so
 * highlight/edit ids stay stable) and whose speaker is the first word's speaker.
 * Blank/whitespace `newText` deletes the entire run.
 *
 * Non-contiguous, unknown or empty ids return the state unchanged.
 */
export function editParagraph(
  state: EditorState,
  sentenceIds: string[],
  newText: string,
): EditorState {
  const next = cloneState(state);
  const run = contiguousRun(next.sentences, sentenceIds);
  if (run === null) return next;

  const startIdx = run[0]!;
  const endIdx = run[run.length - 1]!;
  const runSentences = next.sentences.slice(startIdx, endIdx + 1);

  const toks = tokenize(newText);
  if (toks.length === 0) {
    next.sentences.splice(startIdx, run.length);
    return next;
  }

  let spanStart = 0;
  let spanEnd = 0;
  let speaker: SpeakerId = UNKNOWN_SPEAKER;
  for (const s of runSentences) {
    if (s.words.length > 0) {
      spanStart = s.words[0]!.start;
      speaker = s.words[0]!.speaker;
      break;
    }
  }
  for (let i = runSentences.length - 1; i >= 0; i--) {
    const w = runSentences[i]!.words;
    if (w.length > 0) {
      spanEnd = w[w.length - 1]!.end;
      break;
    }
  }

  const collapsed: EditorSentence = {
    id: runSentences[0]!.id,
    words: retime(
      runSentences.flatMap((s) => s.words),
      toks,
      spanStart,
      spanEnd,
      speaker,
    ),
  };
  next.sentences.splice(startIdx, run.length, collapsed);
  return next;
}

export interface ReplaceOptions {
  /** Match case exactly (default false: case-insensitive). */
  caseSensitive?: boolean;
  /** Require whole-word boundaries around the match (default false). */
  wholeWord?: boolean;
}

/** Escape every regex metacharacter so `find` is matched as a literal string. */
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Literal, per-sentence find/replace (a multi-word `find` matches across word gaps).
 * `replace` is inserted verbatim (no `$`-substitution). In a changed sentence only the
 * replaced words are re-timed (see `retime`); a replacement that would empty a sentence
 * is skipped. Empty `find` is a
 * no-op. Returns the new state and the number of matches replaced.
 */
export function replaceAll(
  state: EditorState,
  find: string,
  replace: string,
  opts: ReplaceOptions = {},
): { state: EditorState; count: number } {
  if (find === "") return { state, count: 0 };

  const escaped = escapeRegExp(find);
  const pattern = opts.wholeWord ? `\\b${escaped}\\b` : escaped;
  const flags = opts.caseSensitive ? "g" : "gi";
  const regex = new RegExp(pattern, flags);
  const replacer = (): string => replace;

  const next = cloneState(state);
  let count = 0;

  for (const sentence of next.sentences) {
    const orig = sentence.words;
    const joined = orig.map((w) => w.text).join(" ");

    regex.lastIndex = 0;
    let matches = 0;
    while (regex.exec(joined) !== null) {
      matches++;
    }
    if (matches === 0) continue;

    const newText = joined.replace(regex, replacer);
    const toks = tokenize(newText);
    if (toks.length === 0) continue; // no-delete rule

    const s = orig.length > 0 ? orig[0]!.start : 0;
    const e = orig.length > 0 ? orig[orig.length - 1]!.end : 0;
    const speaker = orig.length > 0 ? orig[0]!.speaker : UNKNOWN_SPEAKER;
    sentence.words = retime(orig, toks, s, e, speaker);
    count += matches;
  }

  return { state: next, count };
}

/**
 * Concatenate a CONTIGUOUS run of sentences into ONE sentence (id = the first sentence's
 * id), preserving every word's timing in document order. No re-spread. Used to join
 * sentences into a paragraph without retyping. Non-contiguous ids, an unknown id, or an
 * empty list return the state UNCHANGED.
 */
export function joinSentences(
  state: EditorState,
  sentenceIds: string[],
): EditorState {
  const next = cloneState(state);
  const run = contiguousRun(next.sentences, sentenceIds);
  if (run === null) return next;

  const startIdx = run[0]!;
  const runSentences = next.sentences.slice(startIdx, startIdx + run.length);
  const merged: EditorSentence = {
    id: runSentences[0]!.id,
    words: runSentences.flatMap((s) => s.words),
  };
  next.sentences.splice(startIdx, run.length, merged);
  return next;
}

/**
 * Split one sentence into TWO at `wordIndex`: words `[0, wordIndex)` keep the original id,
 * words `[wordIndex, end)` move to a new sentence with the first free id among `${id}-b`,
 * `${id}-b2`, …. Each half keeps its word timings (no re-spread). An out-of-range
 * `wordIndex` (<= 0 or >= word count) or an unknown id returns the state unchanged.
 */
export function splitSentence(
  state: EditorState,
  sentenceId: string,
  wordIndex: number,
): EditorState {
  const next = cloneState(state);
  const idx = next.sentences.findIndex((s) => s.id === sentenceId);
  if (idx === -1) return next;

  const sentence = next.sentences[idx]!;
  if (wordIndex <= 0 || wordIndex >= sentence.words.length) return next;

  const head: EditorSentence = {
    id: sentence.id,
    words: sentence.words.slice(0, wordIndex),
  };
  const taken = new Set(next.sentences.map((s) => s.id));
  let tailId = `${sentence.id}-b`;
  for (let n = 2; taken.has(tailId); n++) tailId = `${sentence.id}-b${n}`;
  const tail: EditorSentence = {
    id: tailId,
    words: sentence.words.slice(wordIndex),
  };
  next.sentences.splice(idx, 1, head, tail);
  return next;
}

/**
 * Global, positional speaker rename: all words whose positional id is `origId` will
 * DISPLAY as `displayNameValue`.
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
 * applies at display time.
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
 * Export as `{ words: [{ word, start, end, speaker }] }`.
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

/**
 * Group sentences into pause-based paragraphs for plain (no-speaker) QA reading.
 *
 * Starts a new paragraph when the gap between the previous sentence's last-word
 * `end` and the current sentence's first-word `start` exceeds `gapSeconds`, OR
 * once the current paragraph already holds `maxSentences` sentences (a readable
 * cap for audio with no clear pauses). Pure: returns an array of sentence groups
 * in document order; empty input → `[]`. Sentences with no words never start a
 * gap break on their own (treated as gap 0) but still count toward the cap.
 */
export function groupParagraphs(
  sentences: EditorSentence[],
  gapSeconds = 0.75,
  maxSentences = 6,
): EditorSentence[][] {
  const paragraphs: EditorSentence[][] = [];
  let current: EditorSentence[] = [];
  let prevEnd: number | null = null;

  for (const s of sentences) {
    const firstStart = s.words[0]?.start;
    const gap =
      prevEnd !== null && firstStart !== undefined ? firstStart - prevEnd : 0;
    const tooLong = current.length >= maxSentences;
    if (current.length > 0 && (gap > gapSeconds || tooLong)) {
      paragraphs.push(current);
      current = [];
    }
    current.push(s);
    const lastEnd = s.words[s.words.length - 1]?.end;
    if (lastEnd !== undefined) prevEnd = lastEnd;
  }
  if (current.length > 0) paragraphs.push(current);
  return paragraphs;
}

/** Format seconds as mm:ss. */
function mmss(t: number): string {
  const total = Math.floor(t);
  const mm = Math.floor(total / 60);
  const ss = total % 60;
  return `${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
}

/** Script export: a `<speaker>  [mm:ss]` header at each display-speaker change, then the words. */
export function exportText(state: EditorState): string {
  const blocks: { header: string; words: string[] }[] = [];
  let cur: string | null = null;
  for (const w of orderedWords(state)) {
    const spk = displayName(state.renameMap, w.speaker);
    if (spk !== cur) {
      blocks.push({ header: `${spk}  [${mmss(w.start)}]`, words: [] });
      cur = spk;
    }
    blocks[blocks.length - 1]!.words.push(w.text);
  }
  return blocks.map((b) => `${b.header}\n${b.words.join(" ")}\n`).join("\n");
}
