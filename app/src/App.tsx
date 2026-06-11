import { useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { loadTranscript } from "@core/editor";
import type { EditorState } from "@core/editor";
import type { Transcript } from "@core/types";
import Editor from "./Editor";

// PR3b: the review-editor flow. Pick audio -> invoke("transcribe") ->
// loadTranscript -> render <Editor>. All edit behaviour is driven by the tested
// core reducer; this component only owns the EditorState and the audio src.

export default function App() {
  const [status, setStatus] = useState<string>("idle");
  const [state, setState] = useState<EditorState | null>(null);
  const [audioSrc, setAudioSrc] = useState<string | undefined>(undefined);

  async function pickAndTranscribe() {
    setState(null);
    setStatus("selecting file…");
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Audio", extensions: ["wav", "m4a", "mp3", "flac"] }],
    });
    if (typeof path !== "string") {
      setStatus("idle");
      return;
    }
    setStatus(`transcribing ${path}…`);
    try {
      const transcript = await invoke<Transcript>("transcribe", { path });
      setState(loadTranscript(transcript));
      setAudioSrc(convertFileSrc(path));
      setStatus(`done — ${transcript.segments.length} segments`);
    } catch (e) {
      setStatus(`error: ${String(e)}`);
    }
  }

  return (
    <main style={{ fontFamily: "system-ui, sans-serif", padding: "1.5rem", maxWidth: 960 }}>
      <h1>Podcast Transcribe</h1>
      <p
        style={{
          background: "#eef7ee",
          border: "1px solid #2e7d32",
          color: "#1b5e20",
          padding: "0.6rem 0.9rem",
          borderRadius: 6,
          fontWeight: 600,
        }}
      >
        100% local — no network access. This build contains no networking code and
        runs fully in airplane mode.
      </p>
      <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
        <button onClick={pickAndTranscribe} style={{ padding: "0.6rem 1rem", fontSize: "1rem" }}>
          Open audio &rarr; transcribe
        </button>
        <span>Status: {status}</span>
      </div>
      {state && (
        <div style={{ marginTop: "1.25rem" }}>
          <Editor state={state} setState={setState} audioSrc={audioSrc} />
        </div>
      )}
    </main>
  );
}
