import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

// MINIMAL smoke UI for PR2. This is intentionally NOT the review editor
// (that is PR3, driven by core/src/editor.ts). Its only job is to prove the
// IPC -> engine wiring links and runs, and to surface the zero-egress posture.

type Transcript = {
  segments: Array<{
    start: number;
    end: number;
    text: string;
    speaker?: string | null;
    words: Array<{ word: string; start: number; end: number; speaker?: string | null }>;
  }>;
};

export default function App() {
  const [status, setStatus] = useState<string>("idle");
  const [result, setResult] = useState<string>("");

  async function pickAndTranscribe() {
    setResult("");
    setStatus("selecting file…");
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Audio", extensions: ["wav"] }],
    });
    if (typeof path !== "string") {
      setStatus("idle");
      return;
    }
    setStatus(`transcribing ${path}…`);
    try {
      // num_speakers omitted -> engine auto-detects.
      const transcript = await invoke<Transcript>("transcribe", { path });
      setResult(JSON.stringify(transcript, null, 2));
      setStatus(`done — ${transcript.segments.length} segments`);
    } catch (e) {
      setStatus(`error: ${String(e)}`);
    }
  }

  return (
    <main style={{ fontFamily: "system-ui, sans-serif", padding: "1.5rem", maxWidth: 900 }}>
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
      <button onClick={pickAndTranscribe} style={{ padding: "0.6rem 1rem", fontSize: "1rem" }}>
        Open audio &rarr; transcribe
      </button>
      <p>Status: {status}</p>
      {result && (
        <pre
          style={{
            background: "#1e1e1e",
            color: "#d4d4d4",
            padding: "1rem",
            borderRadius: 6,
            overflow: "auto",
            maxHeight: 480,
          }}
        >
          {result}
        </pre>
      )}
    </main>
  );
}
