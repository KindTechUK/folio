import { useEffect, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { loadTranscript } from "@core/editor";
import type { EditorState } from "@core/editor";
import type { Transcript } from "@core/types";
import { progressView } from "./progress";
import type { Progress, ProgressView } from "./progress";
import Editor from "./Editor";
import Models from "./Models";

// PR3b: the review-editor flow. Pick audio -> invoke("transcribe") ->
// loadTranscript -> render <Editor>. All edit behaviour is driven by the tested
// core reducer; this component only owns the EditorState and the audio src.
// PR3c: a "Models" view that lists/verifies signed model packs (trust boundary).

type View = "review" | "models";

// Mirrors the Rust `EncryptionStatus` struct returned by the
// `volume_encryption_status` command. `state` is "encrypted" | "unencrypted" |
// "unknown"; we warn unless it is confirmed "encrypted".
interface EncryptionStatus {
  state: string;
  detail: string;
}

export default function App() {
  const [view, setView] = useState<View>("review");
  const [status, setStatus] = useState<string>("idle");
  const [state, setState] = useState<EditorState | null>(null);
  const [audioSrc, setAudioSrc] = useState<string | undefined>(undefined);
  // The raw filesystem path of the picked audio (NOT the convertFileSrc URL),
  // retained so "Delete matter" can remove the source file — the only artifact.
  const [loadedPath, setLoadedPath] = useState<string | null>(null);
  // Volume-encryption banner state. `null` until the on-mount check resolves;
  // `dismissed` hides it for the session.
  const [encryption, setEncryption] = useState<EncryptionStatus | null>(null);
  const [encryptionDismissed, setEncryptionDismissed] = useState(false);
  // Speaker detection is OPT-IN (default off): on real audio its quality hurt
  // more than it helped, so the default is plain paragraph QA mode.
  const [detectSpeakers, setDetectSpeakers] = useState(false);
  const [speakerCount, setSpeakerCount] = useState("");
  // Whether the CURRENTLY loaded transcript was diarized (drives the editor view).
  const [diarized, setDiarized] = useState(false);
  // Determinate progress for the in-flight transcription (null = not running).
  const [progress, setProgress] = useState<ProgressView | null>(null);

  // On mount, check whether the volume holding the app is encrypted. This is a
  // soft advisory: on macOS/dev it returns the "Windows only" variant. Tauri
  // invoke is not jsdom-testable, so this wiring is thin and verified manually.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const result = await invoke<EncryptionStatus>(
          "volume_encryption_status",
          { path: "." },
        );
        if (!cancelled) setEncryption(result);
      } catch {
        // Never let an advisory check disrupt the app — leave the banner hidden.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Wipe the matter's only on-disk artifact (the source audio) and clear the
  // loaded transcript. The engine writes no temp files, so the picked path is
  // the entire footprint to remove.
  async function deleteMatter() {
    if (
      !window.confirm(
        "Delete the source audio for this matter and clear the transcript? This cannot be undone.",
      )
    ) {
      return;
    }
    try {
      const report = await invoke<{ deleted: string[]; failed: unknown[] }>(
        "delete_matter",
        { paths: loadedPath ? [loadedPath] : [] },
      );
      setState(null);
      setAudioSrc(undefined);
      setLoadedPath(null);
      setStatus(`matter deleted — ${report.deleted.length} file(s) removed`);
    } catch (e) {
      setStatus(`error deleting matter: ${String(e)}`);
    }
  }

  async function pickAndTranscribe() {
    setState(null);
    setProgress(null);
    setStatus("selecting file…");
    const path = await open({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "Audio",
          // Symphonia covers these in-app (decoded to 16 kHz mono); the command
          // resamples/downmixes as needed.
          extensions: ["wav", "m4a", "mp3", "flac", "mp4", "mov", "aac", "ogg"],
        },
      ],
    });
    if (typeof path !== "string") {
      setStatus("idle");
      return;
    }
    const parsedCount = parseInt(speakerCount, 10);
    const num_speakers =
      detectSpeakers && Number.isFinite(parsedCount) && parsedCount > 0
        ? parsedCount
        : undefined;

    // Determinate progress bar driven by backend `transcribe-progress` events.
    // The command is async/off-thread, so the webview stays responsive while it
    // runs. We unlisten in `finally` (and on the error path) so no listener leaks.
    const startedAt = Date.now();
    setStatus("transcribing…");
    setProgress({ fraction: 0, label: "Starting…" });
    const unlisten = await listen<Progress>("transcribe-progress", (event) => {
      const elapsedSec = (Date.now() - startedAt) / 1000;
      setProgress(progressView(event.payload, elapsedSec));
    });

    try {
      const transcript = await invoke<Transcript>("transcribe", {
        path,
        diarize: detectSpeakers,
        num_speakers,
      });
      setState(loadTranscript(transcript));
      setDiarized(detectSpeakers);
      setAudioSrc(convertFileSrc(path));
      setLoadedPath(path);
      setStatus(`done — ${transcript.segments.length} segments`);
    } catch (e) {
      setStatus(`error: ${String(e)}`);
    } finally {
      unlisten();
      setProgress(null);
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
      {encryption &&
        encryption.state !== "encrypted" &&
        !encryptionDismissed && (
          <div
            role="alert"
            data-testid="encryption-warning"
            style={{
              display: "flex",
              gap: 12,
              alignItems: "flex-start",
              background: "#fff8e1",
              border: "1px solid #c08a00",
              color: "#7a5800",
              padding: "0.6rem 0.9rem",
              borderRadius: 6,
              margin: "0 0 0.75rem",
            }}
          >
            <span style={{ flex: 1 }}>
              This volume is not confirmed BitLocker-encrypted. For
              client-confidential matters, store and run on an encrypted volume.{" "}
              <span style={{ color: "#9a7400" }}>({encryption.detail})</span>
            </span>
            <button
              aria-label="dismiss encryption warning"
              data-testid="dismiss-encryption-warning"
              onClick={() => setEncryptionDismissed(true)}
              style={{
                background: "none",
                border: "none",
                color: "#7a5800",
                cursor: "pointer",
                fontSize: 18,
                lineHeight: 1,
                padding: 0,
              }}
            >
              ×
            </button>
          </div>
        )}
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
            {state && (
              <button
                data-testid="delete-matter"
                onClick={() => void deleteMatter()}
                style={{
                  padding: "0.5rem 0.9rem",
                  marginLeft: "auto",
                  color: "#7f1d1d",
                  background: "#fee2e2",
                  border: "1px solid #b91c1c",
                  borderRadius: 6,
                  cursor: "pointer",
                }}
              >
                Delete matter
              </button>
            )}
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
          {progress && (
            <div
              data-testid="transcribe-progress"
              style={{ margin: "0.9rem 0 0", display: "flex", flexDirection: "column", gap: 4, maxWidth: 480 }}
            >
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: 13 }}>
                <span>{progress.label}</span>
                <span>
                  {Math.round(progress.fraction * 100)}%
                  {progress.etaSeconds !== undefined &&
                    ` · ~${Math.ceil(progress.etaSeconds)}s left`}
                </span>
              </div>
              <progress
                value={progress.fraction}
                max={1}
                style={{ width: "100%", height: 14 }}
              />
            </div>
          )}
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
