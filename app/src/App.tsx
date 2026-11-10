import { useEffect, useRef, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
import { basename, buildSession } from "./session";
import type { SessionJson } from "./session";
import Editor from "./Editor";
import Models, { selectAsrModel } from "./Models";
import type { PackStatus } from "./Models";
import Library from "./Library";
import Sidebar from "./Sidebar";
import type { View } from "./Sidebar";
import { Banner, Button } from "./ui";

const HEADINGS: Record<View, { title: string; subtitle: string }> = {
  library: {
    title: "Transcripts",
    subtitle: "Draft transcripts generated and stored on this device.",
  },
  transcribe: {
    title: "Import recording",
    subtitle: "Create a private transcript without uploading the recording.",
  },
  models: {
    title: "Models & settings",
    subtitle: "Manage the signed model packs available to Folio.",
  },
};

const MODEL_LABELS: Record<string, string> = {
  parakeet: "Parakeet",
  "small.en": "Whisper small (English)",
  "medium.en": "Whisper medium (English)",
};

type SaveStatus = "saved" | "saving" | "error";

interface OpenSessionMeta {
  id: string;
  createdAt: string;
  audioPath: string;
  audioFilename: string;
  diarized: boolean;
}

const SAVE_LABELS: Record<SaveStatus, string> = {
  saved: "Saved locally",
  saving: "Saving locally…",
  error: "Local save failed",
};

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
  // `state` is the history's present; `setState` pushes a new entry.
  const [history, setHistory] = useState<History<EditorState> | null>(null);
  const state = history?.present ?? null;
  const setState = (next: EditorState) =>
    setHistory((h) => (h ? pushHistory(h, next) : initHistory(next)));
  const onUndo = () => setHistory((h) => (h ? undo(h) : h));
  const onRedo = () => setHistory((h) => (h ? redo(h) : h));
  const [audioSrc, setAudioSrc] = useState<string | undefined>(undefined);
  // Picked (backend-approved) recording waiting for "Create transcript".
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [encryption, setEncryption] = useState<EncryptionStatus | null>(null);
  const [encryptionDismissed, setEncryptionDismissed] = useState(false);
  // Speaker detection is OPT-IN (default off): on real audio its quality hurt
  // more than it helped, so the default is plain paragraph QA mode.
  const [detectSpeakers, setDetectSpeakers] = useState(false);
  const [speakerCount, setSpeakerCount] = useState("");
  // Whether the CURRENTLY loaded transcript was diarized (drives the editor view).
  const [diarized, setDiarized] = useState(false);
  const [progress, setProgress] = useState<ProgressView | null>(null);
  const [installedAsrModel, setInstalledAsrModel] = useState<string | null>(null);
  const [debugAsrModel, setDebugAsrModel] = useState<string | null>(null);
  const [modelsChecked, setModelsChecked] = useState(false);
  const asrModel = debugAsrModel ?? installedAsrModel;
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("saved");
  // Metadata for the currently loaded library session (null = unsaved/none).
  // Drives debounced auto-save and ties a loaded transcript to its saved entry.
  const [currentSessionMeta, setCurrentSessionMeta] = useState<OpenSessionMeta | null>(null);
  // The last snapshot written to disk, and the one on screen now (async saves
  // read it to decide whether they are still the latest).
  const savedStateRef = useRef<EditorState | null>(null);
  const stateRef = useRef<EditorState | null>(null);
  stateRef.current = state;

  function updateModelStatus(packs: PackStatus[]) {
    setInstalledAsrModel(selectAsrModel(packs));
    setModelsChecked(true);
  }

  // Soft advisory on mount: non-Windows builds report `unknown`.
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

  // Write one snapshot to the library. Never throws; false means the save failed.
  async function persist(meta: OpenSessionMeta, snapshot: EditorState): Promise<boolean> {
    if (snapshot === savedStateRef.current) return true;
    setSaveStatus("saving");
    try {
      await invoke("save_session", {
        session: buildSession({
          id: meta.id,
          createdAt: meta.createdAt,
          updatedAt: new Date().toISOString(),
          audioPath: meta.audioPath,
          diarized: meta.diarized,
          state: snapshot,
        }),
      });
      savedStateRef.current = snapshot;
      if (stateRef.current === snapshot) setSaveStatus("saved");
      return true;
    } catch {
      setSaveStatus("error");
      return false;
    }
  }

  // Write any edit still waiting for the autosave timer.
  function flushPendingSave(): Promise<boolean> {
    if (!state || !currentSessionMeta) return Promise.resolve(true);
    return persist(currentSessionMeta, state);
  }

  // Debounced (800ms) auto-save. The header says "Saving" from the moment an
  // edit lands until it is on disk.
  useEffect(() => {
    if (!state || !currentSessionMeta) return;
    if (state === savedStateRef.current) {
      setSaveStatus("saved");
      return;
    }
    setSaveStatus("saving");
    const meta = currentSessionMeta;
    const timer = setTimeout(() => void persist(meta, state), 800);
    return () => clearTimeout(timer);
  }, [state, currentSessionMeta]);

  // Open a saved session from the Library: load its JSON, restore the editor
  // and audio, and switch to the review view. Audio may be missing if the
  // referenced source file was moved/deleted.
  async function openSession(id: string) {
    // Keep the open transcript on screen if its latest edit could not be saved.
    if (!(await flushPendingSave())) {
      setView("transcribe");
      return;
    }
    try {
      const res = await invoke<{ session: SessionJson; audioAvailable: boolean }>(
        "load_session",
        { id },
      );
      savedStateRef.current = res.session.editorState;
      setHistory(initHistory(res.session.editorState));
      setDiarized(res.session.diarized);
      setCurrentSessionMeta({
        id: res.session.id,
        createdAt: res.session.createdAt,
        audioPath: res.session.audioPath,
        audioFilename: res.session.audioFilename,
        diarized: res.session.diarized,
      });
      setStatus("idle");
      setAudioSrc(
        res.audioAvailable ? convertFileSrc(res.session.audioPath) : undefined,
      );
      setView("transcribe");
    } catch (e) {
      setStatus(`error opening transcript: ${String(e)}`);
    }
  }

  // Ask the backend to confirm and delete both the source recording and saved
  // transcript, then clear the loaded UI only after the backend succeeds.
  async function deleteTranscript() {
    try {
      const report = await invoke<{ deleted: string[]; failed: unknown[] }>(
        "delete_matter",
        { id: currentSessionMeta?.id ?? "" },
      );
      if (report.failed.length > 0) {
        setStatus("error deleting transcript: source audio could not be deleted");
        return;
      }
      setHistory(null);
      setAudioSrc(undefined);
      setCurrentSessionMeta(null);
      setStatus("idle");
      setView("library");
    } catch (e) {
      setStatus(`error deleting transcript: ${String(e)}`);
    }
  }

  // Clear any open transcript and show an empty import card. A running
  // transcription keeps the screen until it finishes, and an edit that cannot be
  // saved keeps its transcript open (the header shows the failure).
  async function startImport() {
    if (progress || !(await flushPendingSave())) {
      setView("transcribe");
      return;
    }
    setHistory(null);
    setAudioSrc(undefined);
    setCurrentSessionMeta(null);
    setSelectedPath(null);
    setProgress(null);
    setStatus("idle");
    setView("transcribe");
  }

  function navigate(next: View) {
    if (next === "transcribe") void startImport();
    else setView(next);
  }

  async function chooseRecording() {
    if (!asrModel) {
      setStatus("error: install a verified transcription model before choosing a recording");
      return;
    }
    try {
      const path = await invoke<string | null>("pick_recording");
      if (typeof path === "string") setSelectedPath(path);
      setStatus("idle");
    } catch (e) {
      setStatus(`error: ${String(e)}`);
    }
  }

  async function createTranscript() {
    if (!selectedPath || !asrModel) return;
    const path = selectedPath;
    setHistory(null);
    setCurrentSessionMeta(null);
    const parsedCount = parseInt(speakerCount, 10);
    const num_speakers =
      detectSpeakers && Number.isFinite(parsedCount) && parsedCount > 0
        ? parsedCount
        : undefined;

    // Unlisten in `finally` so the progress listener never leaks.
    let unlisten = () => {};
    const startedAt = Date.now();
    setStatus("transcribing…");
    setProgress({ fraction: 0, label: "Starting…" });
    try {
      unlisten = await listen<Progress>("transcribe-progress", (event) => {
        const elapsedSec = (Date.now() - startedAt) / 1000;
        setProgress(progressView(event.payload, elapsedSec));
      });

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
      setSelectedPath(null);
      const meta = {
        id,
        createdAt,
        audioPath: path,
        audioFilename: basename(path),
        diarized: detectSpeakers,
      };
      setCurrentSessionMeta(meta);
      setStatus("idle");
      await persist(meta, editorState);
    } catch (e) {
      setStatus(`error: ${String(e)}`);
    } finally {
      unlisten();
      setProgress(null);
    }
  }

  const showEncryptionWarning =
    encryption && encryption.state !== "encrypted" && !encryptionDismissed;
  const reviewing = view === "transcribe" && state !== null;
  const heading = HEADINGS[view];
  const transcriptTitle =
    currentSessionMeta?.audioFilename.replace(/\.[^.]+$/, "") || "Draft transcript";
  const errorMessage = status.startsWith("error")
    ? status.replace(/^error[^:]*:\s*/, "")
    : null;

  return (
    <div className="app-shell">
      {/* An open transcript belongs to Transcripts, not to Import recording. */}
      <Sidebar view={reviewing ? "library" : view} onNavigate={navigate} />

      <main className={reviewing ? "app-main transcript-workspace" : "app-main"}>
        {reviewing ? (
          <header className="draft-header">
            <div style={{ minWidth: 0 }}>
              <h1 title={transcriptTitle}>{transcriptTitle}</h1>
              <div className="draft-meta">
                <span>Draft transcript</span>
                <span aria-hidden="true">·</span>
                <span
                  data-testid="save-status"
                  className={saveStatus === "error" ? "save-error" : undefined}
                >
                  {SAVE_LABELS[saveStatus]}
                </span>
              </div>
            </div>
            <div className="draft-header-actions">
              <Button onClick={() => void startImport()}>Import another</Button>
              <Button
                variant="danger"
                data-testid="delete-transcript"
                onClick={() => void deleteTranscript()}
              >
                Delete transcript and audio
              </Button>
            </div>
          </header>
        ) : (
          <header className="page-header">
            <h1>{heading.title}</h1>
            <p>{heading.subtitle}</p>
          </header>
        )}

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
            client-confidential recordings, store and run on an encrypted volume.{" "}
            <span style={{ opacity: 0.8 }}>({encryption.detail})</span>
          </Banner>
        )}

        <div className="page-content">
          {view === "transcribe" ? (
            <>
              {!state && !progress && (
                <section className="import-card">
                  <h2>Choose a recording to transcribe</h2>
                  <p
                    style={{
                      color: "var(--slate-500)",
                      margin: "var(--space-2) 0 var(--space-5)",
                      maxWidth: 560,
                    }}
                  >
                    Supported audio and video files are processed entirely on this
                    device and saved to Transcripts automatically.
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
                  ) : selectedPath ? (
                    <div className="selected-recording">
                      <div style={{ minWidth: 0 }}>
                        <div className="field-label">Selected recording</div>
                        <strong title={selectedPath}>{basename(selectedPath)}</strong>
                      </div>
                      <Button size="sm" onClick={() => void chooseRecording()}>
                        Change
                      </Button>
                    </div>
                  ) : (
                    <Button
                      variant="primary"
                      onClick={() => void chooseRecording()}
                      disabled={!modelsChecked}
                      style={{ marginBottom: "var(--space-4)" }}
                    >
                      {modelsChecked ? "Choose recording" : "Checking transcription model…"}
                    </Button>
                  )}
                  {errorMessage && asrModel && (
                    <Banner
                      variant="warning"
                      role="alert"
                      style={{ marginBottom: "var(--space-4)" }}
                    >
                      {errorMessage}
                    </Banner>
                  )}

                  {asrModel && (
                    <div className="model-ready">
                      <div>
                        <div className="field-label">Transcription model</div>
                        <strong>{MODEL_LABELS[asrModel] ?? asrModel}</strong>
                        <span> · Ready on this device</span>
                      </div>
                    </div>
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
                      Transcript options
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
                  {selectedPath && (
                    <Button
                      variant="primary"
                      onClick={() => void createTranscript()}
                      style={{ marginTop: "var(--space-5)", minWidth: 180 }}
                    >
                      Create transcript
                    </Button>
                  )}
                </section>
              )}

              {progress && (
                <section
                  className="progress-card"
                  data-testid="transcribe-progress"
                  aria-live="polite"
                >
                  <h2>Creating local transcript</h2>
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
                    aria-label="Transcription progress"
                    style={{ width: "100%", height: 12, marginTop: "var(--space-2)" }}
                  />
                </section>
              )}

              {state && (
                <>
                  {errorMessage && (
                    <Banner variant="warning" role="alert" style={{ marginTop: "var(--space-3)" }}>
                      {errorMessage}
                    </Banner>
                  )}
                  {!audioSrc && (
                    <Banner variant="info" role="note" style={{ marginTop: "var(--space-3)" }}>
                      The source recording was not found, so playback is unavailable.
                    </Banner>
                  )}
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
                </>
              )}
            </>
          ) : view === "models" ? (
            <Models onPacksChanged={updateModelStatus} />
          ) : (
            <Library onOpen={(id) => void openSession(id)} onImport={() => void startImport()} />
          )}
        </div>
      </main>
    </div>
  );
}
