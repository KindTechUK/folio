//! Live (utterance-by-utterance) transcription against the real Parakeet model.
//!
//! `#[ignore]`d: needs the Parakeet + Silero VAD models and a 16 kHz mono WAV:
//!
//!     FOLIO_MODELS_DIR=<dir> FOLIO_LIVE_WAV=<wav> \
//!         cargo test --release --test live -- --include-ignored

use std::path::PathBuf;

use engine::{
    load_wav_16k_mono, AsrModel, EngineConfig, SherpaEngine, TranscriptionEngine, VadProfile,
    Word,
};

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("set {name}")))
}

/// Pairs of (live word, file word) with the same text, in order: the most
/// matches possible (longest common subsequence), and among those the pairing
/// closest in time, so a repeated word ("we ... we") pairs with the same
/// spoken instance.
fn aligned<'a>(a: &'a [Word], b: &'a [Word]) -> Vec<(&'a Word, &'a Word)> {
    let key = |w: &Word| w.word.to_lowercase();
    let (n, m) = (a.len(), b.len());
    // (matches, -total time gap) - larger is better.
    let mut dp = vec![vec![(0u32, 0.0f64); m + 1]; n + 1];
    let better = |x: (u32, f64), y: (u32, f64)| x.0 > y.0 || (x.0 == y.0 && x.1 > y.1);
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let mut best = if better(dp[i + 1][j], dp[i][j + 1]) { dp[i + 1][j] } else { dp[i][j + 1] };
            if key(&a[i]) == key(&b[j]) {
                let next = dp[i + 1][j + 1];
                let cand = (next.0 + 1, next.1 - (a[i].start - b[j].start).abs());
                if better(cand, best) {
                    best = cand;
                }
            }
            dp[i][j] = best;
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        let here = dp[i][j];
        if key(&a[i]) == key(&b[j]) {
            let next = dp[i + 1][j + 1];
            if here == (next.0 + 1, next.1 - (a[i].start - b[j].start).abs()) {
                out.push((&a[i], &b[j]));
                i += 1;
                j += 1;
                continue;
            }
        }
        if dp[i + 1][j] == here {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

#[test]
#[ignore = "requires Parakeet + VAD models and a WAV; see module docs"]
fn live_words_have_absolute_times_matching_the_file_path() {
    let (samples, sr) = load_wav_16k_mono(env_path("FOLIO_LIVE_WAV")).unwrap();
    let duration = samples.len() as f64 / sr as f64;
    let mut cfg = EngineConfig::new(env_path("FOLIO_MODELS_DIR"));
    cfg.asr_model = AsrModel::ParakeetV2;
    let eng = SherpaEngine::new(&cfg).unwrap();

    // Live: uneven chunks, as a microphone callback would deliver them.
    let vad = eng.vad(VadProfile::LIVE).unwrap();
    let rec = eng.recognizer().unwrap();
    let mut live: Vec<Word> = Vec::new();
    let sizes = [160usize, 1024, 4800, 333];
    let (mut pos, mut k) = (0, 0);
    let mut utterances = Vec::new();
    while pos < samples.len() {
        let end = (pos + sizes[k % sizes.len()]).min(samples.len());
        utterances.extend(vad.accept(&samples[pos..end]));
        pos = end;
        k += 1;
    }
    utterances.extend(vad.flush());
    for (start, chunk) in &utterances {
        if let Some(seg) = rec.decode_utterance(*start, chunk) {
            live.extend(seg.words);
        }
    }

    let file: Vec<Word> = eng
        .transcribe(&samples, sr, &|_| {})
        .unwrap()
        .into_iter()
        .flat_map(|s| s.words)
        .collect();

    assert!(!live.is_empty());
    for w in live.windows(2) {
        assert!(w[1].start >= w[0].start, "times go backwards: {w:?}");
    }
    assert!(live.iter().all(|w| w.start >= 0.0 && w.end <= duration + 0.01));

    let pairs = aligned(&live, &file);
    assert!(pairs.len() * 10 >= live.len() * 8, "too few shared words to compare");
    for (l, f) in pairs {
        assert!(
            (l.start - f.start).abs() <= 0.5,
            "{:?} at {} live vs {} file",
            l.word,
            l.start,
            f.start
        );
    }
}
