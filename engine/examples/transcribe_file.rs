//! Scriptable CLI front-end for the engine, used by the end-to-end harness
//! (`scripts/e2e.sh`).
//!
//! Usage:
//!
//!     cargo run --release --example transcribe_file -- \
//!         <wav_16k_mono> <out.json> [--diarize] [--speakers N] [--model small.en|medium.en|parakeet] \
//!         [--threads N] [--models-dir DIR]
//!
//! It loads a 16 kHz mono WAV, runs ASR (plus speaker diarization with
//! `--diarize`; `--speakers N` implies it), writes the
//! `{ "segments": [...] }` sidecar to `<out.json>`, and prints
//! RTF / n_segments / n_words / speakers to stderr (so stdout stays clean).
//!
//! The models directory defaults to `../models` relative to the engine crate
//! root (the repo's `models/`); override with `--models-dir <dir>`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Instant;

use engine::{load_wav_16k_mono, process, AsrModel, EngineConfig, SherpaEngine};

fn usage() -> ! {
    eprintln!(
        "usage: transcribe_file <wav_16k_mono> <out.json> \
         [--diarize] [--speakers N] [--model small.en|medium.en|parakeet] [--threads N] [--models-dir DIR]"
    );
    exit(2);
}

fn default_models_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("models")
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);

    let wav = match args.next() {
        Some(a) => a,
        None => usage(),
    };
    let out = match args.next() {
        Some(a) => a,
        None => usage(),
    };

    let mut diarize = false;
    let mut num_speakers: Option<u32> = None;
    let mut asr_model = AsrModel::default();
    let mut num_threads: i32 = 4;
    let mut models_dir = default_models_dir();

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--diarize" => diarize = true,
            "--speakers" => {
                let v = args.next().unwrap_or_else(|| usage());
                num_speakers = Some(v.parse().expect("--speakers must be a positive integer"));
                diarize = true;
            }
            "--model" => {
                let v = args.next().unwrap_or_else(|| usage());
                asr_model = match v.as_str() {
                    "small.en" | "small" | "SmallEn" => AsrModel::SmallEn,
                    "medium.en" | "medium" | "MediumEn" => AsrModel::MediumEn,
                    "parakeet" | "parakeet-v2" | "ParakeetV2" => AsrModel::ParakeetV2,
                    other => {
                        eprintln!("unknown --model {other:?} (want small.en|medium.en|parakeet)");
                        exit(2);
                    }
                };
            }
            "--threads" => {
                let v = args.next().unwrap_or_else(|| usage());
                num_threads = v.parse().expect("--threads must be an integer");
            }
            "--models-dir" => {
                models_dir = PathBuf::from(args.next().unwrap_or_else(|| usage()));
            }
            other => {
                eprintln!("unknown argument {other:?}");
                usage();
            }
        }
    }

    let (samples, sr) = load_wav_16k_mono(&wav)?;
    let audio_dur = samples.len() as f64 / sr as f64;

    let mut cfg = EngineConfig::new(&models_dir);
    cfg.asr_model = asr_model;
    cfg.num_threads = num_threads;
    cfg.diarize = diarize;
    cfg.num_speakers = num_speakers;

    eprintln!("=== transcribe_file ===");
    eprintln!("wav         : {wav}");
    eprintln!("models_dir  : {}", models_dir.display());
    eprintln!("model       : {}", asr_model.stem());
    eprintln!("threads     : {num_threads}");
    eprintln!(
        "speakers    : {}",
        num_speakers
            .map(|n| n.to_string())
            .unwrap_or_else(|| "auto".to_string())
    );
    eprintln!("audio_dur_s : {audio_dur:.2}");

    let eng = SherpaEngine::new(&cfg)?;

    let t0 = Instant::now();
    let transcript = process(&eng, &samples, sr, &cfg, &|p| {
        eprintln!("progress    : {} {:.0}%", p.stage, p.fraction * 100.0);
    })?;
    let proc_s = t0.elapsed().as_secs_f64();
    let rtf = if audio_dur > 0.0 { proc_s / audio_dur } else { 0.0 };

    if let Some(parent) = Path::new(&out).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&out, serde_json::to_string_pretty(&transcript)?)?;

    let n_segments = transcript.segments.len();
    let n_words: usize = transcript.segments.iter().map(|s| s.words.len()).sum();
    let speakers: BTreeSet<String> = transcript
        .segments
        .iter()
        .flat_map(|s| s.words.iter())
        .filter_map(|w| w.speaker.clone())
        .collect();

    eprintln!("proc_s      : {proc_s:.2}");
    eprintln!("RTF         : {rtf:.3}");
    eprintln!("n_segments  : {n_segments}");
    eprintln!("n_words     : {n_words}");
    eprintln!("speakers    : {:?}", speakers.iter().collect::<Vec<_>>());
    eprintln!("wrote       : {out}");

    Ok(())
}
