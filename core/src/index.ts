/**
 * Public API of the transcription core (used by core tests and scripts/e2e_core.mjs).
 * The app must NOT import this file: it pulls in node:crypto via modelpack.
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

export { flattenWords, splitSentences, buildTurns } from "./transcript.js";

export { assignSpeakers } from "./diarization.js";

export {
  loadTranscript,
  editSentence,
  editParagraph,
  joinSentences,
  splitSentence,
  renameSpeaker,
  reassignSentenceSpeaker,
  exportJSON,
  exportText,
  groupParagraphs,
} from "./editor.js";
export type { EditorWord, EditorSentence, EditorState } from "./editor.js";

export { validateManifest, verifyModelPack, compareSemver } from "./modelpack.js";
