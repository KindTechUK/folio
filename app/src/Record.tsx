import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { Transcript } from "@core/types";
import { mmss } from "./format";
import { progressView } from "./progress";
import type { Progress, ProgressView } from "./progress";
import SpeakerBar from "./SpeakerBar";
import { requestedSpeakers } from "./SpeakerOptions";
import { Banner, Button } from "./ui";

/** One utterance as the backend emits it (`record-utterance`). */
interface Utterance {
  start: number;
  end: number;
  text: string;
}

interface Level {
  level: number;
  speaking: boolean;
  elapsedS: number;
  /** Seconds of speech waiting to be transcribed. */
  backlogS: number;
}

interface Warnings {
  silent: boolean;
  narrowband: boolean;
}

interface Status {
  message: string | null;
  transient: boolean;
}

/** What `stop_recording` returns. */
interface Stopped {
  transcript: Transcript;
  speakersLabelled: boolean;
  /** Something the user should know about the result (e.g. labelling failed). */
  note: string | null;
}

/** What a finished recording hands to the transcript view. */
export interface FinishedRecording {
  transcript: Transcript;
  audioPath: string;
  speakersLabelled: boolean;
  notes: string;
  note: string | null;
}

const SILENT_MESSAGE =
  "Folio is receiving no sound from the microphone. Check that it isn't muted, and on Windows " +
  "that Settings > Privacy & security > Microphone allows desktop apps.";
const NARROWBAND_MESSAGE =
  "This looks like a headset microphone. Headsets are tuned to pick up only the wearer, so other " +
  "people in the room may be missed. For a conversation in a room, use the computer's built-in " +
  "microphone or a USB desk microphone.";
/** Save notes this long after the last keystroke (ms). */
const NOTES_SAVE_DELAY = 800;
/** Show the catching-up note once live text is this far behind (seconds). */
const BACKLOG_NOTE_S = 15;
const IDLE_LEVEL: Level = { level: 0, speaking: false, elapsedS: 0, backlogS: 0 };
const NO_WARNINGS: Warnings = { silent: false, narrowband: false };

type Phase = "idle" | "starting" | "recording" | "paused" | "stopping" | "discarding";

export default function Record({
  asrModel,
  modelsChecked,
  modelLabel,
  onOpenModels,
  detectSpeakers,
  onDetectSpeakers,
  speakerCount,
  onSpeakerCount,
  leaveBlocked,
  importRunning,
  onActiveChange,
  onFinished,
}: {
  asrModel: string | null;
  modelsChecked: boolean;
  modelLabel: string;
  onOpenModels: () => void;
  detectSpeakers: boolean;
  onDetectSpeakers: (on: boolean) => void;
  speakerCount: string;
  onSpeakerCount: (count: string) => void;
  /** Someone tried to leave the page while recording. */
  leaveBlocked: boolean;
  /** An imported file is still being transcribed. */
  importRunning: boolean;
  onActiveChange: (active: boolean) => void;
  onFinished: (recording: FinishedRecording) => void;
}) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [utterances, setUtterances] = useState<Utterance[]>([]);
  const [level, setLevel] = useState<Level>(IDLE_LEVEL);
  const [warnings, setWarnings] = useState<Warnings>(NO_WARNINGS);
  const [deviceStatus, setDeviceStatus] = useState<string | null>(null);
  const [deviceName, setDeviceName] = useState<string | null>(null);
  const statusTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [progress, setProgress] = useState<ProgressView | null>(null);
  const [tab, setTab] = useState<"transcript" | "notes">("transcript");
  const [notes, setNotes] = useState("");
  // The latest notes, read when Stop finishes (typing can continue meanwhile).
  const notesRef = useRef("");
  const [notesError, setNotesError] = useState<string | null>(null);
  const notesTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const audioPathRef = useRef<string | null>(null);
  const unlistenRef = useRef<UnlistenFn[]>([]);
  const listRef = useRef<HTMLDivElement | null>(null);

  const active = phase !== "idle";

  useEffect(() => {
    onActiveChange(active);
  }, [active]); // onActiveChange is a fresh closure each render; only changes matter

  useEffect(() => () => stopListening(), []);

  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [utterances.length]);

  function stopListening() {
    for (const off of unlistenRef.current) off();
    unlistenRef.current = [];
    if (statusTimer.current) clearTimeout(statusTimer.current);
    if (notesTimer.current) clearTimeout(notesTimer.current);
  }

  function showStatus(status: Status) {
    if (statusTimer.current) clearTimeout(statusTimer.current);
    setDeviceStatus(status.message);
    if (status.transient) statusTimer.current = setTimeout(() => setDeviceStatus(null), 6000);
  }

  function reset() {
    stopListening();
    audioPathRef.current = null;
    setUtterances([]);
    setLevel(IDLE_LEVEL);
    setWarnings(NO_WARNINGS);
    setDeviceStatus(null);
    setDeviceName(null);
    setConfirmDiscard(false);
    setProgress(null);
    setTab("transcript");
    notesRef.current = "";
    setNotes("");
    setNotesError(null);
    setPhase("idle");
  }

  // Keep a copy of the notes next to the recording while it runs, so a crash
  // doesn't lose them. The notes in this page are what gets saved on Stop.
  function editNotes(text: string) {
    notesRef.current = text;
    setNotes(text);
    if (notesTimer.current) clearTimeout(notesTimer.current);
    notesTimer.current = setTimeout(() => {
      invoke("save_recording_notes", { text }).then(
        () => setNotesError(null),
        (e) => setNotesError(String(e)),
      );
    }, NOTES_SAVE_DELAY);
  }

  async function start() {
    if (!asrModel) return;
    setError(null);
    setUtterances([]);
    setPhase("starting");
    try {
      unlistenRef.current = await Promise.all([
        listen<Utterance>("record-utterance", (e) =>
          setUtterances((list) => [...list, e.payload]),
        ),
        listen<Level>("record-level", (e) => setLevel(e.payload)),
        listen<string>("record-error", (e) => setError(e.payload)),
        listen<Warnings>("record-warnings", (e) => setWarnings(e.payload)),
        listen<Status>("record-status", (e) => showStatus(e.payload)),
        listen<string>("record-device", (e) => setDeviceName(e.payload)),
      ]);
      const started = await invoke<{ audioPath: string }>("start_recording", {
        model: asrModel,
      });
      audioPathRef.current = started.audioPath;
      setPhase("recording");
    } catch (e) {
      reset();
      setError(String(e));
    }
  }

  async function togglePause() {
    const pausing = phase === "recording";
    try {
      await invoke(pausing ? "pause_recording" : "resume_recording");
      setPhase(pausing ? "paused" : "recording");
    } catch (e) {
      setError(String(e));
    }
  }

  async function stop() {
    const audioPath = audioPathRef.current;
    if (!audioPath) return;
    setPhase("stopping");
    setConfirmDiscard(false);
    if (notesTimer.current) clearTimeout(notesTimer.current);
    const startedAt = Date.now();
    setProgress({ fraction: 0.95, label: "Finishing transcript…" });
    try {
      unlistenRef.current.push(
        await listen<Progress>("transcribe-progress", (e) =>
          setProgress(progressView(e.payload, (Date.now() - startedAt) / 1000)),
        ),
      );
      const stopped = await invoke<Stopped>("stop_recording", {
        diarize: detectSpeakers,
        numSpeakers: requestedSpeakers(detectSpeakers, speakerCount),
      });
      const finished: FinishedRecording = {
        transcript: stopped.transcript,
        audioPath,
        speakersLabelled: stopped.speakersLabelled,
        notes: notesRef.current,
        note: stopped.note,
      };
      reset();
      onFinished(finished);
    } catch (e) {
      reset();
      setError(String(e));
    }
  }

  async function discard() {
    if (phase !== "recording" && phase !== "paused") return;
    setPhase("discarding");
    try {
      await invoke("discard_recording");
      reset();
    } catch (e) {
      reset();
      setError(String(e));
    }
  }

  if (!active) {
    return (
      <section className="import-card">
        <h2>Record a conversation</h2>
        <p className="card-intro">
          Audio is recorded and transcribed on this device. Text appears a few seconds after each
          person finishes speaking, and the recording is saved to Transcripts when you stop.
        </p>
        {modelsChecked && !asrModel ? (
          <Banner variant="warning" role="alert" style={{ marginBottom: "var(--space-4)" }}>
            A verified transcription model must be installed before recording.{" "}
            <Button size="sm" onClick={onOpenModels} style={{ marginLeft: "var(--space-2)" }}>
              Open Models &amp; settings
            </Button>
          </Banner>
        ) : (
          <>
            <Banner variant="info" role="note" style={{ marginBottom: "var(--space-4)" }}>
              Tell everyone present that you are recording.
            </Banner>
            {asrModel && (
              <div className="model-ready">
                <div>
                  <div className="field-label">Transcription model</div>
                  <strong>{modelLabel}</strong>
                  <span> · Ready on this device</span>
                </div>
              </div>
            )}
            <SpeakerBar
              labelSpeakers={detectSpeakers}
              onLabelSpeakers={onDetectSpeakers}
              speakerCount={speakerCount}
              onSpeakerCount={onSpeakerCount}
            />
            {importRunning && (
              <Banner variant="info" role="note" style={{ marginBottom: "var(--space-4)" }}>
                An imported recording is still being transcribed. You can record when it
                finishes.
              </Banner>
            )}
            <Button
              variant="primary"
              onClick={() => void start()}
              disabled={!modelsChecked || importRunning}
              style={{ margin: "var(--space-5) 0 var(--space-4)" }}
            >
              {modelsChecked ? "Start recording" : "Checking transcription model…"}
            </Button>
          </>
        )}
        {error && (
          <Banner variant="warning" role="alert" style={{ marginBottom: "var(--space-4)" }}>
            {error}
          </Banner>
        )}
      </section>
    );
  }

  const busy = phase === "starting" || phase === "stopping" || phase === "discarding";
  const stateLabel =
    phase === "starting"
      ? "Starting…"
      : phase === "paused"
        ? "Paused"
        : phase === "stopping"
          ? "Finishing…"
          : phase === "discarding"
            ? "Discarding…"
            : "Recording";

  return (
    <section className="recording-card" aria-label="Recording">
      <div className="recording-status">
        <span
          className={phase === "recording" ? "recording-dot live" : "recording-dot"}
          aria-hidden="true"
        />
        <strong data-testid="recording-state">{stateLabel}</strong>
        {deviceName && (
          <span className="recording-device" data-testid="recording-device" title="Microphone">
            {deviceName}
          </span>
        )}
        <span className="tabular" data-testid="recording-elapsed">
          {mmss(level.elapsedS)}
        </span>
        <div
          className="level-meter"
          role="meter"
          aria-label="Input level"
          aria-valuemin={0}
          aria-valuemax={1}
          aria-valuenow={phase === "recording" ? level.level : 0}
        >
          <div style={{ width: `${Math.round((phase === "recording" ? level.level : 0) * 100)}%` }} />
        </div>
      </div>

      <div className="recording-tabs" role="tablist" aria-label="Recording view">
        <button
          type="button"
          role="tab"
          id="tab-transcript"
          aria-selected={tab === "transcript"}
          aria-controls="panel-transcript"
          onClick={() => setTab("transcript")}
        >
          Transcript
        </button>
        <button
          type="button"
          role="tab"
          id="tab-notes"
          aria-selected={tab === "notes"}
          aria-controls="panel-notes"
          onClick={() => setTab("notes")}
        >
          Your notes
        </button>
      </div>

      <SpeakerBar
        labelSpeakers={detectSpeakers}
        onLabelSpeakers={onDetectSpeakers}
        speakerCount={speakerCount}
        onSpeakerCount={onSpeakerCount}
        disabled={busy}
      />

      <div
        className="notes-panel"
        role="tabpanel"
        id="panel-notes"
        aria-labelledby="tab-notes"
        hidden={tab !== "notes"}
      >
        <textarea
          className="notes-input"
          aria-label="Your notes"
          aria-describedby="notes-privacy"
          placeholder="Write anything you want to remember. Only you see these notes."
          value={notes}
          onChange={(e) => editNotes(e.target.value)}
        />
        <p id="notes-privacy" className="notes-privacy">
          Private to you. Saved on this device with the transcript.
        </p>
        {notesError && (
          <p className="notes-privacy" role="alert">
            Notes couldn't be saved yet: {notesError}
          </p>
        )}
      </div>

      <div
        className="live-transcript"
        ref={listRef}
        aria-live="polite"
        data-testid="live-transcript"
        role="tabpanel"
        id="panel-transcript"
        aria-labelledby="tab-transcript"
        hidden={tab !== "transcript"}
      >
        {utterances.length === 0 && phase !== "stopping" && (
          <p className="live-empty">Text appears here a few seconds after someone speaks.</p>
        )}
        {utterances.map((u, i) => (
          <p key={`${u.start}-${i}`} className="live-utterance">
            <span className="tabular live-time">{mmss(u.start)}</span>
            <span>{u.text}</span>
          </p>
        ))}
        {phase === "recording" && level.speaking && (
          <p className="live-listening">Listening…</p>
        )}
      </div>

      {progress && phase === "stopping" && (
        <div className="recording-finish" aria-live="polite">
          <span>{progress.label}</span>
          <progress
            value={progress.fraction}
            max={1}
            aria-label="Finishing progress"
            style={{ width: "100%", height: 8 }}
          />
        </div>
      )}

      {confirmDiscard ? (
        <div className="recording-actions" role="group" aria-label="Confirm discard">
          <span>Delete this recording? Nothing will be saved.</span>
          {/* The safe choice comes first, where the pointer was over Discard. */}
          <Button onClick={() => setConfirmDiscard(false)} disabled={busy}>
            Keep recording
          </Button>
          <Button variant="danger" onClick={() => void discard()} disabled={busy}>
            Delete recording
          </Button>
        </div>
      ) : (
        <div className="recording-actions">
          <Button onClick={() => void togglePause()} disabled={busy}>
            {phase === "paused" ? "Resume" : "Pause"}
          </Button>
          <Button variant="primary" onClick={() => void stop()} disabled={busy}>
            Stop and create transcript
          </Button>
          <Button variant="ghost" onClick={() => setConfirmDiscard(true)} disabled={busy}>
            Discard
          </Button>
        </div>
      )}

      {/* Notices sit below the buttons so they never move under the pointer. */}
      {leaveBlocked && (
        <Banner variant="info" role="note" style={{ marginBottom: 0 }}>
          Stop or discard the recording before leaving this page.
        </Banner>
      )}
      {error && (
        <Banner variant="warning" role="alert" style={{ marginBottom: 0 }}>
          {error}
        </Banner>
      )}
      {deviceStatus && (
        <Banner variant="info" role="status" style={{ marginBottom: 0 }}>
          {deviceStatus}
        </Banner>
      )}
      {warnings.silent && phase === "recording" && (
        <Banner variant="warning" role="alert" style={{ marginBottom: 0 }}>
          {SILENT_MESSAGE}
        </Banner>
      )}
      {warnings.narrowband && (
        <Banner variant="info" role="note" style={{ marginBottom: 0 }}>
          {NARROWBAND_MESSAGE}
        </Banner>
      )}
      {level.backlogS > BACKLOG_NOTE_S && phase === "recording" && (
        <p className="live-backlog" role="status">
          Live text is catching up (about {Math.round(level.backlogS)} s behind). The recording
          itself is safe.
        </p>
      )}

    </section>
  );
}
