//! How a recording is split before decoding (decoding cells D1–D3 in
//! `docs/research/voice2text-lessons-2026-11.md`).

use std::fmt;
use std::str::FromStr;

/// A segmentation strategy, written on the command line as:
/// `vad:<silence>/<max>`, `vad-agc:<silence>/<max>`,
/// `vad-pad:<silence>/<max>/<pad_ms>`, `energy:<piece>/<search>` or `whole`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segmenter {
    /// Silero VAD; only speech is decoded. Folio's file path is `vad:0.8/120`,
    /// its live path `vad:0.8/15`.
    Vad { silence: f32, max: f32 },
    /// The same VAD judging a levelled copy of the audio (the engine's
    /// `Leveller`); the decoder still gets the original.
    VadAgc { silence: f32, max: f32 },
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
            Some(("vad-agc", r)) => {
                nums(r, 2).map(|v| Segmenter::VadAgc { silence: v[0], max: v[1] })
            }
            Some(("vad-pad", r)) => nums(r, 3).map(|v| Segmenter::VadPad {
                silence: v[0],
                max: v[1],
                pad_ms: v[2] as u32,
            }),
            Some(("energy", r)) => {
                nums(r, 2).map(|v| Segmenter::Energy { piece: v[0], search: v[1] })
            }
            _ => Err(format!("unknown segmenter {s:?} (vad:, vad-agc:, vad-pad:, energy:, whole)")),
        }
    }
}

impl fmt::Display for Segmenter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Segmenter::Vad { silence, max } => write!(f, "vad:{silence}/{max}"),
            Segmenter::VadAgc { silence, max } => write!(f, "vad-agc:{silence}/{max}"),
            Segmenter::VadPad { silence, max, pad_ms } => write!(f, "vad-pad:{silence}/{max}/{pad_ms}"),
            Segmenter::Energy { piece, search } => write!(f, "energy:{piece}/{search}"),
            Segmenter::Whole => write!(f, "whole"),
        }
    }
}

pub use engine::segment::energy_cuts;

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

    #[test]
    fn padding_merges_ranges_that_meet() {
        assert_eq!(
            pad_ranges(&[(100, 200), (250, 300), (900, 950)], 30, 960),
            vec![(70, 330), (870, 960)]
        );
    }

    #[test]
    fn segmenters_round_trip_through_text() {
        for s in ["vad:0.8/120", "vad-agc:0.8/15", "vad-pad:0.8/120/250", "energy:30/5", "whole"] {
            assert_eq!(s.parse::<Segmenter>().unwrap().to_string(), s);
        }
        assert!("vad:0.8".parse::<Segmenter>().is_err());
    }
}
