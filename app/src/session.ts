// Persisted-session types and builder (one JSON file per transcript, written by
// the Rust `save_session` command).

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

/** Assemble a `SessionJson`; id and timestamps are caller-supplied to keep this deterministic. */
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
