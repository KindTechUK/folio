//! In-process, CPU-only ASR + speaker diarization engine.
//!
//! Wraps the official k2-fsa `sherpa-onnx` Rust crate and emits the project's
//! JSON transcript contract (`core/src/types.ts`): `{ segments: [{ start, end,
//! text, speaker, words: [{ word, start, end, speaker }] }] }`.
//!
//! This is a Rust port of the validated Python spike scripts under `models/`
//! (`asr_spike.py`, `diar_spike.py`, `merge_spike.py`). Parameters are matched
//! verbatim — the de-risk for the eventual Windows-CPU build is proving the
//! native linking and the contract, not re-tuning the pipeline.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod decode;
pub mod merge;
pub mod sherpa;

pub use decode::{decode_to_16k_mono, decode_to_16k_mono_with_progress};
pub use sherpa::SherpaEngine;

/// A progress update emitted during the pipeline so the UI can render a
/// determinate bar instead of a blocking spinner. `fraction` is in `[0, 1]`.
///
/// Serializes to the shape consumed by `app/src/progress.ts` (the Tauri
/// `transcribe-progress` event payload).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Progress {
    /// One of `"decoding" | "transcribing" | "diarizing" | "done"`.
    pub stage: String,
    /// Units completed so far within the current stage.
    pub done: u32,
    /// Total units in the current stage (`0` when indeterminate).
    pub total: u32,
    /// Overall completion fraction in `[0, 1]`.
    pub fraction: f32,
}

impl Progress {
    /// Build a `Progress` clamping `fraction` into `[0, 1]`.
    pub fn new(stage: &str, done: u32, total: u32, fraction: f32) -> Self {
        Self {
            stage: stage.to_string(),
            done,
            total,
            fraction: fraction.clamp(0.0, 1.0),
        }
    }
}

/// A progress sink: a `Sync` callback so it can be shared across the
/// `spawn_blocking` boundary in the Tauri command. Tests pass a no-op `&|_| {}`.
pub type ProgressSink<'a> = dyn Fn(Progress) + Sync + 'a;

/// Progress-bar fraction bands, so the pipeline's phases map onto ONE monotonic
/// [0,1] bar with honest per-phase advancement (see the `transcribe` command and
/// [`sherpa::SherpaEngine::transcribe`]): decode → `[0, DECODE_END]`, model load
/// holds at `DECODE_END` ("loading"), VAD → `[DECODE_END, VAD_END]`
/// ("segmenting"), transcription → `[VAD_END, TRANSCRIBE_END]`, then
/// diarize/done fill the remainder. Before this, the whole prefix showed a
/// single frozen "decoding 0%".
pub const PROGRESS_DECODE_END: f32 = 0.15;
pub const PROGRESS_VAD_END: f32 = 0.30;
pub const PROGRESS_TRANSCRIBE_END: f32 = 0.95;

/// A single recognized word with timing and (optionally) its diarized speaker.
///
/// Mirrors `Word` in `core/src/types.ts`. `speaker` serializes to `null` when
/// absent. Word-level timestamps are a PLAYBACK AID ONLY (per the PRD), not an
/// edit surface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Word {
    pub word: String,
    pub start: f64,
    pub end: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

/// An engine segment carrying word-level detail. Mirrors `Segment` in
/// `core/src/types.ts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub words: Vec<Word>,
}

/// The top-level sidecar. Serializes to `{ "segments": [...] }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transcript {
    pub segments: Vec<Segment>,
}

/// A diarization output span: `speaker` is talking from `start` to `end`
/// (seconds). Mirrors `SpeakerSegment` in `core/src/types.ts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpeakerSpan {
    pub speaker: String,
    pub start: f64,
    pub end: f64,
}

/// An ASR segment before speaker assignment: timing + text + words (words carry
/// no speaker yet).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsrSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub words: Vec<Word>,
}

/// Which ASR model to use. Default is `ParakeetV2` (English): on CPU it is the
/// fastest by far and the only one of our models that emits native word-level
/// timestamps. `SmallEn`/`MediumEn` (Whisper) remain available for non-English
/// or hard/noisy audio.
///
/// Two families are supported through the SAME sherpa-onnx `OfflineRecognizer`:
/// the Whisper encoder/decoder models (`SmallEn`/`MediumEn`) and the NeMo
/// transducer `ParakeetV2`. They differ only in the recognizer config and the
/// on-disk file layout (see [`AsrModel::asr_files`] and
/// [`sherpa::SherpaEngine::build_recognizer`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AsrModel {
    SmallEn,
    MediumEn,
    /// NVIDIA Parakeet-TDT-0.6B-v2 (English), an int8 NeMo transducer run via
    /// sherpa-onnx. On CPU it is ~6x faster than `small.en` and ~24x faster than
    /// `medium.en` (see `models/run_asr_bench.sh`) and emits NATIVE word-level
    /// timestamps — unlike our int8 Whisper decoders, which lack cross-attention
    /// outputs and fall back to spread timings. The default. Trade-off:
    /// English-only, so callers handling other languages must select Whisper.
    #[default]
    ParakeetV2,
}

impl AsrModel {
    /// Model subdirectory name under `models_dir`, e.g.
    /// `sherpa-onnx-whisper-small.en` or
    /// `sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8`.
    pub fn dir(&self) -> &'static str {
        match self {
            AsrModel::SmallEn => "sherpa-onnx-whisper-small.en",
            AsrModel::MediumEn => "sherpa-onnx-whisper-medium.en",
            AsrModel::ParakeetV2 => "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8",
        }
    }

    /// Whisper file-name stem (e.g. `small.en` in `small.en-encoder.int8.onnx`).
    /// A Whisper-only naming concept; the transducer layout has no stem, so
    /// `ParakeetV2` returns a display-only identifier that is NOT used to build
    /// file paths (see [`AsrModel::asr_files`]).
    pub fn stem(&self) -> &'static str {
        match self {
            AsrModel::SmallEn => "small.en",
            AsrModel::MediumEn => "medium.en",
            AsrModel::ParakeetV2 => "parakeet-tdt-0.6b-v2",
        }
    }

    /// True for NeMo transducer models (encoder/decoder/joiner + greedy decode,
    /// native word timestamps) vs Whisper encoder/decoder models.
    pub fn is_transducer(&self) -> bool {
        matches!(self, AsrModel::ParakeetV2)
    }

    /// The model-relative ASR file paths (relative to `models_dir`), NOT
    /// including the Silero VAD or diarization models. Whisper models use the
    /// `<stem>-encoder/decoder/tokens` layout; transducer models use
    /// `encoder/decoder/joiner.int8.onnx` + `tokens.txt`. This is the single
    /// source of truth [`required_model_files`] builds on.
    pub fn asr_files(&self) -> Vec<String> {
        let dir = self.dir();
        if self.is_transducer() {
            vec![
                format!("{dir}/encoder.int8.onnx"),
                format!("{dir}/decoder.int8.onnx"),
                format!("{dir}/joiner.int8.onnx"),
                format!("{dir}/tokens.txt"),
            ]
        } else {
            let stem = self.stem();
            vec![
                format!("{dir}/{stem}-encoder.int8.onnx"),
                format!("{dir}/{stem}-decoder.int8.onnx"),
                format!("{dir}/{stem}-tokens.txt"),
            ]
        }
    }
}

/// Engine configuration. `num_threads` defaults to 4 and `asr_model` to
/// `ParakeetV2` (the fastest CPU model with native word timestamps; select a
/// Whisper model for non-English audio).
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Directory holding all model files (the project's `models/`).
    pub models_dir: PathBuf,
    pub asr_model: AsrModel,
    pub num_threads: i32,
    /// Fixed speaker count, or `None` to auto-detect via the clustering
    /// threshold.
    pub num_speakers: Option<u32>,
    /// When `true`, run speaker diarization and merge speakers into the
    /// transcript. When `false` (the DEFAULT — see PR #4 / the PRD), transcribe
    /// only and emit speaker-less segments/words for plain "paragraph QA" mode.
    /// Measured speaker quality on real audio was poor enough that labels hurt,
    /// so diarization is opt-in; the code path is kept for multi-party use.
    pub diarize: bool,
}

impl EngineConfig {
    pub fn new(models_dir: impl AsRef<Path>) -> Self {
        Self {
            models_dir: models_dir.as_ref().to_path_buf(),
            asr_model: AsrModel::default(),
            num_threads: 4,
            num_speakers: None,
            diarize: false,
        }
    }
}

/// Fallback speaker label when no diarized span can be matched to a word.
pub const UNKNOWN_SPEAKER: &str = "UNKNOWN";

/// Silero VAD model file (relative to `models_dir`). Always required.
pub const VAD_MODEL: &str = "silero_vad.onnx";
/// Pyannote segmentation model (relative to `models_dir`). Diarization only.
pub const SEG_MODEL: &str = "sherpa-onnx-pyannote-segmentation-3-0/model.onnx";
/// 3D-Speaker embedding model (relative to `models_dir`). Diarization only.
pub const EMB_MODEL: &str = "3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx";

/// The RELATIVE model-file paths (forward-slash) the engine needs for the given
/// `asr_model` / `diarize` combination. The single source of truth shared by
/// [`SherpaEngine::new`]'s existence check and the verified-pack resolver, so
/// the two can never drift.
///
/// Always: the model's ASR files (via [`AsrModel::asr_files`] — Whisper's
/// encoder/decoder/tokens, or a transducer's encoder/decoder/joiner/tokens) and
/// the Silero VAD model. When `diarize`: also the pyannote segmentation and
/// 3D-Speaker embedding models.
pub fn required_model_files(asr_model: AsrModel, diarize: bool) -> Vec<String> {
    let mut files = asr_model.asr_files();
    files.push(VAD_MODEL.to_string());
    if diarize {
        files.push(SEG_MODEL.to_string());
        files.push(EMB_MODEL.to_string());
    }
    files
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("model file not found: {0}")]
    ModelNotFound(PathBuf),
    #[error("audio error: {0}")]
    Audio(String),
    #[error("sherpa-onnx error: {0}")]
    Sherpa(String),
}

/// The stable adapter interface (PRD user story #30). An implementation does the
/// two heavy steps; `process` stitches their outputs into the contract via
/// [`merge`].
pub trait TranscriptionEngine {
    /// Transcribe 16 kHz mono `samples` into word-bearing segments.
    ///
    /// `progress` is called as the transcription advances (e.g. once per VAD
    /// chunk) with the `"transcribing"` stage so the UI can show a determinate
    /// bar. Implementations that have no incremental progress may ignore it.
    fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        progress: &ProgressSink<'_>,
    ) -> anyhow::Result<Vec<AsrSegment>>;

    /// Diarize 16 kHz mono `samples` into speaker spans. `num_speakers = None`
    /// auto-detects.
    fn diarize(
        &self,
        samples: &[f32],
        sample_rate: u32,
        num_speakers: Option<u32>,
    ) -> anyhow::Result<Vec<SpeakerSpan>>;
}

/// Run the pipeline into the contract.
///
/// When `cfg.diarize` is `true`: transcribe + diarize + merge (speaker labels).
/// When `false` (the default): transcribe ONLY and build a speaker-less
/// transcript (segment + word `speaker: None`) for plain paragraph QA mode —
/// `diarize` is never called.
/// `progress` is the sink for [`Progress`] events: `process` emits `decoding`
/// (0%) up front, forwards the engine's per-chunk `transcribing` updates,
/// brackets the diarize step with `diarizing`, and emits `done` (100%) last.
/// Pass `&|_| {}` to ignore progress.
pub fn process<E: TranscriptionEngine>(
    engine: &E,
    samples: &[f32],
    sample_rate: u32,
    cfg: &EngineConfig,
    progress: &ProgressSink<'_>,
) -> anyhow::Result<Transcript> {
    // Decode progress is emitted by the caller (it owns the file→samples step)
    // and model-load/VAD/transcription progress by the engine, all onto the
    // shared PROGRESS_* bands. `process` just forwards those and brackets the
    // optional diarize step, then emits `done`.
    let asr = engine.transcribe(samples, sample_rate, progress)?;

    let transcript = if cfg.diarize {
        progress(Progress::new("diarizing", 0, 1, 0.95));
        let spans = engine.diarize(samples, sample_rate, cfg.num_speakers)?;
        merge::merge(&asr, &spans)
    } else {
        merge::from_asr(&asr)
    };

    progress(Progress::new("done", 1, 1, 1.0));
    Ok(transcript)
}

/// Load a WAV as 16 kHz mono `f32` samples in `[-1, 1)`.
///
/// Asserts 16 kHz, single channel, 16-bit PCM (matching the spike's
/// `i16 / 32768.0` normalization). Returns `(samples, sample_rate)`.
pub fn load_wav_16k_mono(path: impl AsRef<Path>) -> anyhow::Result<(Vec<f32>, u32)> {
    let path = path.as_ref();
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| EngineError::Audio(format!("opening {}: {e}", path.display())))?;
    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(EngineError::Audio(format!(
            "expected mono, got {} channels",
            spec.channels
        ))
        .into());
    }
    if spec.sample_rate != 16_000 {
        return Err(EngineError::Audio(format!(
            "expected 16000 Hz, got {} Hz",
            spec.sample_rate
        ))
        .into());
    }
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(EngineError::Audio(format!(
            "expected 16-bit PCM, got {:?} / {} bits",
            spec.sample_format, spec.bits_per_sample
        ))
        .into());
    }
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.map(|v| v as f32 / 32768.0))
        .collect::<Result<_, _>>()
        .map_err(|e| EngineError::Audio(format!("decoding samples: {e}")))?;
    Ok((samples, spec.sample_rate))
}

#[cfg(test)]
mod process_tests {
    use super::*;
    use std::cell::Cell;

    /// A model-free `TranscriptionEngine`: returns canned ASR segments and
    /// records whether `diarize` was called.
    struct MockEngine {
        diarize_called: Cell<bool>,
    }

    impl MockEngine {
        fn new() -> Self {
            Self { diarize_called: Cell::new(false) }
        }
    }

    impl TranscriptionEngine for MockEngine {
        fn transcribe(
            &self,
            _samples: &[f32],
            _sample_rate: u32,
            progress: &ProgressSink<'_>,
        ) -> anyhow::Result<Vec<AsrSegment>> {
            // Emit two transcribing ticks to mimic a 2-chunk VAD pass.
            progress(Progress::new("transcribing", 1, 2, 0.5));
            progress(Progress::new("transcribing", 2, 2, 0.9));
            Ok(vec![AsrSegment {
                start: 0.0,
                end: 2.0,
                text: "hello world".into(),
                words: vec![
                    Word { word: "hello".into(), start: 0.0, end: 1.0, speaker: None },
                    Word { word: "world".into(), start: 1.0, end: 2.0, speaker: None },
                ],
            }])
        }

        fn diarize(
            &self,
            _samples: &[f32],
            _sample_rate: u32,
            _num_speakers: Option<u32>,
        ) -> anyhow::Result<Vec<SpeakerSpan>> {
            self.diarize_called.set(true);
            Ok(vec![
                SpeakerSpan { speaker: "SPEAKER_00".into(), start: 0.0, end: 1.0 },
                SpeakerSpan { speaker: "SPEAKER_01".into(), start: 1.0, end: 2.0 },
            ])
        }
    }

    #[test]
    fn process_without_diarize_emits_no_speakers_and_skips_diarize() {
        let engine = MockEngine::new();
        let cfg = EngineConfig::new("models"); // diarize defaults to false
        assert!(!cfg.diarize, "diarize must default to false");

        let t = process(&engine, &[0.0_f32; 16], 16_000, &cfg, &|_| {}).unwrap();

        assert!(!engine.diarize_called.get(), "diarize must NOT be called when off");
        for seg in &t.segments {
            assert_eq!(seg.speaker, None, "segment speaker must be None");
            for w in &seg.words {
                assert_eq!(w.speaker, None, "word speaker must be None");
            }
        }
    }

    #[test]
    fn process_with_diarize_merges_speakers() {
        let engine = MockEngine::new();
        let mut cfg = EngineConfig::new("models");
        cfg.diarize = true;

        let t = process(&engine, &[0.0_f32; 16], 16_000, &cfg, &|_| {}).unwrap();

        assert!(engine.diarize_called.get(), "diarize must be called when on");
        let words = &t.segments[0].words;
        assert_eq!(words[0].speaker.as_deref(), Some("SPEAKER_00"));
        assert_eq!(words[1].speaker.as_deref(), Some("SPEAKER_01"));
        assert!(t.segments[0].speaker.is_some(), "segment speaker must be set");
    }

    #[test]
    fn process_emits_progress_with_monotonic_fraction_and_stages() {
        let engine = MockEngine::new();
        let mut cfg = EngineConfig::new("models");
        cfg.diarize = true; // exercise the diarizing stage too

        // The sink is `Sync` (it must cross `spawn_blocking` in the command), so
        // collect through a Mutex rather than a RefCell.
        let events = std::sync::Mutex::new(Vec::<Progress>::new());
        process(&engine, &[0.0_f32; 16], 16_000, &cfg, &|p| {
            events.lock().unwrap().push(p);
        })
        .unwrap();

        let events = events.into_inner().unwrap();
        assert!(events.len() >= 4, "expected several progress events");

        // Last event is done at 100%. (decode/load/VAD progress is emitted by
        // the caller/engine, not `process`, so the first event here is the
        // mock's transcribing tick.)
        let last = events.last().unwrap();
        assert_eq!(last.stage, "done");
        assert_eq!(last.fraction, 1.0);

        // Stages appear in pipeline order and fraction never decreases.
        let stages: Vec<&str> = events.iter().map(|p| p.stage.as_str()).collect();
        assert!(stages.contains(&"transcribing"));
        assert!(stages.contains(&"diarizing"));
        let mut prev = -1.0_f32;
        for p in &events {
            assert!(
                p.fraction >= prev - f32::EPSILON,
                "fraction must be monotonic non-decreasing, got {} after {}",
                p.fraction,
                prev
            );
            assert!((0.0..=1.0).contains(&p.fraction), "fraction in [0,1]");
            prev = p.fraction;
        }
    }

    #[test]
    fn progress_new_clamps_fraction() {
        assert_eq!(Progress::new("decoding", 0, 1, 2.0).fraction, 1.0);
        assert_eq!(Progress::new("decoding", 0, 1, -1.0).fraction, 0.0);
    }

    #[test]
    fn parakeet_uses_transducer_file_layout() {
        // Transducer layout: encoder/decoder/joiner + tokens (no `<stem>-`
        // prefix), plus Silero VAD. NO Whisper-style `-encoder.int8.onnx`.
        let files = required_model_files(AsrModel::ParakeetV2, false);
        let dir = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8";
        assert!(files.contains(&format!("{dir}/encoder.int8.onnx")));
        assert!(files.contains(&format!("{dir}/decoder.int8.onnx")));
        assert!(files.contains(&format!("{dir}/joiner.int8.onnx")), "transducer needs a joiner");
        assert!(files.contains(&format!("{dir}/tokens.txt")));
        assert!(files.contains(&VAD_MODEL.to_string()));
        assert!(AsrModel::ParakeetV2.is_transducer());
        assert!(
            !files.iter().any(|f| f.contains("-encoder.int8.onnx")),
            "transducer must not use the Whisper <stem>-encoder naming: {files:?}"
        );
    }

    #[test]
    fn whisper_still_uses_stem_layout_and_is_not_transducer() {
        let files = required_model_files(AsrModel::SmallEn, false);
        assert!(files.contains(&"sherpa-onnx-whisper-small.en/small.en-encoder.int8.onnx".to_string()));
        assert!(!AsrModel::SmallEn.is_transducer());
        assert!(!files.iter().any(|f| f.ends_with("joiner.int8.onnx")));
    }
}
