import { useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { loadTranscript } from "@core/editor";
import type { EditorState } from "@core/editor";
import type { Transcript } from "@core/types";
import Editor from "./Editor";
import Models from "./Models";

// PR3b: the review-editor flow. Pick audio -> invoke("transcribe") ->
// loadTranscript -> render <Editor>. All edit behaviour is driven by the tested
// core reducer; this component only owns the EditorState and the audio src.
// PR3c: a "Models" view that lists/verifies signed model packs (trust boundary).

type View = "review" | "models";

export default function App() {
  const [view, setView] = useState<View>("review");
  const [status, setStatus] = useState<string>("idle");
  const [state, setState] = useState<EditorState | null>(null);
  const [audioSrc, setAudioSrc] = useState<string | undefined>(undefined);
  // Speaker detection is OPT-IN (default off): on real audio its quality hurt
  // more than it helped, so the default is plain paragraph QA mode.
  const [detectSpeakers, setDetectSpeakers] = useState(false);
  const [speakerCount, setSpeakerCount] = useState("");
  // Whether the CURRENTLY loaded transcript was diarized (drives the editor view).
  const [diarized, setDiarized] = useState(false);

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
    const parsedCount = parseInt(speakerCount, 10);
    const num_speakers =
      detectSpeakers && Number.isFinite(parsedCount) && parsedCount > 0
        ? parsedCount
        : undefined;
    try {
      const transcript = await invoke<Transcript>("transcribe", {
        path,
        diarize: detectSpeakers,
        num_speakers,
      });
      setState(loadTranscript(transcript));
      setDiarized(detectSpeakers);
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
      <nav style={{ display: "flex", gap: 8, margin: "0.5rem 0 1rem" }}>
        <button
          onClick={() => setView("review")}
          aria-pressed={view === "review"}
          style={{ padding: "0.4rem 0.9rem", fontWeight: view === "review" ? 700 : 400 }}
        >
          Review
        </button>
        <button
          onClick={() => setView("models")}
          aria-pressed={view === "models"}
          style={{ padding: "0.4rem 0.9rem", fontWeight: view === "models" ? 700 : 400 }}
        >
          Models
        </button>
      </nav>

      {view === "review" ? (
        <>
          <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
            <button onClick={pickAndTranscribe} style={{ padding: "0.6rem 1rem", fontSize: "1rem" }}>
              Open audio &rarr; transcribe
            </button>
            <span>Status: {status}</span>
          </div>
          <div
            style={{
              display: "flex",
              gap: 12,
              alignItems: "center",
              margin: "0.6rem 0 0",
              fontSize: 14,
            }}
          >
            <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
              <input
                type="checkbox"
                data-testid="detect-speakers"
                checked={detectSpeakers}
                onChange={(e) => setDetectSpeakers(e.target.checked)}
              />
              Detect speakers (slower, experimental)
            </label>
            {detectSpeakers && (
              <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
                Speakers
                <input
                  type="number"
                  min={1}
                  data-testid="speaker-count"
                  placeholder="auto"
                  value={speakerCount}
                  onChange={(e) => setSpeakerCount(e.target.value)}
                  style={{ width: 64 }}
                />
              </label>
            )}
          </div>
          {state && (
            <div style={{ marginTop: "1.25rem" }}>
              <Editor
                state={state}
                setState={setState}
                audioSrc={audioSrc}
                diarized={diarized}
              />
            </div>
          )}
        </>
      ) : (
        <Models />
      )}
    </main>
  );
}
