//! Tauri IPC commands. PR2 exposes a single `transcribe` command that wires the
//! frontend to the in-process `engine` crate (sherpa-onnx ASR + diarization).
//!
//! This is in-process and CPU-only: the audio never leaves the machine and no
//! network call is made. The command is what gives the network-crate gate a
//! real dependency graph (app -> engine -> sherpa-onnx) to scan.

use std::path::PathBuf;

use engine::{decode_to_16k_mono, process, EngineConfig, SherpaEngine};
use tauri::{AppHandle, Emitter};

/// Resolve the directory holding the model files.
///
/// PR2: read `PODCAST_MODELS_DIR` if set, else fall back to the repo's `models/`
/// dir (dev convenience). PR3 finalizes the bundled-models path (signed model
/// packs resolved under the per-user data dir + the installer-bundled default
/// pack) — see docs/plans/vibe-fork-plan.md §5.
fn models_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PODCAST_MODELS_DIR") {
        return PathBuf::from(dir);
    }
    // ../../models relative to app/src-tauri (the committed dev models dir).
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models")
}

/// Transcribe an audio file (wav/mp3/m4a/mp4/flac/ogg…) into the project's JSON
/// contract.
///
/// ASYNC + off the UI thread: the heavy decode + ASR + (optional) diarize runs
/// inside `spawn_blocking` so the webview stays responsive, and a determinate
/// progress stream is pushed to the frontend via the `transcribe-progress`
/// event. Input is decoded to 16 kHz mono in-process (Symphonia + Rubato, no
/// bundled ffmpeg) — any common container/codec is accepted.
///
/// `diarize` defaults to `false` (paragraph QA mode — speaker detection on real
/// audio was poor enough to hurt; see PR #4): the transcript comes back with no
/// speaker labels. Pass `diarize = Some(true)` to opt into speaker detection;
/// `num_speakers = None` then lets the engine auto-detect. Uses the default ASR
/// model (`small.en`).
#[tauri::command]
pub async fn transcribe(
    app: AppHandle,
    path: String,
    num_speakers: Option<u32>,
    diarize: Option<bool>,
) -> Result<engine::Transcript, String> {
    let mut cfg = EngineConfig::new(models_dir());
    cfg.num_speakers = num_speakers;
    cfg.diarize = diarize.unwrap_or(false);

    tauri::async_runtime::spawn_blocking(move || {
        let sherpa = SherpaEngine::new(&cfg).map_err(|e| e.to_string())?;
        let (samples, sample_rate) = decode_to_16k_mono(&path).map_err(|e| e.to_string())?;
        process(&sherpa, &samples, sample_rate, &cfg, &move |p| {
            // Best-effort: a dropped progress event must never fail the job.
            let _ = app.emit("transcribe-progress", p);
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("transcription task failed: {e}"))?
}
