//! Per-word speaker assignment + segment-speaker majority.
//!
//! Mirrors `core/src/diarization.ts` `assignSpeakers` plus a duration-weighted
//! segment majority; the TS `core` is the oracle this must conform to:
//!
//! - each word gets the speaker of the diarized span it overlaps most;
//! - no overlap -> nearest span by midpoint distance to either span edge;
//! - no spans at all -> `UNKNOWN`;
//! - segment speaker = duration-weighted majority of its words' speakers.

use crate::{AsrSegment, Segment, SpeakerSpan, Transcript, Word, UNKNOWN_SPEAKER};
use std::collections::HashMap;

/// Speaker for a word spanning `[ws, we]` given diarized `spans`.
fn speaker_for(ws: f64, we: f64, spans: &[SpeakerSpan]) -> String {
    let mut best: Option<&str> = None;
    let mut best_ov = 0.0_f64;
    for sp in spans {
        let ov = we.min(sp.end) - ws.max(sp.start);
        if ov > best_ov {
            best_ov = ov;
            best = Some(&sp.speaker);
        }
    }
    if let Some(s) = best {
        return s.to_string();
    }
    // No overlap: nearest span by distance from the word midpoint to either edge.
    let mid = (ws + we) / 2.0;
    spans
        .iter()
        .min_by(|a, b| {
            let da = (mid - a.start).abs().min((mid - a.end).abs());
            let db = (mid - b.start).abs().min((mid - b.end).abs());
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|sp| sp.speaker.clone())
        .unwrap_or_else(|| UNKNOWN_SPEAKER.to_string())
}

/// Assign speakers to every word, then to every segment by duration-weighted
/// majority, producing the contract [`Transcript`].
pub fn merge(asr: &[AsrSegment], spans: &[SpeakerSpan]) -> Transcript {
    let mut out_segs = Vec::with_capacity(asr.len());
    for seg in asr {
        let mut words: Vec<Word> = Vec::with_capacity(seg.words.len());
        for w in &seg.words {
            words.push(Word {
                word: w.word.clone(),
                start: w.start,
                end: w.end,
                speaker: Some(speaker_for(w.start, w.end, spans)),
                original: w.original.clone(),
            });
        }
        let mut weight: HashMap<String, f64> = HashMap::new();
        for w in &words {
            let spk = w.speaker.clone().unwrap_or_else(|| UNKNOWN_SPEAKER.to_string());
            *weight.entry(spk).or_insert(0.0) += (w.end - w.start).max(1e-3);
        }
        // Highest weight wins; ties go to the smallest label, for determinism.
        let seg_spk = weight
            .into_iter()
            .max_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.0.cmp(&a.0))
            })
            .map(|(s, _)| s)
            .unwrap_or_else(|| UNKNOWN_SPEAKER.to_string());

        out_segs.push(Segment {
            start: seg.start,
            end: seg.end,
            text: seg.text.clone(),
            speaker: Some(seg_spk),
            words,
        });
    }
    Transcript { segments: out_segs }
}

/// Build a speaker-less [`Transcript`] from ASR segments alone (no diarization).
///
/// Used by the default "paragraph QA" path (`EngineConfig::diarize == false`):
/// every segment and word carries `speaker: None`.
pub fn from_asr(asr: &[AsrSegment]) -> Transcript {
    let segments = asr
        .iter()
        .map(|seg| Segment {
            start: seg.start,
            end: seg.end,
            text: seg.text.clone(),
            speaker: None,
            words: seg
                .words
                .iter()
                .map(|w| Word {
                    word: w.word.clone(),
                    start: w.start,
                    end: w.end,
                    speaker: None,
                    original: w.original.clone(),
                })
                .collect(),
        })
        .collect();
    Transcript { segments }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(spk: &str, s: f64, e: f64) -> SpeakerSpan {
        SpeakerSpan { speaker: spk.into(), start: s, end: e }
    }
    fn word(w: &str, s: f64, e: f64) -> Word {
        Word { word: w.into(), start: s, end: e, speaker: None, original: None }
    }

    /// Speaker `merge` assigns to a single word spanning `[ws, we]`.
    fn merged_word_speaker(ws: f64, we: f64, spans: &[SpeakerSpan]) -> Option<String> {
        let asr = vec![AsrSegment {
            start: ws,
            end: we,
            text: "x".into(),
            words: vec![word("x", ws, we)],
        }];
        merge(&asr, spans).segments[0].words[0].speaker.clone()
    }

    #[test]
    fn max_overlap_wins() {
        let spans = vec![span("SPEAKER_00", 0.0, 1.0), span("SPEAKER_01", 1.0, 2.0)];
        // word 0.9..1.6 overlaps S0 by 0.1, S1 by 0.6 -> S1
        assert_eq!(merged_word_speaker(0.9, 1.6, &spans).as_deref(), Some("SPEAKER_01"));
    }

    #[test]
    fn nearest_by_midpoint_when_no_overlap() {
        let spans = vec![span("SPEAKER_00", 0.0, 1.0), span("SPEAKER_01", 5.0, 6.0)];
        // word 2.0..2.2 (mid 2.1) is nearer S0's end (1.0) than S1's start (5.0)
        assert_eq!(merged_word_speaker(2.0, 2.2, &spans).as_deref(), Some("SPEAKER_00"));
    }

    #[test]
    fn unknown_when_no_spans() {
        assert_eq!(merged_word_speaker(0.0, 1.0, &[]).as_deref(), Some(UNKNOWN_SPEAKER));
    }

    #[test]
    fn segment_speaker_is_duration_weighted_majority() {
        let spans = vec![span("SPEAKER_00", 0.0, 0.5), span("SPEAKER_01", 0.5, 5.0)];
        let asr = vec![AsrSegment {
            start: 0.0,
            end: 5.0,
            text: "a b".into(),
            // short S0 word + long S1 word -> segment should be S1
            words: vec![word("a", 0.0, 0.4), word("b", 1.0, 4.0)],
        }];
        let t = merge(&asr, &spans);
        assert_eq!(t.segments[0].speaker.as_deref(), Some("SPEAKER_01"));
        assert_eq!(t.segments[0].words[0].speaker.as_deref(), Some("SPEAKER_00"));
        assert_eq!(t.segments[0].words[1].speaker.as_deref(), Some("SPEAKER_01"));
    }
}
