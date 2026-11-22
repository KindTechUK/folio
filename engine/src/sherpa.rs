//! `SherpaEngine`: the concrete [`TranscriptionEngine`] backed by the official
//! k2-fsa `sherpa-onnx` Rust crate — Whisper or Parakeet ASR over Silero VAD
//! chunks, plus pyannote + 3D-Speaker + FastClustering diarization.

use crate::{
    AsrSegment, EngineConfig, EngineError, Progress, ProgressSink, SpeakerSpan,
    TranscriptionEngine, Word,
};
use anyhow::Context;
use std::path::{Path, PathBuf};

use sherpa_onnx::{
    FastClusteringConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineSpeakerDiarization,
    OfflineSpeakerDiarizationConfig, OfflineSpeakerSegmentationModelConfig,
    OfflineSpeakerSegmentationPyannoteModelConfig, OfflineTransducerModelConfig,
    OfflineWhisperModelConfig, SileroVadModelConfig, SpeakerEmbeddingExtractorConfig,
    VadModelConfig, VoiceActivityDetector,
};

use crate::{EMB_MODEL, SEG_MODEL, VAD_MODEL};

/// VAD segmentation tuning per model family, as
/// `(min_silence_duration_s, max_speech_duration_s)`.
///
/// `min_silence_duration` is the "redemption" time: how long a pause must last
/// before a speech segment is closed. `max_speech_duration` caps a segment's
/// length (Silero cuts at the cap).
///
/// Folio is an offline/import workflow (no live-caption latency budget), so for
/// the default transducer path (Parakeet) we bridge natural pauses into longer,
/// more coherent segments — better ASR context and cleaner review paragraphs.
/// The Whisper path is left at its validated params: it MUST stay under the 30 s
/// decode window, so a raised cap there would silently truncate audio.
fn vad_params(is_transducer: bool) -> (f32, f32) {
    if is_transducer {
        // Offline transducer: ~0.8 s redemption bridges breaths/hesitations;
        // 120 s cap has plenty of headroom (Parakeet has no 30 s window) while
        // still bounding memory and keeping progress ticks flowing.
        (0.8, 120.0)
    } else {
        // Whisper: stay under the 30 s decode window.
        (0.25, 25.0)
    }
}

/// VAD segmentation profile: `min_silence` closes an utterance after that much
/// pause, `max_speech` caps its length (seconds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadProfile {
    pub min_silence: f32,
    pub max_speech: f32,
}

impl VadProfile {
    /// Live capture: utterances close after 0.8 s of silence or 15 s of speech,
    /// which bounds how far text trails speech (spike S1:
    /// docs/spikes/s1-live-lag.md).
    pub const LIVE: VadProfile = VadProfile { min_silence: 0.8, max_speech: 15.0 };

    /// File transcription for `model` (see [`vad_params`]).
    pub fn offline(model: crate::AsrModel) -> Self {
        let (min_silence, max_speech) = vad_params(model.is_transducer());
        VadProfile { min_silence, max_speech }
    }
}

/// Silero VAD that turns a stream of 16 kHz mono samples into utterances.
/// Utterance start positions count samples from creation (or the last
/// [`Vad::reset`]).
pub struct Vad {
    inner: VoiceActivityDetector,
    window: usize,
}

impl Vad {
    /// Feed samples (any length); returns utterances closed so far as
    /// `(start_sample, samples)`.
    pub fn accept(&self, samples: &[f32]) -> Vec<(usize, Vec<f32>)> {
        let mut out = Vec::new();
        for chunk in samples.chunks(self.window) {
            self.inner.accept_waveform(chunk);
            self.drain(&mut out);
        }
        out
    }

    /// Close any open utterance (end of input, or pause).
    pub fn flush(&self) -> Vec<(usize, Vec<f32>)> {
        self.inner.flush();
        let mut out = Vec::new();
        self.drain(&mut out);
        out
    }

    /// Clear all state; sample positions restart at 0.
    pub fn reset(&self) {
        self.inner.reset();
    }

    /// Whether speech is currently detected (an utterance is open).
    pub fn speaking(&self) -> bool {
        self.inner.detected()
    }

    fn drain(&self, out: &mut Vec<(usize, Vec<f32>)>) {
        while let Some(seg) = self.inner.front() {
            out.push((seg.start().max(0) as usize, seg.samples().to_vec()));
            self.inner.pop();
        }
    }
}

/// A loaded ASR model that decodes one utterance at a time. Building one loads
/// the model (seconds for Parakeet), so hold it for a whole session.
pub struct Recognizer {
    inner: OfflineRecognizer,
    sample_rate: u32,
}

impl Recognizer {
    /// Decode one utterance that starts `start_sample` samples into the audio.
    /// Word and segment times are absolute (seconds from the start of the
    /// audio). `None` when nothing was recognised.
    pub fn decode_utterance(&self, start_sample: usize, samples: &[f32]) -> Option<AsrSegment> {
        let stream = self.inner.create_stream();
        stream.accept_waveform(self.sample_rate as i32, samples);
        self.inner.decode(&stream);
        let result = stream.get_result()?;
        let text = result.text.trim().to_string();
        if text.is_empty() {
            return None;
        }
        let sr = self.sample_rate as f64;
        let offset = start_sample as f64 / sr;
        let seg_start = round3(offset);
        let seg_end = round3(offset + samples.len() as f64 / sr);
        let timestamps = result.timestamps.clone().unwrap_or_default();
        let words =
            group_tokens_to_words(&result.tokens, &timestamps, offset, seg_start, seg_end, &text);
        Some(AsrSegment { start: seg_start, end: seg_end, text, words })
    }
}

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
        let asr_dir = self.models_dir.join(self.asr_model.dir());
        let p = |name: &str| asr_dir.join(name).to_string_lossy().into_owned();

        let mut config = OfflineRecognizerConfig::default();
        if self.asr_model.is_transducer() {
            // NeMo Parakeet-TDT transducer. `model_type` MUST be
            // "nemo_transducer" for the TDT/RNN-T NeMo export. `greedy_search`
            // is deliberate: sherpa-onnx #3267 documents modified_beam_search
            // hallucinating / returning empty ~20% of the time on Parakeet-TDT.
            // Native per-token timestamps come back in the result with no flag.
            config.model_config.transducer = OfflineTransducerModelConfig {
                encoder: Some(p("encoder.int8.onnx")),
                decoder: Some(p("decoder.int8.onnx")),
                joiner: Some(p("joiner.int8.onnx")),
            };
            config.model_config.model_type = Some("nemo_transducer".to_string());
            config.model_config.tokens = Some(p("tokens.txt"));
            config.decoding_method = Some("greedy_search".to_string());
        } else {
            let stem = self.asr_model.stem();
            config.model_config.whisper = OfflineWhisperModelConfig {
                encoder: Some(p(&format!("{stem}-encoder.int8.onnx"))),
                decoder: Some(p(&format!("{stem}-decoder.int8.onnx"))),
                language: Some(String::new()), // .en models: empty == default
                task: Some("transcribe".to_string()),
                tail_paddings: 0,
                enable_token_timestamps: true, // word-level timing source
                enable_segment_timestamps: false,
            };
            config.model_config.tokens = Some(p(&format!("{stem}-tokens.txt")));
        }
        config.model_config.provider = Some("cpu".to_string());
        config.model_config.num_threads = self.num_threads;
        config.model_config.debug = false;

        OfflineRecognizer::create(&config)
            .ok_or_else(|| EngineError::Sherpa("create OfflineRecognizer returned None".into()).into())
    }

    /// Load the ASR model for utterance-by-utterance decoding at 16 kHz.
    pub fn recognizer(&self) -> anyhow::Result<Recognizer> {
        Ok(Recognizer { inner: self.build_recognizer()?, sample_rate: 16_000 })
    }

    /// A Silero VAD for 16 kHz mono with the given profile.
    pub fn vad(&self, profile: VadProfile) -> anyhow::Result<Vad> {
        self.vad_at(profile, 16_000)
    }

    fn vad_at(&self, profile: VadProfile, sample_rate: u32) -> anyhow::Result<Vad> {
        let mut silero = SileroVadModelConfig::default();
        silero.model = Some(self.model_path(VAD_MODEL));
        silero.threshold = 0.5;
        silero.min_silence_duration = profile.min_silence;
        silero.min_speech_duration = 0.25;
        silero.max_speech_duration = profile.max_speech;

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

        // Initial buffer size in seconds; sherpa-onnx grows it (without data
        // loss) if a speech run exceeds it.
        let inner = VoiceActivityDetector::create(&vad_config, 100.0)
            .ok_or_else(|| EngineError::Sherpa("create VoiceActivityDetector returned None".into()))?;
        Ok(Vad { inner, window })
    }

    /// VAD-chunk `samples` into `(start_sample, chunk_samples)` speech segments
    /// with the model's offline profile (see [`vad_params`]).
    fn vad_chunks(
        &self,
        samples: &[f32],
        sample_rate: u32,
        progress: &ProgressSink<'_>,
    ) -> anyhow::Result<Vec<(usize, Vec<f32>)>> {
        let vad = self.vad_at(VadProfile::offline(self.asr_model), sample_rate)?;

        let mut chunks: Vec<(usize, Vec<f32>)> = Vec::new();
        // Emit "segmenting" progress across [PROGRESS_DECODE_END, PROGRESS_VAD_END]
        // as the full-file VAD pass advances (throttled to whole-percent steps).
        let total = samples.len().max(1);
        let mut processed = 0usize;
        let mut last_pct: u32 = u32::MAX;
        for chunk in samples.chunks(vad.window) {
            chunks.extend(vad.accept(chunk));
            processed += chunk.len();
            let pct = (100 * processed / total) as u32;
            if pct != last_pct {
                last_pct = pct;
                let frac = crate::PROGRESS_DECODE_END
                    + (crate::PROGRESS_VAD_END - crate::PROGRESS_DECODE_END)
                        * (processed as f32 / total as f32);
                progress(Progress::new("segmenting", processed as u32, total as u32, frac));
            }
        }
        chunks.extend(vad.flush());
        Ok(chunks)
    }
}

/// Group tokens into words on the word-start marker (leading space, `Ġ`, or
/// `▁`), converting chunk-relative token timestamps to absolute seconds via
/// `offset`; falls back to spreading the chunk span uniformly when timestamps
/// are missing.
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
            // Word-start markers: Whisper BPE uses a leading space / `Ġ`;
            // Parakeet's SentencePiece uses `▁` (U+2581). Handle both so the
            // same grouping serves the Whisper and transducer paths.
            let is_new = tok.starts_with(' ') || tok.starts_with('Ġ') || tok.starts_with('▁');
            let clean = tok.replace('Ġ', " ").replace('▁', " ");
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
        // Model load (esp. the ~600 MB Parakeet encoder) is a real wait; surface
        // it as its own "loading" step.
        progress(Progress::new("loading", 0, 0, crate::PROGRESS_DECODE_END));
        let recognizer = Recognizer { inner: self.build_recognizer()?, sample_rate };
        let chunks = self
            .vad_chunks(samples, sample_rate, progress)
            .context("VAD chunking")?;

        let total = chunks.len() as u32;
        let mut segments = Vec::new();
        for (k, (start_sample, chunk)) in chunks.iter().enumerate() {
            // One tick per VAD chunk, filling [PROGRESS_VAD_END,
            // PROGRESS_TRANSCRIBE_END] and leaving headroom for diarize/done.
            let done = (k + 1) as u32;
            let span = crate::PROGRESS_TRANSCRIBE_END - crate::PROGRESS_VAD_END;
            let frac = if total > 0 {
                crate::PROGRESS_VAD_END + span * (done as f32 / total as f32)
            } else {
                crate::PROGRESS_TRANSCRIBE_END
            };
            progress(Progress::new("transcribing", done, total, frac));

            if let Some(seg) = recognizer.decode_utterance(*start_sample, chunk) {
                segments.push(seg);
            }
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
                    ..Default::default()
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
                ..Default::default()
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

    /// VAD params are model-aware: Whisper must stay under its 30 s window; the
    /// offline transducer path may run long and bridges pauses at least as much.
    #[test]
    fn vad_params_are_model_aware() {
        let (whisper_sil, whisper_max) = vad_params(false);
        assert_eq!(whisper_max, 25.0, "whisper cap must be the validated 25s");
        assert!(whisper_max < 30.0, "whisper must stay under its 30s window");

        let (par_sil, par_max) = vad_params(true);
        assert!(par_max > 30.0, "transducer has no 30s limit — allow long segments");
        assert!(
            par_sil >= whisper_sil,
            "transducer bridges pauses at least as aggressively as whisper"
        );
    }

    /// Lay down EMPTY placeholder files at every relative path the engine needs
    /// for the DEFAULT model WITHOUT diarization (its ASR files + Silero VAD).
    /// Uses `AsrModel::default()` so the test tracks whichever model is default.
    fn write_asr_vad_placeholders(root: &Path) {
        for rel in crate::required_model_files(AsrModel::default(), false) {
            let p = root.join(&rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, b"").unwrap();
        }
    }

    /// `new()` does fs-existence checks only: with the default model's ASR+VAD
    /// files present
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
