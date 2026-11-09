/**
 * The turn builder. Behaviour matches the retired Python pipeline exactly, so real
 * recorded sidecars build identical turns.
 */

import type { Segment, Word, FlatWord, Turn } from "./types.js";
import { UNKNOWN_SPEAKER } from "./types.js";

/** End punctuation possibly followed by closing quotes/brackets, anchored to end. */
const SENT_END = /[.!?]['"”’)\]]*$/;

/**
 * Flatten document-ordered segments into a flat list of words, resolving a
 * non-null speaker and timestamps for every word.
 *
 * Speaker precedence: word.speaker > segment.speaker > previous word's speaker > UNKNOWN.
 * Timestamp fallback: missing start -> last_end; missing end -> start.
 * Empty/whitespace tokens are dropped. A segment with no words contributes one
 * block built from its (trimmed) text; an empty-text wordless segment is skipped.
 */
export function flattenWords(segments: Segment[]): FlatWord[] {
  const words: FlatWord[] = [];
  let lastSpeaker = UNKNOWN_SPEAKER;
  let lastEnd = 0.0;

  for (const seg of segments) {
    const segSpeaker = seg.speaker ?? null;
    const segWords: Word[] = seg.words ?? [];

    if (segWords.length === 0) {
      const text = (seg.text ?? "").trim();
      if (!text) continue;
      const spk = segSpeaker || lastSpeaker;
      const start = seg.start != null ? Number(seg.start) : lastEnd;
      const end = seg.end != null ? Number(seg.end) : start;
      words.push({ text, start, end, speaker: spk });
      lastSpeaker = spk;
      lastEnd = end;
      continue;
    }

    for (const w of segWords) {
      const tok = (w.word ?? "").trim();
      if (!tok) continue;
      const spk = w.speaker || segSpeaker || lastSpeaker;
      const start = w.start != null ? Number(w.start) : lastEnd;
      const end = w.end != null ? Number(w.end) : start;
      words.push({ text: tok, start, end, speaker: spk });
      lastSpeaker = spk;
      lastEnd = end;
    }
  }

  return words;
}

/**
 * Split a flat word list into sentences. A sentence ends on a word whose text
 * matches end punctuation (optionally trailed by closing quotes/brackets). A
 * trailing fragment with no terminal punctuation becomes its own sentence.
 */
export function splitSentences(words: FlatWord[]): FlatWord[][] {
  const sentences: FlatWord[][] = [];
  let cur: FlatWord[] = [];
  for (const w of words) {
    cur.push(w);
    if (SENT_END.test(w.text)) {
      sentences.push(cur);
      cur = [];
    }
  }
  if (cur.length > 0) {
    sentences.push(cur);
  }
  return sentences;
}

/**
 * Build merged speaker turns. Each sentence is assigned its duration-weighted
 * majority speaker (ties -> first speaker seen in the sentence, insertion order).
 * Consecutive same-speaker sentences merge into one turn. Turn text has collapsed
 * whitespace and tidied spacing before punctuation.
 */
export function buildTurns(segments: Segment[]): Turn[] {
  const sentences = splitSentences(flattenWords(segments));

  const sentTurns: Turn[] = [];
  for (const sent of sentences) {
    if (sent.length === 0) continue;
    // Insertion-ordered map gives tie-break to the first speaker inserted.
    const weights = new Map<string, number>();
    for (const w of sent) {
      const dur = Math.max(w.end - w.start, 1e-3);
      weights.set(w.speaker, (weights.get(w.speaker) ?? 0) + dur);
    }
    let speaker = "";
    let best = -Infinity;
    for (const [spk, weight] of weights) {
      if (weight > best) {
        best = weight;
        speaker = spk;
      }
    }
    sentTurns.push({
      speaker,
      start: sent[0]!.start,
      end: sent[sent.length - 1]!.end,
      text: sent.map((w) => w.text).join(" "),
    });
  }

  const turns: Turn[] = [];
  for (const s of sentTurns) {
    const prev = turns[turns.length - 1];
    if (prev && prev.speaker === s.speaker) {
      prev.text += " " + s.text;
      prev.end = s.end;
    } else {
      turns.push({ ...s });
    }
  }

  for (const t of turns) {
    t.text = t.text
      .split(/\s+/)
      .filter(Boolean)
      .join(" ")
      .replace(/\s+([,.?!;:’'”)])/g, "$1");
  }

  return turns;
}
