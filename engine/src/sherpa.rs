//! `SherpaEngine`: the concrete [`TranscriptionEngine`] backed by the official
//! k2-fsa `sherpa-onnx` Rust crate.
//!
//! Ports `models/asr_spike.py` (Whisper int8 + Silero VAD chunking +
//! token-timestamp word grouping) and `models/diar_spike.py` (pyannote
//! segmentation + 3D-Speaker embedding + FastClustering) verbatim.

use crate::{
    AsrSegment, EngineConfig, EngineError, Progress, ProgressSink, SpeakerSpan,
    TranscriptionEngine, Word,
};
use anyhow::Context;
use std::path::{Path, PathBuf};

use sherpa_onnx::{
    FastClusteringConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineSpeakerDiarization,
    OfflineSpeakerDiarizationConfig, OfflineSpeakerSegmentationModelConfig,
    OfflineSpeakerSegmentationPyannoteModelConfig, OfflineWhisperModelConfig,
    SileroVadModelConfig, SpeakerEmbeddingExtractorConfig, VadModelConfig, VoiceActivityDetector,
};

// Model file names — the single source of truth lives in `crate` (lib.rs), so
// the runtime path-building here and the resolver's required-file list can
// never drift.
use crate::{EMB_MODEL, SEG_MODEL, VAD_MODEL};

/// Concrete engine bound to a `models/` directory and a config (model choice,
/// thread count).
pub struct SherpaEngine {
    models_dir: PathBuf,
    asr_model: crate::AsrModel,
    num_threads: i32,
}

impl SherpaEngine {
    /// Build from an [`EngineConfig`], verifying all required model files exist
    /// up front (so missing files fail with a clear path, not a deep sherpa
    /// error).
    pub fn new(cfg: &EngineConfig) -> anyhow::Result<Self> {
        let dir = &cfg.models_dir;
        // Build the required list from the shared source of truth so the
        // diarization models are required ONLY when `cfg.diarize`.
        for rel in crate::required_model_files(cfg.asr_model, cfg.diarize) {
            let p = dir.join(&rel);
            if !p.exists() {
                return Err(EngineError::ModelNotFound(p).into());
            }
        }
        Ok(Self {
            models_dir: dir.clone(),
            asr_model: cfg.asr_model,
            num_threads: cfg.num_threads,
        })
    }

    fn model_path(&self, rel: impl AsRef<Path>) -> String {
        self.models_dir.join(rel).to_string_lossy().into_owned()
    }

    fn build_recognizer(&self) -> anyhow::Result<OfflineRecognizer> {
        let stem = self.asr_model.stem();
        let asr_dir = self.models_dir.join(self.asr_model.dir());
        let p = |name: String| asr_dir.join(name).to_string_lossy().into_owned();

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.whisper = OfflineWhisperModelConfig {
            encoder: Some(p(format!("{stem}-encoder.int8.onnx"))),
            decoder: Some(p(format!("{stem}-decoder.int8.onnx"))),
            language: Some(String::new()), // .en models: empty == default
            task: Some("transcribe".to_string()),
            tail_paddings: 0,
            enable_token_timestamps: true, // spike: word-level timing source
            enable_segment_timestamps: false,
        };
        config.model_config.tokens = Some(p(format!("{stem}-tokens.txt")));
        config.model_config.provider = Some("cpu".to_string());
        config.model_config.num_threads = self.num_threads;
        config.model_config.debug = false;

        OfflineRecognizer::create(&config)
            .ok_or_else(|| EngineError::Sherpa("create OfflineRecognizer returned None".into()).into())
    }

    /// VAD-chunk `samples` into `(start_sample, chunk_samples)` speech segments
    /// (spike params: threshold 0.5, min_silence/min_speech 0.25, max_speech
    /// 25.0 to stay under Whisper's 30 s window).
    fn vad_chunks(&self, samples: &[f32], sample_rate: u32) -> anyhow::Result<Vec<(usize, Vec<f32>)>> {
        let mut silero = SileroVadModelConfig::default();
        silero.model = Some(self.model_path(VAD_MODEL));
        silero.threshold = 0.5;
        silero.min_silence_duration = 0.25;
        silero.min_speech_duration = 0.25;
        silero.max_speech_duration = 25.0;

        let window = if silero.window_size > 0 {
            silero.window_size as usize
        } else {
            512
        };

        let vad_config = VadModelConfig {
            silero_vad: silero,
            ten_vad: Default::default(),
            sample_rate: sample_rate as i32,
            num_threads: 1,
            provider: Some("cpu".to_string()),
            debug: false,
        };

        // buffer_size_in_seconds large enough for long speech runs (spike: 100).
        let vad = VoiceActivityDetector::create(&vad_config, 100.0)
            .ok_or_else(|| EngineError::Sherpa("create VoiceActivityDetector returned None".into()))?;

        let mut chunks: Vec<(usize, Vec<f32>)> = Vec::new();
        for chunk in samples.chunks(window) {
            vad.accept_waveform(chunk);
            while let Some(seg) = vad.front() {
                chunks.push((seg.start().max(0) as usize, seg.samples().to_vec()));
                vad.pop();
            }
        }
        vad.flush();
        while let Some(seg) = vad.front() {
            chunks.push((seg.start().max(0) as usize, seg.samples().to_vec()));
            vad.pop();
        }
        Ok(chunks)
    }
}

/// Group BPE-ish whisper tokens into words on the leading-space / `Ġ` marker,
/// converting per-token chunk-relative timestamps to absolute seconds via
/// `offset`. Mirrors `asr_spike.py`.
fn group_tokens_to_words(
    tokens: &[String],
    timestamps: &[f32],
    offset: f64,
    seg_start: f64,
    seg_end: f64,
    text: &str,
) -> Vec<Word> {
    let mut words = Vec::new();
    let have_ts = !timestamps.is_empty() && timestamps.len() == tokens.len();
    if have_ts {
        let mut cur_word = String::new();
        let mut cur_start: Option<f64> = None;
        let mut cur_end: f64 = 0.0;
        for (tok, &ts) in tokens.iter().zip(timestamps.iter()) {
            let is_new = tok.starts_with(' ') || tok.starts_with('Ġ');
            let clean = tok.replace('Ġ', " ");
            if is_new && !cur_word.trim().is_empty() {
                words.push(Word {
                    word: cur_word.trim().to_string(),
                    start: round3(offset + cur_start.unwrap_or(0.0)),
                    end: round3(offset + cur_end),
                    speaker: None,
                });
                cur_word.clear();
                cur_start = None;
            }
            if cur_start.is_none() {
                cur_start = Some(ts as f64);
            }
            cur_word.push_str(&clean);
            cur_end = ts as f64;
        }
        if !cur_word.trim().is_empty() {
            words.push(Word {
                word: cur_word.trim().to_string(),
                start: round3(offset + cur_start.unwrap_or(0.0)),
                end: round3(offset + cur_end),
                speaker: None,
            });
        }
    } else {
        // Fallback: released decoders may omit token timestamps -> spread the
        // VAD-chunk span uniformly across whitespace-split words.
        let wlist: Vec<&str> = text.split_whitespace().collect();
        let span = (seg_end - seg_start).max(1e-3);
        let step = span / (wlist.len().max(1) as f64);
        for (k, wd) in wlist.iter().enumerate() {
            words.push(Word {
                word: wd.to_string(),
                start: round3(seg_start + k as f64 * step),
                end: round3(seg_start + (k as f64 + 1.0) * step),
                speaker: None,
            });
        }
    }
    words
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

impl TranscriptionEngine for SherpaEngine {
    fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        progress: &ProgressSink<'_>,
    ) -> anyhow::Result<Vec<AsrSegment>> {
        let recognizer = self.build_recognizer()?;
        let chunks = self
            .vad_chunks(samples, sample_rate)
            .context("VAD chunking")?;

        let total = chunks.len() as u32;
        let mut segments = Vec::new();
        for (k, (start_sample, chunk)) in chunks.iter().enumerate() {
            // One progress tick per VAD chunk. Scale into [0, 0.9] so the bar
            // leaves headroom for the diarizing/done markers `process` emits.
            let done = (k + 1) as u32;
            let frac = if total > 0 {
                0.9 * (done as f32 / total as f32)
            } else {
                0.9
            };
            progress(Progress::new("transcribing", done, total, frac));

            let stream = recognizer.create_stream();
            stream.accept_waveform(sample_rate as i32, chunk);
            recognizer.decode(&stream);
            let result = match stream.get_result() {
                Some(r) => r,
                None => continue,
            };
            let text = result.text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            let offset = *start_sample as f64 / sample_rate as f64;
            let seg_start = round3(offset);
            let seg_end = round3(offset + chunk.len() as f64 / sample_rate as f64);
            let timestamps = result.timestamps.clone().unwrap_or_default();
            let words = group_tokens_to_words(
                &result.tokens,
                &timestamps,
                offset,
                seg_start,
                seg_end,
                &text,
            );
            segments.push(AsrSegment {
                start: seg_start,
                end: seg_end,
                text,
                words,
            });
        }
        Ok(segments)
    }

    fn diarize(
        &self,
        samples: &[f32],
        sample_rate: u32,
        num_speakers: Option<u32>,
    ) -> anyhow::Result<Vec<SpeakerSpan>> {
        let config = OfflineSpeakerDiarizationConfig {
            segmentation: OfflineSpeakerSegmentationModelConfig {
                pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                    model: Some(self.model_path(SEG_MODEL)),
                },
                num_threads: self.num_threads,
                provider: Some("cpu".to_string()),
                ..Default::default()
            },
            embedding: SpeakerEmbeddingExtractorConfig {
                model: Some(self.model_path(EMB_MODEL)),
                num_threads: self.num_threads,
                provider: Some("cpu".to_string()),
                ..Default::default()
            },
            clustering: FastClusteringConfig {
                // -1 (auto via threshold) when no count given; else fixed count.
                num_clusters: num_speakers.map(|n| n as i32).unwrap_or(-1),
                threshold: 0.5,
            },
            min_duration_on: 0.3,
            min_duration_off: 0.5,
        };

        let sd = OfflineSpeakerDiarization::create(&config)
            .ok_or_else(|| EngineError::Sherpa("create OfflineSpeakerDiarization returned None".into()))?;

        if sd.sample_rate() as u32 != sample_rate {
            return Err(EngineError::Audio(format!(
                "diarization expects {} Hz, got {} Hz",
                sd.sample_rate(),
                sample_rate
            ))
            .into());
        }

        let result = sd
            .process(samples)
            .ok_or_else(|| EngineError::Sherpa("diarization process returned None".into()))?;

        let mut spans: Vec<SpeakerSpan> = result
            .sort_by_start_time()
            .into_iter()
            .map(|s| SpeakerSpan {
                speaker: format!("SPEAKER_{:02}", s.speaker),
                start: round3(s.start as f64),
                end: round3(s.end as f64),
            })
            .collect();
        spans.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        Ok(spans)
    }
}

#[cfg(test)]
mod new_tests {
    use super::*;
    use crate::AsrModel;

    /// Lay down EMPTY placeholder files at every relative path the engine needs
    /// for `small.en` WITHOUT diarization (the 3 ASR files + Silero VAD).
    fn write_asr_vad_placeholders(root: &Path) {
        for rel in crate::required_model_files(AsrModel::SmallEn, false) {
            let p = root.join(&rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, b"").unwrap();
        }
    }

    /// `new()` does fs-existence checks only: with the 4 ASR+VAD files present
    /// and `diarize=false` it succeeds, but `diarize=true` fails closed with a
    /// `ModelNotFound` pointing at a missing diarization model.
    #[test]
    fn new_requires_diarization_models_only_when_diarizing() {
        let tmp = std::env::temp_dir().join(format!("sherpa-new-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_asr_vad_placeholders(&tmp);

        let mut cfg = EngineConfig::new(&tmp);
        cfg.diarize = false;
        assert!(
            SherpaEngine::new(&cfg).is_ok(),
            "ASR+VAD files present and diarize=false must be Ok"
        );

        cfg.diarize = true;
        let msg = match SherpaEngine::new(&cfg) {
            Ok(_) => panic!("diarize=true must fail without seg/emb models"),
            Err(e) => e.to_string(),
        };
        assert!(
            msg.contains("model file not found"),
            "expected ModelNotFound, got: {msg}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
