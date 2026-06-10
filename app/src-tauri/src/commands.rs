//! Tauri IPC commands. PR2 exposes a single `transcribe` command that wires the
//! frontend to the in-process `engine` crate (sherpa-onnx ASR + diarization).
//!
//! This is in-process and CPU-only: the audio never leaves the machine and no
//! network call is made. The command is what gives the network-crate gate a
//! real dependency graph (app -> engine -> sherpa-onnx) to scan.

use std::path::PathBuf;

use engine::{load_wav_16k_mono, process, EngineConfig, SherpaEngine};

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

/// Transcribe + diarize a 16 kHz mono WAV into the project's JSON contract.
///
/// `num_speakers = None` lets the engine auto-detect. Uses the default ASR
/// model (`small.en`). Runtime success requires the model files to be present;
/// that is best-effort for PR2 (the bar is that this COMPILES/LINKS into the
/// Tauri build), and is finalized by the model-pack work in PR3.
#[tauri::command]
pub fn transcribe(path: String, num_speakers: Option<u32>) -> Result<engine::Transcript, String> {
    let mut cfg = EngineConfig::new(models_dir());
    cfg.num_speakers = num_speakers;

    let sherpa = SherpaEngine::new(&cfg).map_err(|e| e.to_string())?;
    let (samples, sample_rate) = load_wav_16k_mono(&path).map_err(|e| e.to_string())?;
    process(&sherpa, &samples, sample_rate, &cfg).map_err(|e| e.to_string())
}
