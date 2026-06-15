// Pure session-builder for the persisted local transcript library.
//
// The app stores one JSON file per saved transcript. The frontend (App.tsx)
// assembles a `SessionJson` here and hands it to the Rust `save_session`
// command; reopening reads it back. This module is the framework-free builder
// plus the shared TS types, kept side-effect-free so it is unit-testable under
// vitest without Tauri/jsdom (the live IPC wiring is verified manually with
// `tauri dev`).

import type { EditorState } from "@core/editor";

/** A saved transcript session as stored on disk (one JSON file per session). */
export interface SessionJson {
  id: string;
  createdAt: string; // ISO-8601
  updatedAt: string; // ISO-8601
  audioPath: string; // original file path (referenced, not copied)
  audioFilename: string; // basename of audioPath, for display
  diarized: boolean;
  segments: number; // sentence count, for the library list
  editorState: EditorState;
}

/** Library list-row metadata returned by the Rust `list_sessions` command. */
export interface SessionMeta {
  id: string;
  createdAt: string;
  updatedAt: string;
  audioFilename: string;
  diarized: boolean;
  segments: number;
  audioAvailable: boolean;
}

/** Extract the filename from a path (handles both `/` and Windows `\` separators). */
export function basename(path: string): string {
  const lastSlash = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return lastSlash === -1 ? path : path.slice(lastSlash + 1);
}

/**
 * Pure: assemble a `SessionJson` from the editor state plus the metadata the
 * caller supplies. `id` and the timestamps are passed in (rather than generated
 * here) so the function stays pure and deterministic for tests; `audioFilename`
 * is derived from `audioPath` and `segments` from the sentence count.
 */
export function buildSession(input: {
  id: string;
  createdAt: string;
  updatedAt: string;
  audioPath: string;
  diarized: boolean;
  state: EditorState;
}): SessionJson {
  return {
    id: input.id,
    createdAt: input.createdAt,
    updatedAt: input.updatedAt,
    audioPath: input.audioPath,
    audioFilename: basename(input.audioPath),
    diarized: input.diarized,
    segments: input.state.sentences.length,
    editorState: input.state,
  };
}
