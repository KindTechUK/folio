//! Speech-to-text evaluation bench. See `README.md`.

pub mod align;
pub mod machine;
pub mod normalize;
pub mod pack;
pub mod report;
pub mod run;
pub mod segment;
pub mod stats;

/// Leaderboard-scored errors for one clip: `(edits, reference word count)`.
pub fn score_pair(reference: &str, hyp: &str) -> (align::Edits, usize) {
    let n = normalize::english();
    let r = n.words(reference);
    let h = n.words(hyp);
    (align::edit_distance(&r, &h), r.len())
}
