//! `asr-bench run`: decode one set of a pack with one model and setup.

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use engine::{AsrModel, Decoding, EngineConfig, Hotwords, SherpaEngine, VadProfile};
use serde::{Deserialize, Serialize};

use crate::machine::{machine, peak_mb, Machine};
use crate::pack::Pack;
use crate::segment::{energy_cuts, pad_ranges, Segmenter};

/// Everything that defines a run; a resumed run must match it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Setup {
    pub set: String,
    pub model: String,
    pub segmenter: String,
    pub decoding: String,
    pub hotwords: Option<String>,
    pub hotwords_score: Option<f32>,
    pub threads: i32,
}

/// One line of a results file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Row {
    Run { tag: String, setup: Setup, machine: Machine },
    Clip(ClipResult),
    Summary { load_s: f64, peak_mb: Option<f64>, wall_s: f64, clips: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipResult {
    pub id: String,
    pub audio_s: f64,
    /// Segmentation + decoding, as the app pays it.
    pub decode_s: f64,
    pub segments: usize,
    /// Segments that came back with native word timestamps.
    pub timed_segments: usize,
    pub hyp: String,
}

pub fn parse_decoding(s: &str) -> Result<Decoding> {
    match s.split_once(':') {
        None if s == "greedy" => Ok(Decoding::Greedy),
        Some(("beam", n)) => Ok(Decoding::Beam { max_active_paths: n.parse()? }),
        _ => bail!("decoding {s:?}: use greedy or beam:<paths>"),
    }
}

pub struct RunArgs {
    pub pack: PathBuf,
    pub set: String,
    pub model: AsrModel,
    pub models_dir: PathBuf,
    pub segmenter: Segmenter,
    pub decoding: String,
    pub hotwords: Option<PathBuf>,
    pub hotwords_score: f32,
    pub threads: i32,
    pub tag: Option<String>,
    pub out: PathBuf,
    pub limit: Option<usize>,
}

pub fn default_tag(a: &RunArgs) -> String {
    let mut t = format!("{}_{}_{}_{}", a.set, a.model.stem(), a.segmenter, a.decoding);
    if a.hotwords.is_some() {
        t.push_str(&format!("_hw{}", a.hotwords_score));
    }
    t.replace([':', '/'], "-")
}

pub fn run(a: RunArgs) -> Result<PathBuf> {
    let pack = Pack::load(&a.pack)?;
    let mut clips = pack.set(&a.set);
    if clips.is_empty() {
        bail!("set {:?} not in pack (has {:?})", a.set, pack.sets());
    }
    if let Some(n) = a.limit {
        clips.truncate(n);
    }
    let decoding = parse_decoding(&a.decoding)?;
    let tag = a.tag.clone().unwrap_or_else(|| default_tag(&a));
    let setup = Setup {
        set: a.set.clone(),
        model: a.model.stem().to_string(),
        segmenter: a.segmenter.to_string(),
        decoding: a.decoding.clone(),
        hotwords: a.hotwords.as_ref().map(|p| p.display().to_string()),
        hotwords_score: a.hotwords.as_ref().map(|_| a.hotwords_score),
        threads: a.threads,
    };

    std::fs::create_dir_all(&a.out)?;
    let path = a.out.join(format!("{tag}.jsonl"));
    let done = resume(&path, &setup)?;
    let mut out = OpenOptions::new().create(true).append(true).open(&path)?;
    if done.is_none() {
        write_row(&mut out, &Row::Run { tag: tag.clone(), setup: setup.clone(), machine: machine() })?;
    }
    let done = done.unwrap_or_default();

    let wall = Instant::now();
    let mut cfg = EngineConfig::new(&a.models_dir);
    cfg.asr_model = a.model;
    cfg.num_threads = a.threads;
    cfg.decoding = decoding;
    cfg.hotwords = a.hotwords.clone().map(|file| Hotwords { file, score: a.hotwords_score });
    let engine = SherpaEngine::new(&cfg)?;
    let t = Instant::now();
    let recognizer = engine.recognizer()?;
    let load_s = t.elapsed().as_secs_f64();
    eprintln!("{tag}: model loaded in {load_s:.1} s");

    let todo: Vec<_> = clips.iter().filter(|c| !done.contains(&c.id)).collect();
    for (k, clip) in todo.iter().enumerate() {
        let (samples, sr) = engine::load_wav_16k_mono(a.pack.join(&clip.audio))
            .with_context(|| format!("clip {}", clip.id))?;
        let t = Instant::now();
        let ranges = segment(&engine, &samples, sr, a.segmenter)?;
        let mut texts = Vec::new();
        let mut timed = 0;
        for &(s, e) in &ranges {
            if let Some((seg, native)) = recognizer.decode_utterance_timed(s, &samples[s..e]) {
                texts.push(seg.text);
                timed += usize::from(native);
            }
        }
        let decode_s = t.elapsed().as_secs_f64();
        let audio_s = samples.len() as f64 / sr as f64;
        write_row(
            &mut out,
            &Row::Clip(ClipResult {
                id: clip.id.clone(),
                audio_s,
                decode_s,
                segments: ranges.len(),
                timed_segments: timed,
                hyp: texts.join(" "),
            }),
        )?;
        if (k + 1) % 10 == 0 || k + 1 == todo.len() {
            eprintln!("{tag}: {}/{} clips, last RTF {:.3}", k + 1, todo.len(), decode_s / audio_s);
        }
    }
    write_row(
        &mut out,
        &Row::Summary {
            load_s,
            peak_mb: peak_mb(),
            wall_s: wall.elapsed().as_secs_f64(),
            clips: todo.len(),
        },
    )?;
    Ok(path)
}

/// Sample ranges to decode.
fn segment(
    engine: &SherpaEngine,
    samples: &[f32],
    sr: u32,
    seg: Segmenter,
) -> Result<Vec<(usize, usize)>> {
    let vad_ranges = |silence: f32, max: f32, levelled: bool| -> Result<Vec<(usize, usize)>> {
        let vad = engine.vad(VadProfile { min_silence: silence, max_speech: max, levelled })?;
        let mut chunks = vad.accept(samples);
        chunks.extend(vad.flush());
        Ok(chunks.into_iter().map(|(s, c)| (s, (s + c.len()).min(samples.len()))).collect())
    };
    Ok(match seg {
        Segmenter::Vad { silence, max } => vad_ranges(silence, max, false)?,
        Segmenter::VadAgc { silence, max } => vad_ranges(silence, max, true)?,
        Segmenter::VadPad { silence, max, pad_ms } => {
            let pad = (pad_ms as usize * sr as usize) / 1000;
            pad_ranges(&vad_ranges(silence, max, false)?, pad, samples.len())
        }
        Segmenter::Energy { piece, search } => {
            let mut edges = vec![0];
            edges.extend(energy_cuts(samples, sr, piece, search));
            edges.push(samples.len());
            edges.windows(2).filter(|w| w[1] > w[0]).map(|w| (w[0], w[1])).collect()
        }
        Segmenter::Whole => vec![(0, samples.len())],
    })
}

/// Clip ids already in an existing results file (`None` if there is none).
fn resume(path: &Path, setup: &Setup) -> Result<Option<HashSet<String>>> {
    if !path.exists() {
        return Ok(None);
    }
    let mut done = HashSet::new();
    for (i, line) in BufReader::new(File::open(path)?).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Row>(&line) {
            Ok(Row::Run { setup: s, .. }) if i == 0 => {
                if &s != setup {
                    bail!("{} was made with {s:?}; pass another --tag", path.display());
                }
            }
            Ok(Row::Clip(c)) => {
                done.insert(c.id);
            }
            Ok(_) => {}
            // A line cut short by a crash: ignore it, the clip is redone.
            Err(_) => {}
        }
    }
    eprintln!("{}: resuming, {} clips already done", path.display(), done.len());
    Ok(Some(done))
}

fn write_row(out: &mut File, row: &Row) -> Result<()> {
    writeln!(out, "{}", serde_json::to_string(row)?)?;
    out.flush()?;
    Ok(())
}

/// Read a results file back.
pub fn read_results(path: &Path) -> Result<Results> {
    let mut r = Results { path: path.to_path_buf(), ..Default::default() };
    for line in BufReader::new(File::open(path)?).lines() {
        let line = line?;
        match serde_json::from_str::<Row>(&line) {
            Ok(Row::Run { tag, setup, machine }) => {
                r.tag = tag;
                r.setup = Some(setup);
                r.machine = Some(machine);
            }
            Ok(Row::Clip(c)) => r.clips.push(c),
            Ok(Row::Summary { load_s, peak_mb, .. }) => {
                r.load_s = Some(load_s);
                r.peak_mb = match (r.peak_mb, peak_mb) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
            }
            Err(_) => {}
        }
    }
    if r.setup.is_none() {
        bail!("{}: no run header", path.display());
    }
    Ok(r)
}

#[derive(Debug, Default)]
pub struct Results {
    pub path: PathBuf,
    pub tag: String,
    pub setup: Option<Setup>,
    pub machine: Option<Machine>,
    pub clips: Vec<ClipResult>,
    pub load_s: Option<f64>,
    pub peak_mb: Option<f64>,
}
