import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { Transcript } from "@core/types";
import { mmss } from "./format";
import { progressView } from "./progress";
import type { Progress, ProgressView } from "./progress";
import SpeakerOptions, { requestedSpeakers } from "./SpeakerOptions";
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
}

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
  onFinished: (transcript: Transcript, audioPath: string, diarized: boolean) => void;
}) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [utterances, setUtterances] = useState<Utterance[]>([]);
  const [level, setLevel] = useState<Level>({ level: 0, speaking: false, elapsedS: 0 });
  const [error, setError] = useState<string | null>(null);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [progress, setProgress] = useState<ProgressView | null>(null);
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
  }

  function reset() {
    stopListening();
    audioPathRef.current = null;
    setUtterances([]);
    setLevel({ level: 0, speaking: false, elapsedS: 0 });
    setConfirmDiscard(false);
    setProgress(null);
    setPhase("idle");
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
      ]);
      const started = await invoke<{ audioPath: string }>("start_recording", {
        model: asrModel,
        diarize: detectSpeakers,
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
    const startedAt = Date.now();
    setProgress({ fraction: 0.95, label: "Finishing transcript…" });
    try {
      unlistenRef.current.push(
        await listen<Progress>("transcribe-progress", (e) =>
          setProgress(progressView(e.payload, (Date.now() - startedAt) / 1000)),
        ),
      );
      const transcript = await invoke<Transcript>("stop_recording", {
        numSpeakers: requestedSpeakers(detectSpeakers, speakerCount),
      });
      reset();
      onFinished(transcript, audioPath, detectSpeakers);
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
              style={{ marginBottom: "var(--space-4)" }}
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
        {asrModel && (
          <div className="model-ready">
            <div>
              <div className="field-label">Transcription model</div>
              <strong>{modelLabel}</strong>
              <span> · Ready on this device</span>
            </div>
          </div>
        )}
        <SpeakerOptions
          detectSpeakers={detectSpeakers}
          onDetectSpeakers={onDetectSpeakers}
          speakerCount={speakerCount}
          onSpeakerCount={onSpeakerCount}
        />
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

      {leaveBlocked && (
        <Banner variant="info" role="note" style={{ marginBottom: "var(--space-3)" }}>
          Stop or discard the recording before leaving this page.
        </Banner>
      )}
      {error && (
        <Banner variant="warning" role="alert" style={{ marginBottom: "var(--space-3)" }}>
          {error}
        </Banner>
      )}

      <div className="live-transcript" ref={listRef} aria-live="polite" data-testid="live-transcript">
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
    </section>
  );
}
