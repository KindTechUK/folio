import { describe, it, expect } from "vitest";
import { progressView } from "./progress";
import type { Progress } from "./progress";

// Pure view-model tests. The live Tauri `listen("transcribe-progress", …)`
// wiring in App.tsx is NOT exercised here — Tauri events are not available in
// jsdom; that path is verified manually with `tauri dev` (see App.tsx).

function p(stage: string, fraction: number): Progress {
  return { stage, done: 0, total: 1, fraction };
}

describe("progressView", () => {
  it("maps each stage to a human label", () => {
    expect(progressView(p("decoding", 0), 0).label).toBe("Decoding audio…");
    expect(progressView(p("transcribing", 0.5), 1).label).toBe("Transcribing…");
    expect(progressView(p("diarizing", 0.95), 1).label).toBe("Detecting speakers…");
    expect(progressView(p("done", 1), 1).label).toBe("Done");
  });

  it("falls back to the raw stage string for unknown stages", () => {
    expect(progressView(p("weird", 0.2), 1).label).toBe("weird");
  });

  it("clamps fraction into [0, 1]", () => {
    expect(progressView(p("transcribing", 2), 1).fraction).toBe(1);
    expect(progressView(p("transcribing", -0.5), 1).fraction).toBe(0);
    expect(progressView(p("transcribing", Number.NaN), 1).fraction).toBe(0);
  });

  it("computes ETA = elapsed/fraction·(1−fraction)", () => {
    // 25% done after 10s -> 30s remaining.
    const v = progressView(p("transcribing", 0.25), 10);
    expect(v.etaSeconds).toBeCloseTo(30, 6);
  });

  it("omits ETA when fraction is 0 (not yet computable)", () => {
    expect(progressView(p("decoding", 0), 5).etaSeconds).toBeUndefined();
  });

  it("omits ETA at done / fraction 1", () => {
    expect(progressView(p("done", 1), 60).etaSeconds).toBeUndefined();
    expect(progressView(p("transcribing", 1), 60).etaSeconds).toBeUndefined();
  });

  it("omits ETA when no time has elapsed", () => {
    expect(progressView(p("transcribing", 0.5), 0).etaSeconds).toBeUndefined();
  });
});
