//! Regression: sherpa-onnx before 2.0 returned empty text for quiet Parakeet
//! input (upstream k2-fsa/sherpa-onnx#3997, #2258). Its feature front end floored
//! log energies at FLT_EPSILON where NeMo adds 2^-24, so quiet speech fell to
//! the floor. On real recordings this dropped whole segments of clear speech
//! (docs/research/voice2text-lessons-2026-11.md, "First results").
//!
//! `#[ignore]`d: needs the Parakeet v2 model, whose release folder ships
//! `test_wavs/0.wav`:
//!
//!     FOLIO_MODELS_DIR=<dir> cargo test --release --test quiet_audio -- --include-ignored

use std::path::PathBuf;

use engine::{load_wav_16k_mono, AsrModel, EngineConfig, SherpaEngine};

#[test]
#[ignore = "requires the Parakeet v2 model; see module docs"]
fn quiet_speech_is_still_transcribed() {
    let models = PathBuf::from(std::env::var("FOLIO_MODELS_DIR").expect("set FOLIO_MODELS_DIR"));
    let model = AsrModel::ParakeetV2;
    let (samples, _) = load_wav_16k_mono(models.join(model.dir()).join("test_wavs/0.wav")).unwrap();
    let engine = SherpaEngine::new(&EngineConfig::new(&models)).unwrap();
    let recognizer = engine.recognizer().unwrap();

    let loud = recognizer.decode_utterance(0, &samples).expect("the test recording has speech");
    // The same recording 60 dB quieter: still speech, and a model with NeMo's
    // normalisation transcribes it.
    let quiet: Vec<f32> = samples.iter().map(|s| s * 0.001).collect();
    let text = recognizer.decode_utterance(0, &quiet).map(|s| s.text).unwrap_or_default();

    let words = text.split_whitespace().count();
    let expected = loud.text.split_whitespace().count();
    assert!(
        words * 2 >= expected,
        "60 dB quieter gave {words} words, against {expected} at full level: {text:?}"
    );
}
