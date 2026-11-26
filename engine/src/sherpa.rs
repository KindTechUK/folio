//! `SherpaEngine`: the concrete [`TranscriptionEngine`] backed by the official
//! k2-fsa `sherpa-onnx` Rust crate — Whisper or Parakeet ASR over Silero VAD
//! chunks, plus pyannote + 3D-Speaker + FastClustering diarization.

use crate::{
    AsrSegment, Decoding, EngineConfig, EngineError, Hotwords, ModelFamily, Progress,
    ProgressSink, SpeakerSpan, TranscriptionEngine, Word,
};
use anyhow::Context;
use std::path::{Path, PathBuf};

use sherpa_onnx::{
    FastClusteringConfig, OfflineCohereTranscribeModelConfig, OfflineQwen3ASRModelConfig,
    OfflineRecognizer, OfflineRecognizerConfig, OfflineSpeakerDiarization,
    OfflineSpeakerDiarizationConfig, OfflineSpeakerSegmentationModelConfig,
    OfflineSpeakerSegmentationPyannoteModelConfig, OfflineTransducerModelConfig,
    OfflineWhisperModelConfig, SileroVadModelConfig, SpeakerEmbeddingExtractorConfig,
    VadModelConfig, VoiceActivityDetector,
};

use crate::segment::{energy_cuts, Leveller};
use crate::{EMB_MODEL, SEG_MODEL, VAD_MODEL};

/// The record with Parakeet (imports, and a recording's saved transcript):
/// pieces of about this many seconds, each cut at the quietest 100 ms within
/// [`PIECE_SEARCH_S`] of its mark, so 10–20 s long. Parakeet v2 can drop the
/// opening of a ~30 s piece (`tests/long_pieces.rs`); 15/5 scored the same as
/// 30/5 on the bench (docs/research/voice2text-lessons-2026-11.md).
pub const PIECE_S: f32 = 15.0;
pub const PIECE_SEARCH_S: f32 = 5.0;

/// VAD segmentation tuning per model family, as
/// `(min_silence_duration_s, max_speech_duration_s)`.
///
/// `min_silence_duration` is the "redemption" time: how long a pause must last
/// before a speech segment is closed. `max_speech_duration` caps a segment's
/// length: past it, sherpa-onnx asks for a shorter, clearer silence (0.1 s
/// at 0.9), so a segment usually ends soon after, but it isn't cut.
///
/// Parakeet's file path no longer uses these: it decodes energy-cut pieces
/// ([`PIECE_S`]). Its row stays for callers that ask for a VAD by model.
/// The Whisper path is left at its validated params: it MUST stay under the 30 s
/// decode window, so a raised cap there would silently truncate audio. The
/// encoder-decoder evaluation candidates (Qwen3-ASR, Cohere) get the same
/// conservative cap until the bench says otherwise.
fn vad_params(family: ModelFamily) -> (f32, f32) {
    match family {
        // Offline transducer: ~0.8 s redemption bridges breaths/hesitations;
        // 120 s cap has plenty of headroom (Parakeet has no 30 s window) while
        // still bounding memory and keeping progress ticks flowing.
        ModelFamily::Transducer => (0.8, 120.0),
        // Whisper: stay under the 30 s decode window.
        ModelFamily::Whisper => (0.25, 25.0),
        // Autoregressive decoders with a bounded output length: short segments.
        ModelFamily::Qwen3 | ModelFamily::Cohere => (0.8, 25.0),
    }
}

/// VAD segmentation profile: `min_silence` closes an utterance after that much
/// pause, `max_speech` caps its length (seconds). A `levelled` VAD judges a
/// copy of the audio raised by a [`Leveller`]: quiet and distant speech falls
/// under Silero's threshold otherwise (room-mic research, recommendation 2).
/// The utterances it returns are always the original audio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadProfile {
    pub min_silence: f32,
    pub max_speech: f32,
    pub levelled: bool,
}

impl VadProfile {
    /// Live capture: utterances close after 0.8 s of silence or 15 s of speech,
    /// which bounds how far text trails speech (spike S1:
    /// docs/spikes/s1-live-lag.md).
    pub const LIVE: VadProfile = VadProfile { min_silence: 0.8, max_speech: 15.0, levelled: true };

    /// File transcription for `model` (see [`vad_params`]).
    pub fn offline(model: crate::AsrModel) -> Self {
        let (min_silence, max_speech) = vad_params(model.family());
        VadProfile { min_silence, max_speech, levelled: true }
    }
}

/// The original samples a levelled VAD has been fed, kept long enough to cut
/// its utterances from them.
struct Originals {
    /// Stream position of `buf[0]`.
    base: usize,
    buf: Vec<f32>,
    /// How much [`Originals::trim`] keeps, for Silero's look-back once no
    /// utterance is open.
    keep: usize,
}

impl Originals {
    fn get(&self, start: usize, len: usize) -> Option<Vec<f32>> {
        let from = start.checked_sub(self.base)?;
        self.buf.get(from..from + len).map(<[f32]>::to_vec)
    }

    fn push(&mut self, samples: &[f32]) {
        self.buf.extend_from_slice(samples);
    }

    /// Drop all but the last `keep` samples. Only call it while no utterance
    /// is open: sherpa-onnx doesn't cut at `max_speech` (it only asks for a
    /// shorter, clearer silence), so an open one can be any length.
    fn trim(&mut self) {
        if self.buf.len() > 2 * self.keep {
            let drop = self.buf.len() - self.keep;
            self.buf.drain(..drop);
            self.base += drop;
        }
    }
}

/// Silero VAD that turns a stream of 16 kHz mono samples into utterances.
/// Utterance start positions count samples from creation (or the last
/// [`Vad::reset`]).
pub struct Vad {
    inner: VoiceActivityDetector,
    window: usize,
    sample_rate: u32,
    /// Set for a levelled profile.
    levelling: Option<std::cell::RefCell<(Leveller, Originals)>>,
}

impl Vad {
    /// Feed samples (any length); returns utterances closed so far as
    /// `(start_sample, samples)`.
    pub fn accept(&self, samples: &[f32]) -> Vec<(usize, Vec<f32>)> {
        let mut out = Vec::new();
        for chunk in samples.chunks(self.window) {
            // One window at a time, so the original copy never runs ahead of
            // what Silero has judged (and never trims what it still needs).
            match &self.levelling {
                Some(l) => {
                    let levelled = {
                        let (leveller, originals) = &mut *l.borrow_mut();
                        originals.push(chunk);
                        leveller.process(chunk)
                    };
                    self.inner.accept_waveform(&levelled);
                }
                None => self.inner.accept_waveform(chunk),
            }
            self.drain(&mut out);
            if let Some(l) = &self.levelling {
                if !self.inner.detected() {
                    l.borrow_mut().1.trim();
                }
            }
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
        if let Some(l) = &self.levelling {
            let (leveller, originals) = &mut *l.borrow_mut();
            *leveller = Leveller::new(self.sample_rate);
            originals.buf.clear();
            originals.base = 0;
        }
    }

    /// Whether speech is currently detected (an utterance is open).
    pub fn speaking(&self) -> bool {
        self.inner.detected()
    }

    fn drain(&self, out: &mut Vec<(usize, Vec<f32>)>) {
        while let Some(seg) = self.inner.front() {
            let start = seg.start().max(0) as usize;
            let heard = seg.samples();
            let samples = self
                .levelling
                .as_ref()
                .and_then(|l| l.borrow().1.get(start, heard.len()))
                .unwrap_or_else(|| heard.to_vec());
            out.push((start, samples));
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
        self.decode_utterance_timed(start_sample, samples).map(|(seg, _)| seg)
    }

    /// [`Recognizer::decode_utterance`], also saying whether the model returned
    /// native token timestamps (`false` means word times were spread evenly).
    pub fn decode_utterance_timed(
        &self,
        start_sample: usize,
        samples: &[f32],
    ) -> Option<(AsrSegment, bool)> {
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
        let native = !timestamps.is_empty() && timestamps.len() == result.tokens.len();
        let durations = result.durations.clone().unwrap_or_default();
        let words = group_tokens_to_words(
            &result.tokens,
            &timestamps,
            &durations,
            offset,
            seg_start,
            seg_end,
            &text,
        );
        Some((AsrSegment { start: seg_start, end: seg_end, text, words }, native))
    }
}

/// Concrete engine bound to a `models/` directory and a config (model choice,
/// thread count).
pub struct SherpaEngine {
    models_dir: PathBuf,
    asr_model: crate::AsrModel,
    num_threads: i32,
    decoding: Decoding,
    hotwords: Option<Hotwords>,
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
        if let Some(h) = &cfg.hotwords {
            if !h.file.exists() {
                return Err(EngineError::ModelNotFound(h.file.clone()).into());
            }
            if cfg.asr_model.is_transducer() && cfg.decoding == Decoding::Greedy {
                return Err(EngineError::Sherpa(
                    "hotwords on a transducer need beam search (Decoding::Beam)".into(),
                )
                .into());
            }
        }
        Ok(Self {
            models_dir: dir.clone(),
            asr_model: cfg.asr_model,
            num_threads: cfg.num_threads,
            decoding: cfg.decoding,
            hotwords: cfg.hotwords.clone(),
        })
    }

    fn model_path(&self, rel: impl AsRef<Path>) -> String {
        self.models_dir.join(rel).to_string_lossy().into_owned()
    }

    fn build_recognizer(&self) -> anyhow::Result<OfflineRecognizer> {
        let asr_dir = self.models_dir.join(self.asr_model.dir());
        let p = |name: &str| asr_dir.join(name).to_string_lossy().into_owned();

        let mut config = OfflineRecognizerConfig::default();
        match self.asr_model.family() {
            ModelFamily::Transducer => {
                // NeMo Parakeet-TDT transducer. `model_type` MUST be
                // "nemo_transducer" for the TDT/RNN-T NeMo export. Greedy is the
                // default: sherpa-onnx #3267 documents modified_beam_search
                // hallucinating / returning empty ~20% of the time on
                // Parakeet-TDT. Beam is opt-in for evaluation (and hotwords).
                // Native per-token timestamps come back in the result with no flag.
                config.model_config.transducer = OfflineTransducerModelConfig {
                    encoder: Some(p("encoder.int8.onnx")),
                    decoder: Some(p("decoder.int8.onnx")),
                    joiner: Some(p("joiner.int8.onnx")),
                };
                config.model_config.model_type = Some("nemo_transducer".to_string());
                config.model_config.tokens = Some(p("tokens.txt"));
                match self.decoding {
                    Decoding::Greedy => {
                        config.decoding_method = Some("greedy_search".to_string());
                    }
                    Decoding::Beam { max_active_paths } => {
                        config.decoding_method = Some("modified_beam_search".to_string());
                        config.max_active_paths = max_active_paths;
                    }
                }
                if let Some(h) = &self.hotwords {
                    config.hotwords_file = Some(h.file.to_string_lossy().into_owned());
                    config.hotwords_score = h.score;
                }
            }
            ModelFamily::Whisper => {
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
            ModelFamily::Qwen3 => {
                // Hotwords are a comma-separated prompt, not a biasing graph.
                let hotwords = match &self.hotwords {
                    Some(h) => Some(read_hotword_phrases(&h.file)?.join(",")),
                    None => None,
                };
                config.model_config.qwen3_asr = OfflineQwen3ASRModelConfig {
                    conv_frontend: Some(p("conv_frontend.onnx")),
                    encoder: Some(p("encoder.int8.onnx")),
                    decoder: Some(p("decoder.int8.onnx")),
                    tokenizer: Some(p("tokenizer")),
                    // Room for a 25 s segment's audio tokens plus its text; the
                    // crate default (128 new tokens) truncates long utterances.
                    max_total_len: 1024,
                    max_new_tokens: 256,
                    hotwords,
                    ..Default::default()
                };
            }
            ModelFamily::Cohere => {
                config.model_config.cohere_transcribe = OfflineCohereTranscribeModelConfig {
                    encoder: Some(p("encoder.int8.onnx")),
                    decoder: Some(p("decoder.int8.onnx")),
                    // Cohere has no language detection; Folio is English.
                    language: Some("en".to_string()),
                    use_punct: true,
                    use_itn: true,
                };
                config.model_config.tokens = Some(p("tokens.txt"));
            }
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
        let levelling = profile.levelled.then(|| {
            // Kept between utterances, comfortably more than Silero looks
            // back when one starts; an open utterance is never trimmed.
            let keep = ((profile.max_speech + profile.min_silence + 10.0) * sample_rate as f32) as usize;
            std::cell::RefCell::new((
                Leveller::new(sample_rate),
                Originals { base: 0, buf: Vec::new(), keep },
            ))
        });
        Ok(Vad { inner, window, sample_rate, levelling })
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

/// One Parakeet encoder frame: a 10 ms hop, subsampled 8 times.
const TDT_FRAME_S: f32 = 0.08;

/// Group tokens into words on the word-start marker (leading space, `Ġ`, or
/// `▁`), converting chunk-relative token timestamps to absolute seconds via
/// `offset`; falls back to spreading the chunk span uniformly when timestamps
/// are missing.
fn group_tokens_to_words(
    tokens: &[String],
    timestamps: &[f32],
    durations: &[f32],
    offset: f64,
    seg_start: f64,
    seg_end: f64,
    text: &str,
) -> Vec<Word> {
    let mut words = Vec::new();
    let have_ts = !timestamps.is_empty() && timestamps.len() == tokens.len();
    let have_durations = durations.len() == tokens.len();
    if have_ts {
        // A word ends where its last token ends: that token's start plus its
        // duration (TDT models report it, at least one encoder frame), else
        // the next token's start, else the segment's end. Never the last
        // token's start, which would give a one-token word no length, and
        // never past the segment's end.
        let end_of = |last: usize| {
            let end = match (have_durations, timestamps.get(last + 1)) {
                (true, _) => offset + (timestamps[last] + durations[last].max(TDT_FRAME_S)) as f64,
                (false, Some(&next)) => offset + next as f64,
                (false, None) => seg_end,
            };
            end.min(seg_end)
        };
        let mut cur_word = String::new();
        let mut cur_start: Option<f64> = None;
        for (i, (tok, &ts)) in tokens.iter().zip(timestamps.iter()).enumerate() {
            // Word-start markers: Whisper BPE uses a leading space / `Ġ`;
            // Parakeet's SentencePiece uses `▁` (U+2581). Handle both so the
            // same grouping serves the Whisper and transducer paths.
            let is_new = tok.starts_with(' ') || tok.starts_with('Ġ') || tok.starts_with('▁');
            let clean = tok.replace('Ġ', " ").replace('▁', " ");
            if is_new && !cur_word.trim().is_empty() {
                words.push(Word {
                    word: cur_word.trim().to_string(),
                    start: round3(offset + cur_start.unwrap_or(0.0)),
                    end: round3(end_of(i - 1)),
                    speaker: None,
                    original: None,
                });
                cur_word.clear();
                cur_start = None;
            }
            if cur_start.is_none() {
                cur_start = Some(ts as f64);
            }
            cur_word.push_str(&clean);
        }
        if !cur_word.trim().is_empty() {
            words.push(Word {
                word: cur_word.trim().to_string(),
                start: round3(offset + cur_start.unwrap_or(0.0)),
                end: round3(end_of(tokens.len() - 1)),
                speaker: None,
                original: None,
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
                original: None,
            });
        }
    }
    words
}

/// Non-empty, trimmed lines of a hotwords file.
fn read_hotword_phrases(file: &Path) -> anyhow::Result<Vec<String>> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("reading hotwords {}", file.display()))?;
    Ok(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Speaker diarization (pyannote segmentation + 3D-Speaker embeddings +
/// FastClustering), bound to a folder holding only the speaker models: the
/// verified speaker-detection pack is unpacked on its own, apart from the
/// transcription models.
pub struct Diarizer {
    models_dir: PathBuf,
    num_threads: i32,
}

impl Diarizer {
    /// Bind to `models_dir`, checking up front that it holds every
    /// [`crate::speaker_model_files`] file.
    pub fn new(models_dir: impl AsRef<Path>, num_threads: i32) -> anyhow::Result<Self> {
        let models_dir = models_dir.as_ref().to_path_buf();
        for rel in crate::speaker_model_files() {
            let p = models_dir.join(&rel);
            if !p.exists() {
                return Err(EngineError::ModelNotFound(p).into());
            }
        }
        Ok(Self { models_dir, num_threads })
    }

    fn model_path(&self, rel: impl AsRef<Path>) -> String {
        self.models_dir.join(rel).to_string_lossy().into_owned()
    }

    /// Diarize 16 kHz mono `samples` into speaker spans. `num_speakers = None`
    /// auto-detects.
    pub fn diarize(
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
        let chunks: Vec<(usize, std::borrow::Cow<'_, [f32]>)> =
            if self.asr_model.decodes_pieces() {
                // Parakeet decodes every sample, in ~15 s pieces cut at the
                // quietest moment: VAD gating deleted a third of the words on
                // room-mic meetings (docs/research/room-mic-capture-2026-11.md).
                let mut edges = vec![0];
                edges.extend(energy_cuts(samples, sample_rate, PIECE_S, PIECE_SEARCH_S));
                edges.push(samples.len());
                progress(Progress::new("segmenting", 1, 1, crate::PROGRESS_VAD_END));
                edges
                    .windows(2)
                    .filter(|w| w[1] > w[0])
                    .map(|w| (w[0], std::borrow::Cow::Borrowed(&samples[w[0]..w[1]])))
                    .collect()
            } else {
                // Whisper must stay under its 30 s window, the autoregressive
                // models have short output budgets, and Cohere invents text
                // over silence: these keep the (levelled) VAD.
                self.vad_chunks(samples, sample_rate, progress)
                    .context("VAD chunking")?
                    .into_iter()
                    .map(|(s, c)| (s, std::borrow::Cow::Owned(c)))
                    .collect()
            };

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
        let diarizer = Diarizer { models_dir: self.models_dir.clone(), num_threads: self.num_threads };
        diarizer.diarize(samples, sample_rate, num_speakers)
    }
}

#[cfg(test)]
mod new_tests {
    use super::*;
    use crate::AsrModel;

    /// Parakeet's tokens for "go to bed" (AMI clip EN2002b, 3.5 s in): a
    /// one-token word must still last as long as it was spoken.
    #[test]
    fn every_word_lasts_as_long_as_its_tokens() {
        let tokens: Vec<String> = [" go", " to", " b", "ed"].map(String::from).to_vec();
        let ts = [3.52, 3.68, 3.76, 3.84];
        let times = |durations: &[f32]| {
            group_tokens_to_words(&tokens, &ts, durations, 10.0, 10.0, 15.0, "go to bed")
                .iter()
                .map(|w| (w.word.clone(), w.start, w.end))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            times(&[0.16, 0.08, 0.08, 0.16]),
            vec![
                ("go".into(), 13.52, 13.68),
                ("to".into(), 13.68, 13.76),
                ("bed".into(), 13.76, 14.0)
            ]
        );
        // No durations (Whisper): a word runs to the next word's start, and
        // the last one to the segment's end.
        assert_eq!(
            times(&[]),
            vec![
                ("go".into(), 13.52, 13.68),
                ("to".into(), 13.68, 13.76),
                ("bed".into(), 13.76, 15.0)
            ]
        );
    }

    /// TDT can give a token duration 0 (the next token comes out on the same
    /// encoder frame): the word still lasts that one frame, 80 ms.
    #[test]
    fn a_word_whose_token_has_no_duration_still_lasts_a_frame() {
        let tokens: Vec<String> = [" a", " cat"].map(String::from).to_vec();
        let words = group_tokens_to_words(&tokens, &[1.2, 1.2], &[0.0, 0.16], 10.0, 10.0, 15.0, "a cat");
        assert_eq!((words[0].start, words[0].end), (11.2, 11.28));
    }

    /// A token's duration counts encoder frames, which can run past the end
    /// of the audio: a word never ends after its segment.
    #[test]
    fn a_word_never_ends_after_its_segment() {
        let tokens: Vec<String> = [" go", " home"].map(String::from).to_vec();
        let words =
            group_tokens_to_words(&tokens, &[4.6, 4.88], &[0.24, 0.32], 10.0, 10.0, 15.0, "go home");
        assert_eq!((words[1].start, words[1].end), (14.88, 15.0));
    }

    /// sherpa-onnx doesn't cut an utterance at `max_speech`: past it, it only
    /// asks for a shorter, clearer silence. So an utterance can stay open
    /// longer than any fixed `keep`, and its start must still be there when it
    /// closes.
    #[test]
    fn an_open_utterance_keeps_its_original_samples_however_long_it_runs() {
        let mut originals = Originals { base: 0, buf: Vec::new(), keep: 16_000 };
        // 60 s of speech, one 512-sample VAD window at a time, never closing.
        for k in 0..(60 * 16_000 / 512) {
            originals.push(&[k as f32; 512]);
        }
        assert_eq!(originals.get(0, 512), Some(vec![0.0; 512]));
    }

    /// VAD params are model-aware: Whisper must stay under its 30 s window; the
    /// offline transducer path may run long and bridges pauses at least as much.
    #[test]
    fn vad_params_are_model_aware() {
        let (whisper_sil, whisper_max) = vad_params(ModelFamily::Whisper);
        assert_eq!(whisper_max, 25.0, "whisper cap must be the validated 25s");
        assert!(whisper_max < 30.0, "whisper must stay under its 30s window");

        let (par_sil, par_max) = vad_params(ModelFamily::Transducer);
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

    /// Transducer hotwords only take effect under beam search, so asking for
    /// them with greedy decoding is a configuration error, not a silent no-op.
    #[test]
    fn new_rejects_transducer_hotwords_with_greedy_decoding() {
        let tmp = std::env::temp_dir().join(format!("sherpa-hotwords-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_asr_vad_placeholders(&tmp);
        let file = tmp.join("hotwords.txt");
        std::fs::write(&file, "OKAFOR\n").unwrap();

        let mut cfg = EngineConfig::new(&tmp);
        cfg.hotwords = Some(Hotwords { file, score: 1.5 });
        let msg = match SherpaEngine::new(&cfg) {
            Ok(_) => panic!("greedy + hotwords must be rejected"),
            Err(e) => e.to_string(),
        };
        assert!(msg.contains("beam search"), "got: {msg}");

        cfg.decoding = Decoding::Beam { max_active_paths: 4 };
        assert!(SherpaEngine::new(&cfg).is_ok(), "beam + hotwords must be accepted");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
