import { useEffect, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { loadTranscript } from "@core/editor";
import type { EditorState } from "@core/editor";
import type { Transcript } from "@core/types";
import { progressView } from "./progress";
import type { Progress, ProgressView } from "./progress";
import {
  initHistory,
  pushHistory,
  undo,
  redo,
  canUndo,
  canRedo,
} from "./history";
import type { History } from "./history";
import { buildSession } from "./session";
import type { SessionJson } from "./session";
import Editor from "./Editor";
import Models, { selectAsrModel } from "./Models";
import type { PackStatus } from "./Models";
import Library from "./Library";
import Sidebar from "./Sidebar";
import type { View } from "./Sidebar";
import { Banner, Button, Chip } from "./ui";

const HEADINGS: Record<View, { title: string; subtitle: string }> = {
  library: { title: "Matters", subtitle: "Recordings and transcripts stored on this device." },
  transcribe: {
    title: "Import recording",
    subtitle: "Create a private transcript without uploading the recording.",
  },
  models: {
    title: "Models & settings",
    subtitle: "Manage the signed model packs available to Folio.",
  },
};

// Session id generator: prefers the platform UUID; falls back to a
// timestamp+random id where `crypto.randomUUID` is unavailable.
function newId(): string {
  return (
    globalThis.crypto?.randomUUID?.() ??
    `s-${Date.now()}-${Math.floor(Math.random() * 1e9)}`
  );
}

// Mirrors the Rust `EncryptionStatus` struct returned by `volume_encryption_status`.
interface EncryptionStatus {
  state: "encrypted" | "unencrypted" | "unknown";
  detail: string;
}

export default function App() {
  const [view, setView] = useState<View>("library");
  const [status, setStatus] = useState<string>("idle");
  // Edit history wraps the EditorState so all edits are undoable. `state` is
  // derived from the history's present; `setState` records a new entry.
  const [history, setHistory] = useState<History<EditorState> | null>(null);
  const state = history?.present ?? null;
  const setState = (next: EditorState) =>
    setHistory((h) => (h ? pushHistory(h, next) : initHistory(next)));
  const onUndo = () => setHistory((h) => (h ? undo(h) : h));
  const onRedo = () => setHistory((h) => (h ? redo(h) : h));
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
  const [installedAsrModel, setInstalledAsrModel] = useState<string | null>(null);
  const [debugAsrModel, setDebugAsrModel] = useState<string | null>(null);
  const [modelsChecked, setModelsChecked] = useState(false);
  const asrModel = debugAsrModel ?? installedAsrModel;
  // Metadata for the currently loaded library session (null = unsaved/none).
  // Drives debounced auto-save and ties a loaded matter to its saved entry.
  const [currentSessionMeta, setCurrentSessionMeta] = useState<{
    id: string;
    createdAt: string;
    audioPath: string;
    diarized: boolean;
  } | null>(null);

  function updateModelStatus(packs: PackStatus[]) {
    setInstalledAsrModel(selectAsrModel(packs));
    setModelsChecked(true);
  }

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

  useEffect(() => {
    void Promise.all([
      invoke<PackStatus[]>("list_model_packs"),
      invoke<string | null>("debug_asr_model_override"),
    ])
      .then(([packs, debugModel]) => {
        setInstalledAsrModel(selectAsrModel(packs));
        setDebugAsrModel(debugModel);
        setModelsChecked(true);
      })
      .catch(() => setModelsChecked(true));
  }, []);

  // Debounced auto-save: whenever the edited state changes for a saved session,
  // coalesce rapid edits into a single write ~800ms after the last keystroke.
  // A failed autosave must never crash the UI, so errors are swallowed.
  useEffect(() => {
    if (!state || !currentSessionMeta) return;
    const meta = currentSessionMeta;
    const timer = setTimeout(() => {
      void invoke("save_session", {
        session: buildSession({
          id: meta.id,
          createdAt: meta.createdAt,
          updatedAt: new Date().toISOString(),
          audioPath: meta.audioPath,
          diarized: meta.diarized,
          state,
        }),
      }).catch(() => {});
    }, 800);
    return () => clearTimeout(timer);
  }, [state, currentSessionMeta]);

  // Open a saved session from the Library: load its JSON, restore the editor
  // and audio, and switch to the review view. Audio may be missing if the
  // referenced source file was moved/deleted.
  async function openSession(id: string) {
    try {
      const res = await invoke<{ session: SessionJson; audioAvailable: boolean }>(
        "load_session",
        { id },
      );
      setHistory(initHistory(res.session.editorState));
      setDiarized(res.session.diarized);
      setCurrentSessionMeta({
        id: res.session.id,
        createdAt: res.session.createdAt,
        audioPath: res.session.audioPath,
        diarized: res.session.diarized,
      });
      setAudioSrc(
        res.audioAvailable ? convertFileSrc(res.session.audioPath) : undefined,
      );
      setLoadedPath(res.audioAvailable ? res.session.audioPath : null);
      setView("transcribe");
      setStatus(
        res.audioAvailable
          ? `opened ${res.session.audioFilename}`
          : `opened ${res.session.audioFilename} (audio file not found)`,
      );
    } catch (e) {
      setStatus(`error opening transcript: ${String(e)}`);
    }
  }

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
      // Purge the saved library entry for this matter too, so deleting a matter
      // leaves no persisted transcript behind.
      if (currentSessionMeta) {
        await invoke("delete_session", { id: currentSessionMeta.id });
      }
      setHistory(null);
      setAudioSrc(undefined);
      setLoadedPath(null);
      setCurrentSessionMeta(null);
      setStatus(`matter deleted — ${report.deleted.length} file(s) removed`);
    } catch (e) {
      setStatus(`error deleting matter: ${String(e)}`);
    }
  }

  async function pickAndTranscribe() {
    if (!asrModel) {
      setStatus("error: install a verified transcription model before choosing a recording");
      return;
    }
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
      setStatus(state ? "review open" : "idle");
      return;
    }
    setHistory(null);
    setCurrentSessionMeta(null);
    setProgress(null);
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
        model: asrModel,
      });
      // Build the editor state once and reuse it for both the live history and
      // the saved copy, so the persisted session matches what is on screen.
      const editorState = loadTranscript(transcript);
      const id = newId();
      const createdAt = new Date().toISOString();
      setHistory(initHistory(editorState));
      setDiarized(detectSpeakers);
      setAudioSrc(convertFileSrc(path));
      setLoadedPath(path);
      setCurrentSessionMeta({ id, createdAt, audioPath: path, diarized: detectSpeakers });
      void invoke("save_session", {
        session: buildSession({
          id,
          createdAt,
          updatedAt: createdAt,
          audioPath: path,
          diarized: detectSpeakers,
          state: editorState,
        }),
      }).catch(() => {});
      setStatus(`done — ${transcript.segments.length} segments`);
    } catch (e) {
      setStatus(`error: ${String(e)}`);
    } finally {
      unlisten();
      setProgress(null);
    }
  }

  const showEncryptionWarning =
    encryption && encryption.state !== "encrypted" && !encryptionDismissed;
  const heading =
    view === "transcribe" && state
      ? {
          title: "Review transcript",
          subtitle: "Listen, correct, and verify the machine-generated draft.",
        }
      : HEADINGS[view];
  const statusTone = status.startsWith("error")
    ? "danger"
    : status.startsWith("done") || status.startsWith("opened")
      ? "privacy"
      : "neutral";

  return (
    <div style={{ display: "flex", minHeight: "100vh", background: "var(--canvas)" }}>
      <Sidebar view={view} onNavigate={setView} />

      <main
        style={{
          flex: 1,
          minWidth: 0,
          padding: "var(--space-5)",
          overflowX: "hidden",
        }}
      >
        <header style={{ maxWidth: 1040, marginBottom: "var(--space-5)" }}>
          <h1 style={{ margin: 0, fontSize: "var(--text-2xl)" }}>
            {heading.title}
          </h1>
          <p style={{ margin: "var(--space-2) 0 0", color: "var(--slate-500)" }}>
            {heading.subtitle}
          </p>
        </header>

        {showEncryptionWarning && (
          <Banner
            variant="warning"
            role="alert"
            data-testid="encryption-warning"
            onDismiss={() => setEncryptionDismissed(true)}
            dismissLabel="dismiss encryption warning"
            dismissTestid="dismiss-encryption-warning"
            style={{ maxWidth: 1040, marginBottom: "var(--space-4)" }}
          >
            This volume is not confirmed BitLocker-encrypted. For
            client-confidential matters, store and run on an encrypted volume.{" "}
            <span style={{ opacity: 0.8 }}>({encryption.detail})</span>
          </Banner>
        )}

        <div style={{ maxWidth: 1040 }}>
          {view === "transcribe" ? (
            <>
              {!state && !progress && (
                <section
                  style={{
                    padding: "var(--space-6)",
                    background: "var(--surface)",
                    border: "1px solid var(--slate-200)",
                    borderRadius: "var(--radius-lg)",
                    maxWidth: 720,
                  }}
                >
                  <h2 style={{ margin: 0, fontSize: "var(--text-lg)" }}>
                    Choose a recording to transcribe
                  </h2>
                  <p
                    style={{
                      color: "var(--slate-500)",
                      margin: "var(--space-2) 0 var(--space-5)",
                      maxWidth: 560,
                    }}
                  >
                    Supported audio and video files are processed entirely on this
                    Windows device and saved to Matters automatically.
                  </p>
                  {modelsChecked && !asrModel ? (
                    <Banner
                      variant="warning"
                      role="alert"
                      style={{ marginBottom: "var(--space-4)" }}
                    >
                      A verified transcription model must be installed before
                      importing a recording.{" "}
                      <Button
                        size="sm"
                        onClick={() => setView("models")}
                        style={{ marginLeft: "var(--space-2)" }}
                      >
                        Open Models &amp; settings
                      </Button>
                    </Banner>
                  ) : (
                    <Button
                      variant="primary"
                      onClick={pickAndTranscribe}
                      disabled={!modelsChecked}
                      style={{ marginBottom: "var(--space-4)" }}
                    >
                      {modelsChecked ? "Choose recording" : "Checking transcription model…"}
                    </Button>
                  )}
                  {status.startsWith("error") && asrModel && (
                    <Banner
                      variant="warning"
                      role="alert"
                      style={{ marginBottom: "var(--space-4)" }}
                    >
                      {status.replace(/^error:\s*/, "")}
                    </Banner>
                  )}

                  <div
                    style={{
                      paddingTop: "var(--space-4)",
                      borderTop: "1px solid var(--slate-200)",
                      fontSize: "var(--text-sm)",
                      color: "var(--slate-600)",
                    }}
                  >
                    <div style={{ fontWeight: 600, marginBottom: "var(--space-2)" }}>
                      Optional review setup
                    </div>
                    <label
                      style={{
                        display: "flex",
                        gap: 8,
                        alignItems: "center",
                        cursor: "pointer",
                      }}
                      title="Detect and label distinct speakers. Slower; best on clear 1–2 speaker audio."
                    >
                      <input
                        type="checkbox"
                        data-testid="detect-speakers"
                        checked={detectSpeakers}
                        onChange={(e) => setDetectSpeakers(e.target.checked)}
                      />
                      Detect and label speakers (slower)
                    </label>
                    {detectSpeakers && (
                      <label
                        style={{
                          display: "flex",
                          gap: 8,
                          alignItems: "center",
                          margin: "var(--space-3) 0 0 24px",
                        }}
                      >
                        Expected number of speakers
                        <input
                          type="number"
                          min={1}
                          data-testid="speaker-count"
                          placeholder="Auto"
                          value={speakerCount}
                          onChange={(e) => setSpeakerCount(e.target.value)}
                          style={{ width: 72 }}
                        />
                      </label>
                    )}
                  </div>
                </section>
              )}

              {progress && (
                <section
                  data-testid="transcribe-progress"
                  aria-live="polite"
                  style={{
                    padding: "var(--space-6)",
                    background: "var(--surface)",
                    border: "1px solid var(--slate-200)",
                    borderRadius: "var(--radius-lg)",
                    maxWidth: 720,
                  }}
                >
                  <h2 style={{ margin: 0, fontSize: "var(--text-lg)" }}>
                    Creating local transcript
                  </h2>
                  <p style={{ margin: "var(--space-2) 0 var(--space-4)", color: "var(--slate-500)" }}>
                    Keep Folio open while this recording is processed on your device.
                  </p>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      fontSize: "var(--text-sm)",
                      color: "var(--slate-600)",
                    }}
                  >
                    <span>{progress.label}</span>
                    <span className="tabular">
                      {Math.round(progress.fraction * 100)}%
                      {progress.etaSeconds !== undefined &&
                        ` · about ${Math.ceil(progress.etaSeconds)}s left`}
                    </span>
                  </div>
                  <progress
                    value={progress.fraction}
                    max={1}
                    style={{ width: "100%", height: 12, marginTop: "var(--space-2)" }}
                  />
                </section>
              )}

              {state && (
                <>
                  <div
                    style={{
                      display: "flex",
                      gap: "var(--space-3)",
                      alignItems: "center",
                      flexWrap: "wrap",
                    }}
                  >
                    <Button onClick={pickAndTranscribe}>Import another recording</Button>
                    <Chip tone={statusTone}>{status}</Chip>
                    <Button
                      variant="danger"
                      data-testid="delete-matter"
                      onClick={() => void deleteMatter()}
                      style={{ marginLeft: "auto" }}
                    >
                      Delete audio and transcript
                    </Button>
                  </div>
                  <div style={{ marginTop: "var(--space-5)" }}>
                    <Editor
                      state={state}
                      setState={setState}
                      audioSrc={audioSrc}
                      diarized={diarized}
                      onUndo={onUndo}
                      onRedo={onRedo}
                      canUndo={history ? canUndo(history) : false}
                      canRedo={history ? canRedo(history) : false}
                    />
                  </div>
                </>
              )}
            </>
          ) : view === "models" ? (
            <Models onPacksChanged={updateModelStatus} />
          ) : (
            <Library onOpen={openSession} onImport={() => setView("transcribe")} />
          )}
        </div>
      </main>
    </div>
  );
}
