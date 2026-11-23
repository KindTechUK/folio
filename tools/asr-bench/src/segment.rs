//! How a recording is split before decoding (decoding cells D1–D3 in
//! `docs/research/voice2text-lessons-2026-11.md`).

use std::fmt;
use std::str::FromStr;

/// A segmentation strategy, written on the command line as:
/// `vad:<silence>/<max>`, `vad-pad:<silence>/<max>/<pad_ms>`,
/// `energy:<piece>/<search>` or `whole`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segmenter {
    /// Silero VAD; only speech is decoded. Folio's file path is `vad:0.8/120`,
    /// its live path `vad:0.8/15`.
    Vad { silence: f32, max: f32 },
    /// VAD segments widened by `pad_ms` on each side (overlaps merged).
    VadPad { silence: f32, max: f32, pad_ms: u32 },
    /// voice2text's piece rule: every ~`piece` s, cut at the quietest 100 ms
    /// within `search` s of the mark. Every sample is decoded.
    Energy { piece: f32, search: f32 },
    /// The whole clip in one decode (short clips only).
    Whole,
}

impl FromStr for Segmenter {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let nums = |rest: &str, n: usize| -> Result<Vec<f32>, String> {
            let v: Result<Vec<f32>, _> = rest.split('/').map(str::parse).collect();
            match v {
                Ok(v) if v.len() == n => Ok(v),
                _ => Err(format!("{s}: expected {n} numbers separated by '/'")),
            }
        };
        match s.split_once(':') {
            None if s == "whole" => Ok(Segmenter::Whole),
            Some(("vad", r)) => nums(r, 2).map(|v| Segmenter::Vad { silence: v[0], max: v[1] }),
            Some(("vad-pad", r)) => nums(r, 3).map(|v| Segmenter::VadPad {
                silence: v[0],
                max: v[1],
                pad_ms: v[2] as u32,
            }),
            Some(("energy", r)) => {
                nums(r, 2).map(|v| Segmenter::Energy { piece: v[0], search: v[1] })
            }
            _ => Err(format!("unknown segmenter {s:?} (vad:, vad-pad:, energy:, whole)")),
        }
    }
}

impl fmt::Display for Segmenter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Segmenter::Vad { silence, max } => write!(f, "vad:{silence}/{max}"),
            Segmenter::VadPad { silence, max, pad_ms } => write!(f, "vad-pad:{silence}/{max}/{pad_ms}"),
            Segmenter::Energy { piece, search } => write!(f, "energy:{piece}/{search}"),
            Segmenter::Whole => write!(f, "whole"),
        }
    }
}

/// Cut points (sample indices, ascending, excluding 0 and the end) for the
/// energy rule. Mirrors voice2text's `PieceDecoder.step`: while at least
/// `piece + search` seconds remain after the last cut, look in
/// `[mark - search, mark + search)` for the 100 ms window with the least mean
/// energy and cut at its middle.
pub fn energy_cuts(samples: &[f32], sample_rate: u32, piece: f32, search: f32) -> Vec<usize> {
    let sr = sample_rate as f32;
    let piece = (piece * sr) as usize;
    let search = (search * sr) as usize;
    let window = ((0.1 * sr) as usize).max(1);
    let mut cuts = Vec::new();
    let mut start = 0usize;
    while samples.len() - start >= piece + search && piece > search {
        let lo = start + piece - search;
        let region = &samples[lo..(lo + 2 * search).min(samples.len())];
        if region.len() < window {
            break;
        }
        // Mean energy of each `window`-long run, via prefix sums.
        let mut prefix = Vec::with_capacity(region.len() + 1);
        prefix.push(0f64);
        for &x in region {
            prefix.push(prefix.last().unwrap() + (x as f64) * (x as f64));
        }
        let mut best = (f64::INFINITY, 0usize);
        for k in 0..=region.len() - window {
            let e = prefix[k + window] - prefix[k];
            if e < best.0 {
                best = (e, k);
            }
        }
        let cut = lo + best.1 + window / 2;
        cuts.push(cut);
        start = cut;
    }
    cuts
}

/// Widen `[start, end)` ranges by `pad` samples within `[0, len)`, merging any
/// that then touch.
pub fn pad_ranges(ranges: &[(usize, usize)], pad: usize, len: usize) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &(s, e) in ranges {
        let (s, e) = (s.saturating_sub(pad), (e + pad).min(len));
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Loud noise everywhere except one silent 200 ms gap: the cut must land in it.
    #[test]
    fn energy_cut_lands_in_the_quiet_gap() {
        let sr = 16_000u32;
        let mut x: Vec<f32> = (0..sr as usize * 40)
            .map(|i| if (i * 7919) % 13 < 6 { 0.5 } else { -0.5 })
            .collect();
        let gap = (sr as usize * 31)..(sr as usize * 31 + 3200); // 31.0–31.2 s
        for s in &mut x[gap.clone()] {
            *s = 0.0;
        }
        let cuts = energy_cuts(&x, sr, 30.0, 5.0);
        assert_eq!(cuts.len(), 1, "40 s with 30 s pieces: one cut, got {cuts:?}");
        assert!(gap.contains(&cuts[0]), "cut {} outside the gap {gap:?}", cuts[0]);
    }

    #[test]
    fn audio_shorter_than_a_piece_is_not_cut() {
        assert!(energy_cuts(&vec![0.1; 16_000 * 20], 16_000, 30.0, 5.0).is_empty());
    }

    #[test]
    fn padding_merges_ranges_that_meet() {
        assert_eq!(
            pad_ranges(&[(100, 200), (250, 300), (900, 950)], 30, 960),
            vec![(70, 330), (870, 960)]
        );
    }

    #[test]
    fn segmenters_round_trip_through_text() {
        for s in ["vad:0.8/120", "vad-pad:0.8/120/250", "energy:30/5", "whole"] {
            assert_eq!(s.parse::<Segmenter>().unwrap().to_string(), s);
        }
        assert!("vad:0.8".parse::<Segmenter>().is_err());
    }
}
