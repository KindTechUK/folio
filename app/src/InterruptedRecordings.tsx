import { useState } from "react";
import { Banner, Button } from "./ui";

// Mirrors the Rust `InterruptedRecording` returned by `list_interrupted_recordings`.
export interface InterruptedRecording {
  audioPath: string;
  name: string;
  durationSec: number;
  hasNotes: boolean;
}

export const RECOVERED_NOTE =
  "Recovered from a recording that had no transcript. The last moment of audio may be missing.";

function durationLabel(sec: number): string {
  return sec < 60 ? `${Math.round(sec)} s` : `${Math.round(sec / 60)} min`;
}

interface Props {
  recordings: InterruptedRecording[];
  /** Recovery or deletion can't start now (e.g. a transcription is running). */
  busy: boolean;
  onRecover: (recording: InterruptedRecording) => void;
  onDelete: (recording: InterruptedRecording) => void;
}

// One notice per recording left without a transcript (the app closed or
// crashed while it was recording), offering to transcribe it or delete it.
export default function InterruptedRecordings({ recordings, busy, onRecover, onDelete }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  return (
    <>
      {recordings.map((r) => (
        <Banner
          key={r.audioPath}
          variant="warning"
          role="alert"
          data-testid="interrupted-recording"
          style={{ maxWidth: 1040, marginBottom: "var(--space-4)" }}
        >
          <span className="interrupted-recording">
            <span>
              <strong>{r.name}</strong> ({durationLabel(r.durationSec)}
              {r.hasNotes ? ", with notes" : ""}) has no transcript. Folio may have
              closed while it was recording.
            </span>
            {confirming === r.audioPath ? (
              <span className="interrupted-actions" role="group" aria-label="Confirm delete">
                <span>Delete this recording permanently?</span>
                <Button size="sm" onClick={() => setConfirming(null)}>
                  Keep
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  disabled={busy}
                  onClick={() => {
                    setConfirming(null);
                    onDelete(r);
                  }}
                >
                  Delete permanently
                </Button>
              </span>
            ) : (
              <span className="interrupted-actions">
                <Button size="sm" variant="primary" disabled={busy} onClick={() => onRecover(r)}>
                  Recover transcript
                </Button>
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => setConfirming(r.audioPath)}>
                  Delete
                </Button>
              </span>
            )}
          </span>
        </Banner>
      ))}
    </>
  );
}
