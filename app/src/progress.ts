// View-model for the progress bar: maps the latest Rust `transcribe-progress`
// event + elapsed time to a clamped fraction, label and ETA.

export type ProgressStage =
  | "decoding"
  | "loading"
  | "segmenting"
  | "transcribing"
  | "diarizing"
  | "done";

export interface Progress {
  stage: string;
  done: number;
  total: number;
  fraction: number;
}

export interface ProgressView {
  /** Completion in [0, 1]. */
  fraction: number;
  /** Human-readable stage label. */
  label: string;
  /** Estimated seconds remaining, omitted when not yet computable. */
  etaSeconds?: number;
}

const STAGE_LABELS: Record<string, string> = {
  decoding: "Decoding audio…",
  loading: "Loading model…",
  segmenting: "Detecting speech…",
  transcribing: "Transcribing…",
  diarizing: "Detecting speakers…",
  done: "Done",
};

function clamp01(x: number): number {
  if (!Number.isFinite(x)) return 0;
  if (x < 0) return 0;
  if (x > 1) return 1;
  return x;
}

/**
 * Build the bar's view model from the latest `Progress` event and the elapsed
 * wall-clock seconds since the job started.
 *
 * ETA = elapsed / fraction · (1 − fraction), only once `fraction > 0` and the
 * job is not yet done (no point estimating a finished or unstarted job).
 */
export function progressView(p: Progress, elapsedSec: number): ProgressView {
  const fraction = clamp01(p.fraction);
  const label = STAGE_LABELS[p.stage] ?? p.stage;

  const view: ProgressView = { fraction, label };

  if (
    p.stage !== "done" &&
    fraction > 0 &&
    fraction < 1 &&
    Number.isFinite(elapsedSec) &&
    elapsedSec > 0
  ) {
    view.etaSeconds = (elapsedSec / fraction) * (1 - fraction);
  }

  return view;
}
