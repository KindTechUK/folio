//! A levelled VAD finds quiet, distant speech that the plain one misses, and
//! still hands the decoder the original audio, not the levelled copy
//! (`docs/research/room-mic-capture-2026-11.md`, recommendation 2).
//!
//! On clean close-talk audio Silero hears speech at any level (a test
//! recording at −75 dBFS is still found), so the input is a real room-mic clip
//! from the bench pack: AMI EN2002a, table microphone, where the plain live VAD
//! finds nothing.
//!
//! `#[ignore]`d: needs `silero_vad.onnx` and the bench pack:
//!
//!     FOLIO_MODELS_DIR=<dir> FOLIO_BENCH_PACK=<pack> \
//!         cargo test --release --test levelled_vad -- --include-ignored

use std::path::PathBuf;

use engine::{load_wav_16k_mono, EngineConfig, SherpaEngine, VadProfile};

const CLIP: &str = "audio/ami-sdm/AMI_EN2002a_sdm_MEE073_0031139_0031547.wav";

#[test]
#[ignore = "requires Silero VAD and the bench pack; see module docs"]
fn distant_speech_is_found_and_the_original_audio_is_kept() {
    let env = |k| PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("set {k}")));
    let (samples, _) = load_wav_16k_mono(env("FOLIO_BENCH_PACK").join(CLIP)).unwrap();
    let engine = SherpaEngine::new(&EngineConfig::new(env("FOLIO_MODELS_DIR"))).unwrap();

    let utterances = |levelled: bool| {
        let vad = engine.vad(VadProfile { levelled, ..VadProfile::LIVE }).unwrap();
        let mut out = vad.accept(&samples);
        out.extend(vad.flush());
        out
    };

    assert!(utterances(false).is_empty(), "the plain VAD already hears this clip");
    let found = utterances(true);
    assert!(!found.is_empty(), "the levelled VAD found no speech");
    for (start, chunk) in &found {
        assert_eq!(chunk.as_slice(), &samples[*start..*start + chunk.len()]);
    }
}

/// The whole clip, then a minute of silence, handed over in one call (as a
/// file is): the utterance still carries the original samples.
#[test]
#[ignore = "requires Silero VAD and the bench pack; see module docs"]
fn a_long_input_in_one_call_still_returns_the_original_audio() {
    let env = |k| PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("set {k}")));
    let (mut samples, sr) = load_wav_16k_mono(env("FOLIO_BENCH_PACK").join(CLIP)).unwrap();
    samples.extend(std::iter::repeat_n(0.0, 60 * sr as usize));
    let engine = SherpaEngine::new(&EngineConfig::new(env("FOLIO_MODELS_DIR"))).unwrap();
    let vad = engine.vad(VadProfile::LIVE).unwrap();
    let mut found = vad.accept(&samples);
    found.extend(vad.flush());

    assert!(!found.is_empty(), "the levelled VAD found no speech");
    for (start, chunk) in &found {
        assert_eq!(chunk.as_slice(), &samples[*start..*start + chunk.len()]);
    }
}
