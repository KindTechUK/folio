//! Regression: Parakeet v2 can drop the opening of a ~30 s piece. On a room
//! recording, decoding 0–30.5 s returned only the last 2 s of speech, while
//! 0–20 s and 0–25 s came back whole, at almost any gain. Imports and a
//! recording's saved transcript decode energy-cut pieces, so they must stay
//! short enough to avoid it.
//!
//! The input is a recording made through Folio on a MacBook Pro: AMI ES2004a
//! (120–190 s, room microphone) played through the laptop's speakers and
//! picked up by its built-in microphone, paused for about 10 s in the middle.
//! It's kept in the bench pack, not the repo, as
//! `audio/regressions/es2004a-room-playback.wav`.
//!
//! `#[ignore]`d: needs the Parakeet v2 model and the bench pack:
//!
//!     FOLIO_MODELS_DIR=<dir> FOLIO_BENCH_PACK=<pack> \
//!         cargo test --release --test long_pieces -- --include-ignored

use std::path::PathBuf;

use engine::{load_wav_16k_mono, EngineConfig, SherpaEngine, TranscriptionEngine};

#[test]
#[ignore = "requires the Parakeet v2 model and the bench pack; see module docs"]
fn the_opening_of_a_room_recording_is_kept() {
    let env = |k| PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("set {k}")));
    let wav = env("FOLIO_BENCH_PACK").join("audio/regressions/es2004a-room-playback.wav");
    let (samples, sr) = load_wav_16k_mono(wav).unwrap();
    let engine = SherpaEngine::new(&EngineConfig::new(env("FOLIO_MODELS_DIR"))).unwrap();

    let segments = engine.transcribe(&samples, sr, &|_| {}).unwrap();

    let text = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    // Spoken 3.5–16.5 s into the recording.
    assert!(
        text.contains("want it to be original") && text.contains("hunk of metal"),
        "the opening is missing: {text:?}"
    );
}
