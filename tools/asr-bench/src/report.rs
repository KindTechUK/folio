//! `asr-bench score` and `asr-bench report`: leaderboard-scored WER with
//! intervals, speed and memory, as markdown.

use std::collections::HashMap;
use std::fmt::Write as _;

use anyhow::Result;

use crate::align::Edits;
use crate::pack::Pack;
use crate::run::Results;
use crate::stats::{paired_interval, wer, wer_interval, Unit};
use crate::score_pair;

const REPS: usize = 4000;
const SEED: u64 = 20261122;
/// Below this many units an interval says little; long-form sets have 3–6 recordings.
const MIN_UNITS_FOR_INTERVAL: usize = 10;

/// Scores of one results file.
pub struct Scored {
    pub tag: String,
    pub set: String,
    pub model: String,
    pub segmenter: String,
    pub decoding: String,
    /// Per clip id: edits and reference words.
    pub units: HashMap<String, (Edits, usize)>,
    pub order: Vec<String>,
    pub audio_s: f64,
    pub decode_s: f64,
    pub empty: usize,
    pub segments: usize,
    pub timed: usize,
    pub load_s: Option<f64>,
    pub peak_mb: Option<f64>,
    pub machine: String,
}

pub fn score(results: &Results, pack: &Pack) -> Scored {
    let setup = results.setup.as_ref().unwrap();
    let refs: HashMap<&str, &str> =
        pack.clips.iter().map(|c| (c.id.as_str(), c.reference.as_str())).collect();
    let mut s = Scored {
        tag: results.tag.clone(),
        set: setup.set.clone(),
        model: setup.model.clone(),
        segmenter: setup.segmenter.clone(),
        decoding: setup.decoding.clone()
            + &setup.hotwords_score.map(|h| format!(" +hotwords {h}")).unwrap_or_default(),
        units: HashMap::new(),
        order: Vec::new(),
        audio_s: 0.0,
        decode_s: 0.0,
        empty: 0,
        segments: 0,
        timed: 0,
        load_s: results.load_s,
        peak_mb: results.peak_mb,
        machine: results
            .machine
            .as_ref()
            .map(|m| format!("{}, {} GB, {}", m.cpu, m.ram_gb, m.os))
            .unwrap_or_default(),
    };
    for c in &results.clips {
        let Some(reference) = refs.get(c.id.as_str()) else { continue };
        if s.units.contains_key(&c.id) {
            continue; // a clip redone after a crash: keep the first
        }
        let (e, n) = score_pair(reference, &c.hyp);
        if c.hyp.trim().is_empty() && n > 0 {
            s.empty += 1;
        }
        s.units.insert(c.id.clone(), (e, n));
        s.order.push(c.id.clone());
        s.audio_s += c.audio_s;
        s.decode_s += c.decode_s;
        s.segments += c.segments;
        s.timed += c.timed_segments;
    }
    s
}

fn units(s: &Scored, ids: &[String]) -> Vec<Unit> {
    ids.iter()
        .map(|id| {
            let (e, n) = s.units[id];
            Unit { errors: e.total() as f64, words: n as f64 }
        })
        .collect()
}

fn pct(x: u32, n: usize) -> f64 {
    if n == 0 {
        0.0
    } else {
        100.0 * x as f64 / n as f64
    }
}

/// Markdown tables, one per set; `baseline` is a tag to compare against.
pub fn report(scored: &[Scored], baseline: Option<&str>) -> Result<String> {
    let mut md = String::new();
    let mut sets: Vec<&str> = scored.iter().map(|s| s.set.as_str()).collect();
    sets.sort();
    sets.dedup();
    for set in sets {
        let rows: Vec<&Scored> = scored.iter().filter(|s| s.set == set).collect();
        let base = baseline.and_then(|b| rows.iter().find(|s| s.tag == b).copied());
        writeln!(md, "## {set}\n")?;
        writeln!(
            md,
            "| run | model | segmenter | decoding | n | WER % [95%] | Δ vs baseline [95%] | sub / del / ins % | empty | RTF | peak MB | load s | timed |"
        )?;
        writeln!(md, "|---|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|")?;
        for s in &rows {
            let ids = &s.order;
            let u = units(s, ids);
            let w = wer(&u);
            let ci = if u.len() >= MIN_UNITS_FOR_INTERVAL {
                let (lo, hi) = wer_interval(&u, REPS, SEED);
                format!("{w:.2} [{lo:.2}–{hi:.2}]")
            } else {
                format!("{w:.2} (n<{MIN_UNITS_FOR_INTERVAL})")
            };
            let delta = match base {
                Some(b) if b.tag == s.tag => "baseline".to_string(),
                Some(b) => {
                    let shared: Vec<String> =
                        ids.iter().filter(|id| b.units.contains_key(*id)).cloned().collect();
                    let su = units(s, &shared);
                    let bu = units(b, &shared);
                    let d = wer(&su) - wer(&bu);
                    if shared.len() >= MIN_UNITS_FOR_INTERVAL {
                        let (lo, hi) = paired_interval(&su, &bu, REPS, SEED);
                        format!("{d:+.2} [{lo:+.2}–{hi:+.2}]")
                    } else {
                        format!("{d:+.2}")
                    }
                }
                None => "–".to_string(),
            };
            let n_words: usize = ids.iter().map(|id| s.units[id].1).sum();
            let (sub, del, ins) = ids.iter().fold((0, 0, 0), |a, id| {
                let e = s.units[id].0;
                (a.0 + e.sub, a.1 + e.del, a.2 + e.ins)
            });
            let rtf = if s.audio_s > 0.0 { s.decode_s / s.audio_s } else { 0.0 };
            let timed = if s.segments == 0 {
                "–".to_string()
            } else if s.timed == s.segments {
                "yes".to_string()
            } else if s.timed == 0 {
                "no".to_string()
            } else {
                format!("{}%", 100 * s.timed / s.segments)
            };
            writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} | {} | {:.1} / {:.1} / {:.1} | {} | {:.3} | {} | {} | {} |",
                s.tag,
                s.model,
                s.segmenter,
                s.decoding,
                ids.len(),
                ci,
                delta,
                pct(sub, n_words),
                pct(del, n_words),
                pct(ins, n_words),
                s.empty,
                rtf,
                s.peak_mb.map(|m| format!("{m:.0}")).unwrap_or("–".into()),
                s.load_s.map(|l| format!("{l:.1}")).unwrap_or("–".into()),
                timed,
            )?;
        }
        writeln!(md)?;
        let mut machines: Vec<&str> = rows.iter().map(|s| s.machine.as_str()).collect();
        machines.dedup();
        writeln!(md, "Machine: {}.\n", machines.join("; "))?;
    }
    writeln!(
        md,
        "WER is scored as the Open ASR Leaderboard scores it (its English normaliser, compound-merging \
         alignment). Intervals: {REPS} bootstrap resamples of clips, seed {SEED}; Δ resamples the same \
         clips for both runs. `empty`: clips with a reference but no text. RTF: segmentation + decoding \
         time over audio time. `timed`: segments that came back with native word timestamps."
    )?;
    Ok(md)
}
