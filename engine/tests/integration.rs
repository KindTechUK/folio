//! End-to-end integration test against the real models + the 5-min slice.
//!
//! `#[ignore]`d: needs `../models/slice_120_420.wav` plus the small.en and
//! diarization models. Run with:
//!
//!     cargo test -- --include-ignored --nocapture

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use engine::{load_wav_16k_mono, process, AsrModel, EngineConfig, SherpaEngine};

fn models_dir() -> PathBuf {
    // models/ is a sibling of the engine crate.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("models")
}

/// Required files for a small.en run; returns the first missing one, if any.
fn first_missing(models: &Path) -> Option<PathBuf> {
    let asr = models.join("sherpa-onnx-whisper-small.en");
    let required = [
        models.join("slice_120_420.wav"),
        asr.join("small.en-encoder.int8.onnx"),
        asr.join("small.en-decoder.int8.onnx"),
        asr.join("small.en-tokens.txt"),
        models.join("silero_vad.onnx"),
        models.join("sherpa-onnx-pyannote-segmentation-3-0/model.onnx"),
        models.join("3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx"),
    ];
    required.into_iter().find(|p| !p.exists())
}

#[test]
#[ignore = "requires downloaded models + slice wav; run with --include-ignored"]
fn end_to_end_small_en_3_speakers() {
    let models = models_dir();
    if let Some(missing) = first_missing(&models) {
        panic!("missing required file {}", missing.display());
    }

    let wav = models.join("slice_120_420.wav");
    let (samples, sr) = load_wav_16k_mono(&wav).expect("load slice wav");
    let audio_dur = samples.len() as f64 / sr as f64;

    let mut cfg = EngineConfig::new(&models);
    cfg.asr_model = AsrModel::SmallEn;
    cfg.num_threads = 4;
    cfg.diarize = true;
    cfg.num_speakers = Some(3);

    let eng = SherpaEngine::new(&cfg).expect("build SherpaEngine");

    let t0 = Instant::now();
    let transcript = process(&eng, &samples, sr, &cfg, &|_| {}).expect("process");
    let proc_s = t0.elapsed().as_secs_f64();
    let rtf = proc_s / audio_dur;

    let n_segments = transcript.segments.len();
    let n_words: usize = transcript.segments.iter().map(|s| s.words.len()).sum();
    let speakers: HashSet<String> = transcript
        .segments
        .iter()
        .flat_map(|s| s.words.iter())
        .filter_map(|w| w.speaker.clone())
        .collect();

    assert!(n_segments >= 1, "expected >= 1 segment, got {n_segments}");

    for seg in &transcript.segments {
        for w in &seg.words {
            assert!(w.speaker.is_some(), "word {:?} has no speaker", w.word);
        }
    }
    assert_eq!(speakers.len(), 3, "expected 3 speakers, got {speakers:?}");

    // Word & segment starts non-decreasing (document order).
    let mut prev_seg = f64::NEG_INFINITY;
    for seg in &transcript.segments {
        assert!(
            seg.start >= prev_seg - 1e-6,
            "segment starts must be non-decreasing: {} then {}",
            prev_seg,
            seg.start
        );
        prev_seg = seg.start;
        let mut prev_w = f64::NEG_INFINITY;
        for w in &seg.words {
            assert!(
                w.start >= prev_w - 1e-6,
                "word starts must be non-decreasing within a segment: {} then {}",
                prev_w,
                w.start
            );
            prev_w = w.start;
        }
    }

    let mut detected: Vec<&String> = speakers.iter().collect();
    detected.sort();
    eprintln!("=== engine integration (small.en, num_speakers=3) ===");
    eprintln!("audio_dur_s : {audio_dur:.2}");
    eprintln!("proc_s      : {proc_s:.2}");
    eprintln!("RTF         : {rtf:.3}");
    eprintln!("n_segments  : {n_segments}");
    eprintln!("n_words     : {n_words}");
    eprintln!("speakers    : {detected:?}");

    // Sanity perf gate (not a benchmark): faster than real time.
    assert!(rtf < 1.0, "RTF must be < 1.0, got {rtf:.3}");
}
