//! Tauri IPC commands. PR2 exposes a single `transcribe` command that wires the
//! frontend to the in-process `engine` crate (sherpa-onnx ASR + diarization).
//!
//! This is in-process and CPU-only: the audio never leaves the machine and no
//! network call is made. The command is what gives the network-crate gate a
//! real dependency graph (app -> engine -> sherpa-onnx) to scan.

use engine::{decode_to_16k_mono_with_progress, process, EngineConfig, SherpaEngine};
use tauri::{AppHandle, Emitter};

use crate::models;

/// Map a frontend model name to an [`engine::AsrModel`], defaulting to the engine
/// default (`parakeet`) for `None`/unrecognized values. Accepts the same aliases
/// as the engine example's `--model` flag.
fn parse_asr_model(name: Option<&str>) -> engine::AsrModel {
    match name.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("parakeet") | Some("parakeet-v2") | Some("parakeet-tdt-0.6b-v2") => {
            engine::AsrModel::ParakeetV2
        }
        Some("medium.en") | Some("medium") => engine::AsrModel::MediumEn,
        Some("small.en") | Some("small") => engine::AsrModel::SmallEn,
        _ => engine::AsrModel::default(),
    }
}

/// DEBUG-ONLY raw models dir override: when `KINDTECH_MODELS_DIR` is set in a
/// debug build, the transcribe path uses it RAW (dev/demo convenience), skipping
/// verified-pack resolution. NEVER consulted in a release/hardened build.
#[cfg(debug_assertions)]
fn debug_models_dir_override() -> Option<std::path::PathBuf> {
    std::env::var("KINDTECH_MODELS_DIR").ok().map(std::path::PathBuf::from)
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
/// `num_speakers = None` then lets the engine auto-detect.
///
/// `model` selects the ASR model: `"parakeet"` (Parakeet-TDT-v2, English, the
/// DEFAULT — fastest on CPU with native word timestamps) or `"small.en"` /
/// `"medium.en"` (Whisper, for non-English or hard/noisy audio).
/// `None`/unrecognized falls back to the default (`parakeet`).
/// NOTE: because the transcribe path loads ONLY verified signed packs, a signed
/// Parakeet pack must be shipped or the resolver fails closed on the default.
#[tauri::command]
pub async fn transcribe(
    app: AppHandle,
    path: String,
    num_speakers: Option<u32>,
    diarize: Option<bool>,
    model: Option<String>,
) -> Result<engine::Transcript, String> {
    // `models_dir` is left UNSET here on purpose: it is resolved INSIDE
    // spawn_blocking (verified-pack extraction may be large — keep the UI thread
    // free) and assigned just before `SherpaEngine::new`.
    let mut cfg = EngineConfig::new("");
    cfg.num_speakers = num_speakers;
    cfg.diarize = diarize.unwrap_or(false);
    cfg.asr_model = parse_asr_model(model.as_deref());

    // Clone for the resolver/emit closure (AppHandle is Clone + Send).
    let resolve_app = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        // Resolve the models dir, enforcing verified-pack trust.
        //
        // Debug build: an explicit KINDTECH_MODELS_DIR is honored RAW (dev/demo);
        // otherwise we resolve via the verified-pack resolver.
        // Release/hardened build: ALWAYS resolve via the verified-pack resolver,
        // IGNORING KINDTECH_MODELS_DIR. Fail closed if resolution errors.
        let models_dir = {
            #[cfg(debug_assertions)]
            {
                if let Some(raw) = debug_models_dir_override() {
                    raw
                } else {
                    models::resolve_verified_models_dir(&resolve_app, cfg.asr_model, cfg.diarize)?
                }
            }
            #[cfg(not(debug_assertions))]
            {
                models::resolve_verified_models_dir(&resolve_app, cfg.asr_model, cfg.diarize)?
            }
        };
        cfg.models_dir = models_dir;

        let sherpa = SherpaEngine::new(&cfg).map_err(|e| e.to_string())?;

        // One progress sink for the whole job (best-effort: a dropped event must
        // never fail transcription). Decode drives [0, PROGRESS_DECODE_END];
        // model-load / VAD / transcription fractions come from the engine.
        let progress = move |p: engine::Progress| {
            let _ = app.emit("transcribe-progress", p);
        };
        progress(engine::Progress::new("decoding", 0, 0, 0.0)); // show the bar at once
        let (samples, sample_rate) = decode_to_16k_mono_with_progress(&path, &|f| {
            progress(engine::Progress::new(
                "decoding",
                0,
                0,
                engine::PROGRESS_DECODE_END * f.clamp(0.0, 1.0),
            ));
        })
        .map_err(|e| e.to_string())?;
        process(&sherpa, &samples, sample_rate, &cfg, &progress).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("transcription task failed: {e}"))?
}
