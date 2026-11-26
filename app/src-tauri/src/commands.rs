//! Tauri IPC commands for transcription. In-process and CPU-only: audio never
//! leaves the machine and no network call is made.

use engine::{
    decode_to_16k_mono_with_progress, merge, AsrSegment, Diarizer, EngineConfig, SherpaEngine,
    TranscriptionEngine,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::models;

/// A new transcript, as Import, recovery and Stop return it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTranscript {
    pub(crate) transcript: engine::Transcript,
    /// Whether the transcript carries speaker labels.
    pub(crate) speakers_labelled: bool,
    /// Something the user should know about the result.
    pub(crate) note: Option<String>,
}

const NO_SPEAKER_PACK: &str = "The speaker-detection pack isn't installed, so this transcript has no speakers. Add it on the Models page.";

/// Finish a new transcript from the transcribed `segments`: label speakers
/// with `label` if one is given, apply the vocabulary file, and report `done`.
/// If labelling fails, the transcript has no speakers and the note says why,
/// after any `notes` already given.
pub(crate) fn finish_transcript(
    app: &AppHandle,
    segments: &[AsrSegment],
    label: Option<impl FnOnce() -> anyhow::Result<engine::Transcript>>,
    mut notes: Vec<String>,
) -> NewTranscript {
    let labelled = label.map(|label| {
        let at = engine::PROGRESS_TRANSCRIBE_END;
        let _ = app.emit("transcribe-progress", engine::Progress::new("diarizing", 0, 1, at));
        label()
    });
    let (mut transcript, speakers_labelled) = match labelled {
        Some(Ok(transcript)) => (transcript, true),
        Some(Err(e)) => {
            notes.push(unlabelled_note(&e));
            (merge::from_asr(segments), false)
        }
        None => (merge::from_asr(segments), false),
    };
    crate::vocab::apply(app, &mut transcript);
    let _ = app.emit("transcribe-progress", engine::Progress::new("done", 1, 1, 1.0));
    let note = (!notes.is_empty()).then(|| notes.join(" "));
    NewTranscript { transcript, speakers_labelled, note }
}

/// Speakers were asked for, but no speaker-detection pack is installed.
#[derive(Debug)]
struct NoSpeakerPack;

impl std::fmt::Display for NoSpeakerPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no speaker-detection pack is installed")
    }
}

impl std::error::Error for NoSpeakerPack {}

/// The note for a transcript whose speakers couldn't be labelled because of `e`.
fn unlabelled_note(e: &anyhow::Error) -> String {
    if e.is::<NoSpeakerPack>() {
        NO_SPEAKER_PACK.to_string()
    } else {
        format!("Speakers couldn't be labelled, so this transcript has none. ({e})")
    }
}

/// The diarizer to label speakers with, from the speaker-detection pack. Get
/// it before reading any audio: it fails fast when the pack is missing.
pub(crate) fn speaker_diarizer(app: &AppHandle, num_threads: i32) -> anyhow::Result<Diarizer> {
    diarizer_for(resolve_speaker_dir(app), num_threads)
}

/// The diarizer for a resolved speaker-models folder; `Ok(None)` means no
/// speaker-detection pack is installed.
fn diarizer_for(
    dir: Result<Option<std::path::PathBuf>, String>,
    num_threads: i32,
) -> anyhow::Result<Diarizer> {
    let dir = dir.map_err(anyhow::Error::msg)?.ok_or(NoSpeakerPack)?;
    Diarizer::new(dir, num_threads)
}

/// Diarize `samples` with `diarizer` (from [`speaker_diarizer`]) and attach
/// speakers to the transcribed `segments`.
pub(crate) fn label_speakers(
    diarizer: &Diarizer,
    samples: &[f32],
    sample_rate: u32,
    segments: &[AsrSegment],
    num_speakers: Option<u32>,
) -> anyhow::Result<engine::Transcript> {
    let spans = diarizer.diarize(samples, sample_rate, num_speakers)?;
    Ok(merge::merge(segments, &spans))
}

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
) -> Result<std::path::PathBuf, String> {
    #[cfg(debug_assertions)]
    if let Some(raw) = debug_models_dir_override() {
        return Ok(raw);
    }
    models::resolve_verified_models_dir(app, asr_model)
}

/// Resolve the speaker-detection models' folder, or `None` when no verified
/// speaker-detection pack is installed (debug-only raw override: the override
/// folder, if it holds the speaker models). Blocking: may unpack the pack.
pub(crate) fn resolve_speaker_dir(app: &AppHandle) -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(debug_assertions)]
    if let Some(raw) = debug_models_dir_override() {
        let present = engine::speaker_model_files().iter().all(|file| raw.join(file).is_file());
        return Ok(present.then_some(raw));
    }
    models::resolve_verified_speaker_dir(app)
}

/// Transcribe an audio file (wav/mp3/m4a/mp4/flac/ogg…) into the project's JSON
/// contract.
///
/// Runs decode + ASR + optional diarize in `spawn_blocking` and streams progress
/// via the `transcribe-progress` event.
///
/// `diarize` defaults to `false` (speaker-less transcript); `Some(true)` opts
/// in, and `num_speakers = None` then auto-detects. If labelling fails, the
/// transcript comes back without speakers and with a note saying why.
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
) -> Result<NewTranscript, String> {
    // `models_dir` is left UNSET here on purpose: it is resolved INSIDE
    // spawn_blocking (verified-pack extraction may be large — keep the UI thread
    // free) and assigned just before `SherpaEngine::new`.
    let mut cfg = EngineConfig::new("");
    cfg.asr_model = parse_asr_model(model.as_deref());
    let diarize = diarize.unwrap_or(false);

    let resolve_app = app.clone();
    let run_app = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        // Resolve the models dir, enforcing verified-pack trust (debug-only raw
        // override: see `debug_models_dir_override`). Transcription models
        // only: speakers are labelled as a separate step below, from their own
        // pack, so a speaker pack that is missing or fails to unpack, load or
        // run doesn't cost the transcript.
        cfg.models_dir = resolve_models_dir(&resolve_app, cfg.asr_model)?;

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
        let segments =
            sherpa.transcribe(&samples, sample_rate, &progress).map_err(|e| e.to_string())?;
        let label = diarize.then_some(|| {
            let diarizer = speaker_diarizer(&run_app, cfg.num_threads)?;
            label_speakers(&diarizer, &samples, sample_rate, &segments, num_speakers)
        });
        let result = finish_transcript(&run_app, &segments, label, Vec::new());
        crate::device::record_run(
            &run_app,
            crate::device::Run {
                audio_s: samples.len() as f64 / sample_rate as f64,
                elapsed_s: started.elapsed().as_secs_f64(),
                diarize,
            },
        );
        Ok(result)
    })
    .await
    .map_err(|e| format!("transcription task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::resolve_verified_speaker_dir_in;
    use crate::models::tests::{broken_speaker_pack, copy_fixture_pack, fixtures_context, unique_tmp};
    use modelpack::PublicKey;
    use std::path::Path;

    /// The speaker step's note says why a transcript has no speakers: the pack
    /// isn't installed, or it couldn't be used.
    #[test]
    fn the_speaker_step_says_why_a_transcript_has_no_speakers() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);
        let packs = unique_tmp("note-packs");
        copy_fixture_pack("asr-small-en", &packs);
        let cache_base = unique_tmp("note-cache");
        let note = |packs: &Path| {
            let dir = resolve_verified_speaker_dir_in(packs, &cache_base, &pubkey, &versions);
            let err = diarizer_for(dir, 4).err().expect("no speaker models to label with");
            unlabelled_note(&err)
        };

        assert_eq!(
            note(&packs),
            "The speaker-detection pack isn't installed, so this transcript has no speakers. Add it on the Models page."
        );
        broken_speaker_pack(&packs);
        let broken = note(&packs);
        assert!(
            broken.starts_with("Speakers couldn't be labelled, so this transcript has none. ("),
            "got: {broken}"
        );

        let _ = std::fs::remove_dir_all(&packs);
        let _ = std::fs::remove_dir_all(&cache_base);
    }
}
