//! The scorer port against outputs of the Open ASR Leaderboard's Python scorer
//! and kaldialign 0.12.0 (`tests/fixtures/oasr_golden.json`, made by
//! `tests/fixtures/make_golden.py`). Expected values come from the reference
//! implementation, never from this crate.

use asr_bench::{align::edit_distance, normalize::english};
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    normalise: Vec<NormCase>,
    align: Vec<AlignCase>,
}
#[derive(Deserialize)]
struct NormCase {
    text: String,
    words: Vec<String>,
}
#[derive(Deserialize)]
struct AlignCase {
    #[serde(rename = "ref")]
    reference: String,
    hyp: String,
    ref_words: Vec<String>,
    hyp_words: Vec<String>,
    ins: u32,
    del: u32,
    sub: u32,
    total: u32,
}

fn golden() -> Golden {
    serde_json::from_str(include_str!("fixtures/oasr_golden.json")).unwrap()
}

#[test]
fn normaliser_matches_the_leaderboard() {
    let n = english();
    let mut failures = Vec::new();
    for c in golden().normalise {
        let got = n.words(&c.text);
        if got != c.words {
            failures.push(format!("{:?}\n   want {:?}\n   got  {:?}", c.text, c.words, got));
        }
    }
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn compound_alignment_matches_kaldialign() {
    for c in golden().align {
        let e = edit_distance(&c.ref_words, &c.hyp_words);
        assert_eq!(
            (e.ins, e.del, e.sub, e.total()),
            (c.ins, c.del, c.sub, c.total),
            "ref {:?} hyp {:?}",
            c.ref_words,
            c.hyp_words
        );
    }
}

#[test]
fn raw_pairs_score_as_the_leaderboard_does() {
    for c in golden().align {
        let (e, n) = asr_bench::score_pair(&c.reference, &c.hyp);
        assert_eq!((e.total(), n), (c.total, c.ref_words.len()), "{:?} / {:?}", c.reference, c.hyp);
    }
}
