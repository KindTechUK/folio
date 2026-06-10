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

pub mod merge;
pub mod sherpa;

pub use sherpa::SherpaEngine;

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

/// Which Whisper model to use. Default is `SmallEn` (the spike recommendation:
/// best RTF/quality trade on CPU).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AsrModel {
    #[default]
    SmallEn,
    MediumEn,
}

impl AsrModel {
    /// Model subdirectory name under `models_dir` and the file-name stem the
    /// downloaded models use, e.g. `sherpa-onnx-whisper-small.en` /
    /// `small.en-encoder.int8.onnx`.
    pub fn dir(&self) -> &'static str {
        match self {
            AsrModel::SmallEn => "sherpa-onnx-whisper-small.en",
            AsrModel::MediumEn => "sherpa-onnx-whisper-medium.en",
        }
    }

    pub fn stem(&self) -> &'static str {
        match self {
            AsrModel::SmallEn => "small.en",
            AsrModel::MediumEn => "medium.en",
        }
    }
}

/// Engine configuration. `num_threads` defaults to 4 and `asr_model` to
/// `SmallEn` (the spike defaults).
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Directory holding all model files (the project's `models/`).
    pub models_dir: PathBuf,
    pub asr_model: AsrModel,
    pub num_threads: i32,
    /// Fixed speaker count, or `None` to auto-detect via the clustering
    /// threshold.
    pub num_speakers: Option<u32>,
}

impl EngineConfig {
    pub fn new(models_dir: impl AsRef<Path>) -> Self {
        Self {
            models_dir: models_dir.as_ref().to_path_buf(),
            asr_model: AsrModel::default(),
            num_threads: 4,
            num_speakers: None,
        }
    }
}

/// Fallback speaker label when no diarized span can be matched to a word.
pub const UNKNOWN_SPEAKER: &str = "UNKNOWN";

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
    fn transcribe(&self, samples: &[f32], sample_rate: u32) -> anyhow::Result<Vec<AsrSegment>>;

    /// Diarize 16 kHz mono `samples` into speaker spans. `num_speakers = None`
    /// auto-detects.
    fn diarize(
        &self,
        samples: &[f32],
        sample_rate: u32,
        num_speakers: Option<u32>,
    ) -> anyhow::Result<Vec<SpeakerSpan>>;
}

/// Run the full pipeline: transcribe + diarize + merge into the contract.
pub fn process<E: TranscriptionEngine>(
    engine: &E,
    samples: &[f32],
    sample_rate: u32,
    cfg: &EngineConfig,
) -> anyhow::Result<Transcript> {
    let asr = engine.transcribe(samples, sample_rate)?;
    let spans = engine.diarize(samples, sample_rate, cfg.num_speakers)?;
    Ok(merge::merge(&asr, &spans))
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
