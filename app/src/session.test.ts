import { describe, it, expect } from "vitest";
import { basename, buildSession } from "./session";
import type { EditorState } from "@core/editor";

// Pure builder tests. The Rust `save_session`/`list_sessions` IPC wiring in
// App.tsx is NOT exercised here (Tauri commands are unavailable in jsdom); that
// round-trip is verified manually with `tauri dev`.

describe("basename", () => {
  it("extracts the filename from a POSIX path", () => {
    expect(basename("/Users/x/clip.m4a")).toBe("clip.m4a");
  });

  it("extracts the filename from a Windows path", () => {
    expect(basename("C:\\Users\\x\\clip.mp3")).toBe("clip.mp3");
  });

  it("returns a bare filename unchanged", () => {
    expect(basename("clip.wav")).toBe("clip.wav");
  });

  it("returns an empty string for an empty path", () => {
    expect(basename("")).toBe("");
  });
});

describe("buildSession", () => {
  const state: EditorState = {
    sentences: [
      {
        id: "sent-0",
        words: [{ text: "hello", start: 0, end: 0.5, speaker: "spk-0" }],
      },
      {
        id: "sent-1",
        words: [{ text: "world", start: 0.5, end: 1, speaker: "spk-0" }],
      },
    ],
    renameMap: {},
  };

  it("assembles a SessionJson, deriving the filename and segment count", () => {
    const session = buildSession({
      id: "sess-123",
      createdAt: "2026-06-15T10:00:00.000Z",
      updatedAt: "2026-06-15T10:05:00.000Z",
      audioPath: "/Users/x/clip.m4a",
      diarized: true,
      state,
      notes: "Ask about the lease.",
    });

    expect(session).toEqual({
      id: "sess-123",
      createdAt: "2026-06-15T10:00:00.000Z",
      updatedAt: "2026-06-15T10:05:00.000Z",
      audioPath: "/Users/x/clip.m4a",
      audioFilename: "clip.m4a",
      diarized: true,
      segments: 2,
      editorState: state,
      notes: "Ask about the lease.",
    });
  });
});
