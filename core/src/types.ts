/**
 * The transcript JSON sidecar contract — the stable interface between the ASR/diarization
 * engine and everything downstream (turn builder, editor, exporters).
 *
 * Same shape as the retired Python pipeline's sidecars, so real recorded sidecars
 * work as test fixtures unchanged.
 *
 * Word-level timestamps are a PLAYBACK AID ONLY, not an edit surface. Native engine
 * timestamps (±200-500ms) are sufficient; forced alignment is not required.
 */

/** A positional speaker label, e.g. "SPEAKER_00". Null/absent means unknown. */
export type SpeakerId = string;

/** A single recognized word with its timing and (optionally) its diarized speaker. */
export interface Word {
  word: string;
  start: number;
  end: number;
  speaker?: SpeakerId | null;
  /** Optional alignment confidence, if the engine provides it. */
  score?: number;
  /** What the model wrote, when the vocabulary file replaced it. */
  original?: string;
}

/** An engine segment. May or may not carry word-level detail. */
export interface Segment {
  start: number;
  end: number;
  text: string;
  speaker?: SpeakerId | null;
  words?: Word[];
}

/** The top-level sidecar: `{ "segments": [...] }`. */
export interface Transcript {
  segments: Segment[];
}

/** A flattened word with a resolved speaker and timestamps (see `flattenWords`). */
export interface FlatWord {
  text: string;
  start: number;
  end: number;
  speaker: SpeakerId;
  /** What the model wrote, when the vocabulary file replaced it. */
  original?: string;
}

/**
 * A speaker turn at sentence granularity: consecutive same-speaker sentences merged.
 * `text` has collapsed whitespace and tidied spacing before punctuation.
 */
export interface Turn {
  speaker: SpeakerId;
  start: number;
  end: number;
  text: string;
}

/** A diarization output span: speaker X is talking from `start` to `end`. */
export interface SpeakerSegment {
  speaker: SpeakerId;
  start: number;
  end: number;
}

export const UNKNOWN_SPEAKER = "UNKNOWN";
