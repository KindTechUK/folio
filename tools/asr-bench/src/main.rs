//! `asr-bench`: see `README.md`.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use asr_bench::pack::Pack;
use asr_bench::report::{report, score};
use asr_bench::run::{read_results, run, RunArgs};
use asr_bench::segment::Segmenter;
use clap::{Parser, Subcommand};
use engine::AsrModel;

#[derive(Parser)]
#[command(about = "Speech-to-text evaluation bench for Folio's engine")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Decode one set of a pack and write results/<tag>.jsonl (resumes if it exists).
    Run {
        #[arg(long)]
        pack: PathBuf,
        #[arg(long)]
        set: String,
        /// Model name, e.g. parakeet-tdt-0.6b-v2, parakeet-ultra, qwen3-asr, cohere.
        #[arg(long, default_value = "parakeet-tdt-0.6b-v2")]
        model: String,
        /// Folder holding the sherpa-onnx model folders and silero_vad.onnx.
        #[arg(long, env = "FOLIO_MODELS_DIR", default_value = "models")]
        models_dir: PathBuf,
        /// vad:<silence>/<max>, vad-pad:<silence>/<max>/<pad_ms>, energy:<piece>/<search>, whole.
        #[arg(long, default_value = "vad:0.8/120")]
        segmenter: String,
        /// greedy or beam:<paths>.
        #[arg(long, default_value = "greedy")]
        decoding: String,
        /// One phrase per line.
        #[arg(long)]
        hotwords: Option<PathBuf>,
        #[arg(long, default_value_t = 1.5)]
        hotwords_score: f32,
        #[arg(long, default_value_t = 4)]
        threads: i32,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long, default_value = "results")]
        out: PathBuf,
        /// Only the first N clips of the set.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Print a report for one or more results files.
    Report {
        #[arg(long)]
        pack: PathBuf,
        /// Tag of the run to compare the others against.
        #[arg(long)]
        baseline: Option<String>,
        /// Write the markdown here as well as to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        results: Vec<PathBuf>,
    },
    /// Score raw text pairs from the command line, for checking the scorer.
    Score { reference: String, hypothesis: String },
    /// Print this machine's CPU, cores, RAM and OS.
    Machine,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Run {
            pack,
            set,
            model,
            models_dir,
            segmenter,
            decoding,
            hotwords,
            hotwords_score,
            threads,
            tag,
            out,
            limit,
        } => {
            let model = AsrModel::from_name(&model).ok_or_else(|| {
                let names: Vec<_> = AsrModel::ALL.iter().map(|m| m.stem()).collect();
                anyhow!("unknown model {model:?}; one of {names:?}")
            })?;
            let segmenter: Segmenter = segmenter.parse().map_err(|e: String| anyhow!(e))?;
            let path = run(RunArgs {
                pack,
                set,
                model,
                models_dir,
                segmenter,
                decoding,
                hotwords,
                hotwords_score,
                threads,
                tag,
                out,
                limit,
            })?;
            println!("{}", path.display());
        }
        Cmd::Report { pack, baseline, out, results } => {
            let pack = Pack::load(&pack)?;
            let scored = results
                .iter()
                .map(|p| read_results(p).map(|r| score(&r, &pack)))
                .collect::<Result<Vec<_>>>()?;
            let md = report(&scored, baseline.as_deref())?;
            if let Some(out) = out {
                std::fs::write(out, &md)?;
            }
            print!("{md}");
        }
        Cmd::Score { reference, hypothesis } => {
            let (e, n) = asr_bench::score_pair(&reference, &hypothesis);
            let n_ = asr_bench::normalize::english();
            println!("ref: {}", n_.words(&reference).join(" "));
            println!("hyp: {}", n_.words(&hypothesis).join(" "));
            println!(
                "{} errors ({} sub, {} del, {} ins) over {n} words = {:.2}% WER",
                e.total(),
                e.sub,
                e.del,
                e.ins,
                if n > 0 { 100.0 * e.total() as f64 / n as f64 } else { 0.0 }
            );
        }
        Cmd::Machine => {
            println!("{}", serde_json::to_string_pretty(&asr_bench::machine::machine())?);
        }
    }
    Ok(())
}
