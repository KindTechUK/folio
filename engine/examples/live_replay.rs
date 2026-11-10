//! Spike S1 (docs/spikes/next-phase-spikes.md): can the existing Silero VAD +
//! Parakeet `OfflineRecognizer` path keep up with live speech?
//!
//! Throwaway spike code, not used by the app. It replays a 16 kHz mono WAV as if
//! it were a microphone, closes utterances with a "live" VAD profile, decodes
//! each one as it closes, and reports how far the text lags behind the speech.
//!
//!     cargo run --release --example live_replay -- <wav_16k_mono> \
//!         [--models-dir DIR] [--threads N] \
//!         [--silence 0.3,0.5,0.8] [--max-speech 10,15,20] \
//!         [--realtime] [--loops N] [--json OUT.jsonl] [--texts DIR]
//!
//! Modes:
//! - default (simulated clock): feeds the whole file as fast as possible but
//!   timestamps each utterance on the audio clock, and models a single decode
//!   worker (an utterance waits if the previous decode is still running). Decode
//!   times are real measurements. Runs every `--silence` x `--max-speech` pair,
//!   then compares each live transcript with the offline file path
//!   (0.8 s / 120 s profile) by word error rate. `--texts DIR` saves each
//!   transcript as plain text, for scoring against a human reference.
//! - `--realtime`: one profile only. A feeder thread paces the audio at real
//!   speed into the VAD and a decoder thread decodes utterances as they arrive,
//!   so lag is wall-clock. `--loops N` repeats the file for a soak run. On Unix,
//!   RSS and process CPU time are sampled every 10 s with `ps`.
//!
//! Lag definitions (seconds):
//! - `end_lag`: speech end -> text available (the S1 pass metric, p95 <= 5 s).
//!   Includes the VAD's silence wait, any queueing, and the decode itself.
//! - `start_lag`: speech start -> text available, i.e. how stale the oldest word
//!   in an utterance is when it appears.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use engine::{load_wav_16k_mono, process, AsrModel, EngineConfig, SherpaEngine};
use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig,
    SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};

const SR: u32 = 16_000;
const WINDOW: usize = 512;
const PARAKEET_DIR: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8";

fn usage() -> ! {
    eprintln!(
        "usage: live_replay <wav_16k_mono> [--models-dir DIR] [--threads N] \
         [--silence 0.3,0.5,0.8] [--max-speech 10,15,20] [--realtime] [--loops N] [--json OUT.jsonl] [--texts DIR]"
    );
    exit(2);
}

fn floats(v: &str) -> Vec<f32> {
    v.split(',')
        .map(|x| x.trim().parse().expect("expected comma-separated numbers"))
        .collect()
}

struct Args {
    wav: String,
    models_dir: PathBuf,
    threads: i32,
    silences: Vec<f32>,
    max_speeches: Vec<f32>,
    realtime: bool,
    loops: usize,
    json: Option<PathBuf>,
    texts: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let wav = it.next().unwrap_or_else(|| usage());
    let mut a = Args {
        wav,
        models_dir: Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("models"),
        threads: 4,
        silences: vec![0.3, 0.5, 0.8],
        max_speeches: vec![10.0, 15.0, 20.0],
        realtime: false,
        loops: 1,
        json: None,
        texts: None,
    };
    while let Some(f) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| usage());
        match f.as_str() {
            "--models-dir" => a.models_dir = PathBuf::from(val()),
            "--threads" => a.threads = val().parse().expect("--threads"),
            "--silence" => a.silences = floats(&val()),
            "--max-speech" => a.max_speeches = floats(&val()),
            "--realtime" => a.realtime = true,
            "--loops" => a.loops = val().parse().expect("--loops"),
            "--json" => a.json = Some(PathBuf::from(val())),
            "--texts" => a.texts = Some(PathBuf::from(val())),
            _ => usage(),
        }
    }
    a
}

fn recognizer(models_dir: &Path, threads: i32) -> OfflineRecognizer {
    // Same configuration as `SherpaEngine::build_recognizer` for Parakeet.
    let p = |n: &str| models_dir.join(PARAKEET_DIR).join(n).to_string_lossy().into_owned();
    let mut c = OfflineRecognizerConfig::default();
    c.model_config.transducer = OfflineTransducerModelConfig {
        encoder: Some(p("encoder.int8.onnx")),
        decoder: Some(p("decoder.int8.onnx")),
        joiner: Some(p("joiner.int8.onnx")),
    };
    c.model_config.model_type = Some("nemo_transducer".into());
    c.model_config.tokens = Some(p("tokens.txt"));
    c.decoding_method = Some("greedy_search".into());
    c.model_config.provider = Some("cpu".into());
    c.model_config.num_threads = threads;
    OfflineRecognizer::create(&c).expect("create OfflineRecognizer")
}

fn vad(models_dir: &Path, min_silence: f32, max_speech: f32) -> VoiceActivityDetector {
    // Same as `SherpaEngine::vad_chunks`, with the live profile's two durations.
    let mut s = SileroVadModelConfig::default();
    s.model = Some(models_dir.join("silero_vad.onnx").to_string_lossy().into_owned());
    s.threshold = 0.5;
    s.min_silence_duration = min_silence;
    s.min_speech_duration = 0.25;
    s.max_speech_duration = max_speech;
    let cfg = VadModelConfig {
        silero_vad: s,
        ten_vad: Default::default(),
        sample_rate: SR as i32,
        num_threads: 1,
        provider: Some("cpu".into()),
        debug: false,
    };
    VoiceActivityDetector::create(&cfg, 100.0).expect("create VAD")
}

fn decode(rec: &OfflineRecognizer, samples: &[f32]) -> String {
    let stream = rec.create_stream();
    stream.accept_waveform(SR as i32, samples);
    rec.decode(&stream);
    stream.get_result().map(|r| r.text.trim().to_string()).unwrap_or_default()
}

/// One closed utterance, timed on the audio clock (simulated) or wall clock (realtime).
struct Utt {
    start: f64,
    end: f64,
    arrival: f64,
    decode_s: f64,
    finish: f64,
    text: String,
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

fn norm_words(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word-level edit distance: (substitutions + deletions + insertions) / reference length.
fn wer(reference: &[String], hyp: &[String]) -> f64 {
    let (n, m) = (reference.len(), hyp.len());
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let sub = prev[j - 1] + usize::from(reference[i - 1] != hyp[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    if n == 0 {
        0.0
    } else {
        prev[m] as f64 / n as f64
    }
}

/// macOS: physical footprint in MB from `top` (RSS there drops as idle pages are
/// compressed, so it can't show whether memory stays flat).
#[cfg(target_os = "macos")]
fn footprint_mb() -> Option<f64> {
    let out = std::process::Command::new("top")
        .args(["-l", "1", "-pid", &std::process::id().to_string(), "-stats", "mem"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let v = s.lines().last()?.trim().trim_end_matches(['+', '-']);
    let (num, unit) = v.split_at(v.len().checked_sub(1)?);
    let n: f64 = num.parse().ok()?;
    Some(match unit {
        "K" => n / 1024.0,
        "M" => n,
        "G" => n * 1024.0,
        _ => return None,
    })
}

/// (memory MB, CPU seconds) for this process, via `ps` (and `top` for memory on
/// macOS). None where `ps` is unavailable.
fn ps_sample() -> Option<(f64, f64)> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=,time=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let mut parts = s.split_whitespace();
    let rss_kb: f64 = parts.next()?.parse().ok()?;
    // macOS: [MM:]SS.ss or H:MM:SS.ss ; Linux: [DD-]HH:MM:SS
    let t = parts.next()?;
    let secs = t
        .rsplit(':')
        .enumerate()
        .map(|(i, x)| x.parse::<f64>().unwrap_or(0.0) * 60f64.powi(i as i32))
        .sum();
    #[cfg(target_os = "macos")]
    let mem = footprint_mb().unwrap_or(rss_kb / 1024.0);
    #[cfg(not(target_os = "macos"))]
    let mem = rss_kb / 1024.0;
    Some((mem, secs))
}

fn ncpus() -> f64 {
    std::thread::available_parallelism().map(|n| n.get() as f64).unwrap_or(1.0)
}

fn simulate(
    samples: &[f32],
    models_dir: &Path,
    rec: &OfflineRecognizer,
    min_silence: f32,
    max_speech: f32,
) -> Vec<Utt> {
    let v = vad(models_dir, min_silence, max_speech);
    let mut utts = Vec::new();
    let mut busy_until = 0.0f64;
    let mut fed = 0usize;
    let mut drain = |v: &VoiceActivityDetector, fed: usize, utts: &mut Vec<Utt>| {
        while let Some(seg) = v.front() {
            let start = seg.start().max(0) as f64 / SR as f64;
            let end = start + seg.samples().len() as f64 / SR as f64;
            let arrival = fed as f64 / SR as f64;
            let t0 = Instant::now();
            let text = decode(rec, seg.samples());
            let decode_s = t0.elapsed().as_secs_f64();
            let begin = arrival.max(busy_until);
            busy_until = begin + decode_s;
            utts.push(Utt { start, end, arrival, decode_s, finish: busy_until, text });
            v.pop();
        }
    };
    for w in samples.chunks(WINDOW) {
        v.accept_waveform(w);
        fed += w.len();
        drain(&v, fed, &mut utts);
    }
    // "Stop": flush the open utterance at the end of the recording.
    v.flush();
    drain(&v, fed, &mut utts);
    utts
}

fn realtime(
    samples: Vec<f32>,
    models_dir: PathBuf,
    threads: i32,
    min_silence: f32,
    max_speech: f32,
    loops: usize,
) -> (Vec<Utt>, Vec<(f64, f64, f64)>) {
    let rec = recognizer(&models_dir, threads);
    let (tx, rx) = mpsc::channel::<(f64, f64, Vec<f32>, Instant)>();
    let t0 = Instant::now();
    let feeder = std::thread::spawn(move || {
        let v = vad(&models_dir, min_silence, max_speech);
        let mut fed = 0usize;
        let send = |v: &VoiceActivityDetector| {
            while let Some(seg) = v.front() {
                let start = seg.start().max(0) as f64 / SR as f64;
                let end = start + seg.samples().len() as f64 / SR as f64;
                tx.send((start, end, seg.samples().to_vec(), Instant::now())).unwrap();
                v.pop();
            }
        };
        for _ in 0..loops {
            for w in samples.chunks(WINDOW) {
                // Pace to real time: this window "arrives" once its last sample was spoken.
                let due = t0 + Duration::from_secs_f64((fed + w.len()) as f64 / SR as f64);
                if let Some(d) = due.checked_duration_since(Instant::now()) {
                    std::thread::sleep(d);
                }
                v.accept_waveform(w);
                fed += w.len();
                send(&v);
            }
        }
        v.flush();
        send(&v);
    });

    let mut utts = Vec::new();
    let mut mem = Vec::new(); // (wall_s, rss_mb, cpu_s)
    let mut next_sample = Instant::now();
    let ps_tick = |mem: &mut Vec<(f64, f64, f64)>, next: &mut Instant| {
        if Instant::now() >= *next {
            if let Some((rss, cpu)) = ps_sample() {
                mem.push((t0.elapsed().as_secs_f64(), rss, cpu));
            }
            *next += Duration::from_secs(10);
        }
    };
    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok((start, end, chunk, arrived)) => {
                let d0 = Instant::now();
                let text = decode(&rec, &chunk);
                let done = Instant::now();
                utts.push(Utt {
                    start,
                    end,
                    arrival: (arrived - t0).as_secs_f64(),
                    decode_s: (done - d0).as_secs_f64(),
                    finish: (done - t0).as_secs_f64(),
                    text,
                });
                let n = utts.len();
                if n % 20 == 0 {
                    eprintln!("  {n} utterances, audio {:.0} s", end);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        ps_tick(&mut mem, &mut next_sample);
    }
    feeder.join().unwrap();
    (utts, mem)
}

fn summary(utts: &[Utt], audio_s: f64) -> serde_json::Value {
    let end_lag = sorted(utts.iter().map(|u| u.finish - u.end).collect());
    let start_lag = sorted(utts.iter().map(|u| u.finish - u.start).collect());
    let lens = sorted(utts.iter().map(|u| u.end - u.start).collect());
    let decode_total: f64 = utts.iter().map(|u| u.decode_s).sum();
    let queued = utts.iter().filter(|u| u.finish - u.decode_s > u.arrival + 0.05).count();
    serde_json::json!({
        "n_utts": utts.len(),
        "utt_len_s": {"p50": pct(&lens, 50.0), "max": pct(&lens, 100.0)},
        "end_lag_s": {"p50": pct(&end_lag, 50.0), "p95": pct(&end_lag, 95.0), "max": pct(&end_lag, 100.0)},
        "start_lag_s": {"p50": pct(&start_lag, 50.0), "p95": pct(&start_lag, 95.0), "max": pct(&start_lag, 100.0)},
        "decode_busy_frac": decode_total / audio_s,
        "n_queued": queued,
    })
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn main() -> anyhow::Result<()> {
    let a = parse_args();
    let (samples, sr) = load_wav_16k_mono(&a.wav)?;
    assert_eq!(sr, SR, "need 16 kHz mono");
    let audio_s = samples.len() as f64 / SR as f64;
    let mut json = a
        .json
        .as_ref()
        .map(|p| std::fs::OpenOptions::new().create(true).append(true).open(p))
        .transpose()?;
    let mut emit = |v: serde_json::Value| {
        println!("{v}");
        if let Some(f) = json.as_mut() {
            writeln!(f, "{v}").unwrap();
        }
    };
    let base = serde_json::json!({
        "wav": Path::new(&a.wav).file_name().unwrap().to_string_lossy(),
        "audio_s": r2(audio_s), "threads": a.threads, "ncpus": ncpus(),
        "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
    });
    eprintln!("audio {audio_s:.1} s, threads {}, cpus {}", a.threads, ncpus());

    if a.realtime {
        let (ms, mx) = (a.silences[0], a.max_speeches[0]);
        eprintln!("realtime replay x{} at silence {ms} s / max {mx} s ...", a.loops);
        let cpu0 = ps_sample().map(|s| s.1);
        let (utts, mem) =
            realtime(samples.clone(), a.models_dir.clone(), a.threads, ms, mx, a.loops);
        let wall = mem.last().map(|m| m.0).unwrap_or(audio_s * a.loops as f64);
        let cpu = match (cpu0, mem.last()) {
            (Some(c0), Some(m)) => Some((m.2 - c0) / m.0 / ncpus() * 100.0),
            _ => None,
        };
        let rss: Vec<f64> = mem.iter().map(|m| m.1).collect();
        let mut s = summary(&utts, audio_s * a.loops as f64);
        s["mode"] = "realtime".into();
        s["min_silence"] = ms.into();
        s["max_speech"] = mx.into();
        s["loops"] = a.loops.into();
        s["wall_s"] = r2(wall).into();
        s["cpu_pct_of_machine"] = cpu.map(r2).into();
        s["rss_mb"] = serde_json::json!({
            "first": rss.first(), "min": rss.iter().cloned().fold(f64::MAX, f64::min),
            "max": rss.iter().cloned().fold(0.0, f64::max), "last": rss.last(),
        });
        s["rss_trace_mb"] = rss.iter().step_by(6).map(|x| r2(*x)).collect::<Vec<_>>().into();
        emit(merge(&base, s));
        return Ok(());
    }

    // Offline reference: the app's file path (default engine config, no diarization).
    eprintln!("offline reference ...");
    let mut cfg = EngineConfig::new(&a.models_dir);
    cfg.asr_model = AsrModel::ParakeetV2;
    cfg.num_threads = a.threads;
    let eng = SherpaEngine::new(&cfg)?;
    let t0 = Instant::now();
    let offline = process(&eng, &samples, SR, &cfg, &|_| {})?;
    let offline_s = t0.elapsed().as_secs_f64();
    let ref_text: String =
        offline.segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    let ref_words = norm_words(&ref_text);
    let stem = Path::new(&a.wav).file_stem().unwrap().to_string_lossy().into_owned();
    let save = |name: &str, text: &str| {
        if let Some(d) = &a.texts {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join(format!("{stem}.{name}.txt")), text).unwrap();
        }
    };
    save("offline", &ref_text);
    emit(merge(
        &base,
        serde_json::json!({"mode": "offline", "proc_s": r2(offline_s),
                           "rtf": offline_s / audio_s, "n_words": ref_words.len()}),
    ));

    let rec = recognizer(&a.models_dir, a.threads);
    for &ms in &a.silences {
        for &mx in &a.max_speeches {
            let cpu0 = ps_sample().map(|s| s.1);
            let utts = simulate(&samples, &a.models_dir, &rec, ms, mx);
            let cpu_s = match (cpu0, ps_sample()) {
                (Some(c0), Some((_, c1))) => Some(c1 - c0),
                _ => None,
            };
            let live_text: String =
                utts.iter().map(|u| u.text.as_str()).collect::<Vec<_>>().join(" ");
            let live_words = norm_words(&live_text);
            save(&format!("live-{ms}-{mx}"), &live_text);
            let mut s = summary(&utts, audio_s);
            s["mode"] = "simulated".into();
            s["min_silence"] = ms.into();
            s["max_speech"] = mx.into();
            s["wer_vs_offline"] = r2(100.0 * wer(&ref_words, &live_words)).into();
            s["n_words"] = live_words.len().into();
            // Average machine CPU if this ran live: CPU time spread over the audio's duration.
            s["proj_cpu_pct_of_machine"] = cpu_s.map(|c| r2(c / audio_s / ncpus() * 100.0)).into();
            emit(merge(&base, s));
        }
    }
    Ok(())
}

fn merge(base: &serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let mut out = base.clone();
    for (k, v) in extra.as_object().unwrap() {
        out[k] = v.clone();
    }
    out
}
