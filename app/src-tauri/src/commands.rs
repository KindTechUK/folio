//! Tauri IPC commands for transcription. In-process and CPU-only: audio never
//! leaves the machine and no network call is made.

use engine::{decode_to_16k_mono_with_progress, process, EngineConfig, SherpaEngine};
use tauri::{AppHandle, Emitter};

use crate::models;

/// Map a frontend model name to an [`engine::AsrModel`], falling back to the
/// engine default (`parakeet`) for `None`/unrecognized values.
pub(crate) fn parse_asr_model(name: Option<&str>) -> engine::AsrModel {
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

/// Report a usable raw-model override to the frontend in development builds.
///
/// This keeps the import preflight honest when developers run with
/// `KINDTECH_MODELS_DIR`: the UI only enables transcription when every file the
/// selected engine model needs is actually present. Release builds always return
/// `None` and rely exclusively on verified signed packs.
#[tauri::command]
pub fn debug_asr_model_override() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        let dir = debug_models_dir_override()?;
        for (model, name) in [
            (engine::AsrModel::ParakeetV2, "parakeet"),
            (engine::AsrModel::SmallEn, "small.en"),
            (engine::AsrModel::MediumEn, "medium.en"),
        ] {
            if engine::required_model_files(model, false)
                .iter()
                .all(|file| dir.join(file).is_file())
            {
                return Some(name.to_string());
            }
        }
        None
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// Resolve the models dir, enforcing verified-pack trust (debug-only raw
/// override: see `debug_models_dir_override`). Blocking: may extract packs.
pub(crate) fn resolve_models_dir(
    app: &AppHandle,
    asr_model: engine::AsrModel,
    diarize: bool,
) -> Result<std::path::PathBuf, String> {
    #[cfg(debug_assertions)]
    if let Some(raw) = debug_models_dir_override() {
        return Ok(raw);
    }
    models::resolve_verified_models_dir(app, asr_model, diarize)
}

/// Transcribe an audio file (wav/mp3/m4a/mp4/flac/ogg…) into the project's JSON
/// contract.
///
/// Runs decode + ASR + optional diarize in `spawn_blocking` and streams progress
/// via the `transcribe-progress` event.
///
/// `diarize` defaults to `false` (speaker-less transcript); `Some(true)` opts
/// in, and `num_speakers = None` then auto-detects.
///
/// `model`: `"parakeet"` (default) | `"small.en"` | `"medium.en"`;
/// `None`/unrecognized falls back to the default.
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

    let resolve_app = app.clone();
    let run_app = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        // Resolve the models dir, enforcing verified-pack trust (debug-only raw
        // override: see `debug_models_dir_override`).
        cfg.models_dir = resolve_models_dir(&resolve_app, cfg.asr_model, cfg.diarize)?;

        // Timed from model load to the finished transcript: what the person
        // waits for, and what the Import estimate learns from.
        let started = std::time::Instant::now();
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
        let transcript =
            process(&sherpa, &samples, sample_rate, &cfg, &progress).map_err(|e| e.to_string())?;
        crate::device::record_run(
            &run_app,
            crate::device::Run {
                audio_s: samples.len() as f64 / sample_rate as f64,
                elapsed_s: started.elapsed().as_secs_f64(),
                diarize: cfg.diarize,
            },
        );
        Ok(transcript)
    })
    .await
    .map_err(|e| format!("transcription task failed: {e}"))?
}
