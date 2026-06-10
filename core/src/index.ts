/**
 * Public API of the transcription core — framework-independent logic that drops into
 * the Tauri/Vibe frontend. No DOM, no Python, no network: pure data transforms plus
 * the model-pack verification used by the offline model-update system.
 */

export type {
  Word,
  Segment,
  Transcript,
  FlatWord,
  Turn,
  SpeakerId,
  SpeakerSegment,
} from "./types.js";
export { UNKNOWN_SPEAKER } from "./types.js";

// Turn builder — port of the Python pipeline's flatten/split/build_turns.
export { flattenWords, splitSentences, buildTurns } from "./transcript.js";

// Diarization speaker-assignment — overlap mapping (not the ML model).
export { assignSpeakers } from "./diarization.js";

// Headless review-editor reducer — sentence edit / relabel / export.
export {
  loadTranscript,
  editSentence,
  renameSpeaker,
  reassignSentenceSpeaker,
  exportJSON,
  exportText,
} from "./editor.js";

// Signed/versioned model-pack validation & verification.
export { validateManifest, verifyModelPack, compareSemver } from "./modelpack.js";
