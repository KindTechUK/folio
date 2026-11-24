//! Spike S5: link llama.cpp (via `llama-cpp-2`) and sherpa-onnx (via the
//! production `engine` crate) into ONE CPU-only binary, then prove both native
//! runtimes start in the same process.
//!
//! Usage:
//!   s5-llama --self-check
//!       Starts sherpa-onnx/onnxruntime and the llama.cpp CPU backend, then exits
//!       0. No model needed (this is what CI runs on Windows, macOS and Linux).
//!   s5-llama --model <file.gguf> [--threads 4] [--n-predict 64] [--ctx 2048]
//!            [--prompt "..."] [--vad <silero_vad.onnx>]
//!       Loads the GGUF on the CPU and generates up to `--n-predict` tokens,
//!       printing load time, prompt and generation tokens/s, and peak RSS.
//!       `--vad` also runs a Silero VAD session through sherpa-onnx in the same
//!       process first, so both runtimes have live threads and allocations.
//!
//! Throwaway spike code: not linked into the app.

use std::path::PathBuf;
use std::time::Instant;

struct Args {
    self_check: bool,
    model: Option<PathBuf>,
    vad: Option<PathBuf>,
    threads: i32,
    n_predict: usize,
    ctx: u32,
    prompt: String,
}

const DEFAULT_PROMPT: &str =
    "Summarise in two sentences why a law firm might want transcription to run offline.";

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        self_check: false,
        model: None,
        vad: None,
        threads: 4,
        n_predict: 64,
        ctx: 2048,
        prompt: DEFAULT_PROMPT.to_string(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--self-check" => a.self_check = true,
            "--model" => a.model = Some(PathBuf::from(value()?)),
            "--vad" => a.vad = Some(PathBuf::from(value()?)),
            "--threads" => a.threads = value()?.parse().map_err(|e| format!("--threads: {e}"))?,
            "--n-predict" => {
                a.n_predict = value()?.parse().map_err(|e| format!("--n-predict: {e}"))?
            }
            "--ctx" => a.ctx = value()?.parse().map_err(|e| format!("--ctx: {e}"))?,
            "--prompt" => a.prompt = value()?,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if !a.self_check && a.model.is_none() {
        return Err("pass --self-check or --model <file.gguf>".into());
    }
    Ok(a)
}

/// Peak resident set size of this process, in MB (same method as asr-bench).
fn peak_rss_mb() -> Option<f64> {
    #[cfg(unix)]
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 {
            return None;
        }
        let raw = u.ru_maxrss as f64;
        // macOS reports bytes, Linux kilobytes.
        let bytes = if cfg!(target_os = "macos") { raw } else { raw * 1024.0 };
        Some(bytes / 1e6)
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let mut c: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) == 0 {
            return None;
        }
        Some(c.PeakWorkingSetSize as f64 / 1e6)
    }
    #[cfg(not(any(unix, windows)))]
    None
}

/// sherpa-onnx side: run native code in both sherpa-onnx and onnxruntime, and
/// go through the engine's real constructor (which references the full
/// recognizer/VAD/diarization graph, so the linker must resolve all of it).
fn sherpa_check() -> Result<(), String> {
    println!(
        "sherpa-onnx {} (git {}), onnxruntime {}",
        sherpa_onnx::version(),
        sherpa_onnx::git_sha1(),
        sherpa_onnx::onnxruntime_version()
    );
    let empty = std::env::temp_dir().join(format!("s5-llama-no-models-{}", std::process::id()));
    let cfg = engine::EngineConfig::new(&empty);
    match engine::SherpaEngine::new(&cfg) {
        Ok(_) => return Err("SherpaEngine::new unexpectedly succeeded with no models".into()),
        Err(e) => println!("engine: SherpaEngine::new without models fails closed as expected ({e})"),
    }
    println!(
        "engine: {} model files required for the default config",
        engine::required_model_files(cfg.asr_model, cfg.diarize).len()
    );
    Ok(())
}

/// Run a real Silero VAD session through sherpa-onnx on one second of silence.
fn sherpa_vad(path: &std::path::Path) -> Result<(), String> {
    let t = Instant::now();
    let mut silero = sherpa_onnx::SileroVadModelConfig::default();
    silero.model = Some(path.to_string_lossy().into_owned());
    silero.threshold = 0.5;
    silero.min_silence_duration = 0.25;
    silero.min_speech_duration = 0.25;
    silero.max_speech_duration = 20.0;
    silero.window_size = 512;
    let cfg = sherpa_onnx::VadModelConfig {
        silero_vad: silero,
        ten_vad: Default::default(),
        sample_rate: 16_000,
        num_threads: 1,
        provider: Some("cpu".to_string()),
        debug: false,
    };
    let vad = sherpa_onnx::VoiceActivityDetector::create(&cfg, 30.0)
        .ok_or_else(|| format!("could not load VAD model {}", path.display()))?;
    vad.accept_waveform(&vec![0.0f32; 16_000]);
    vad.flush();
    println!(
        "sherpa VAD: loaded {} and ran 1 s of silence in {:.0} ms (speech found: {})",
        path.display(),
        t.elapsed().as_secs_f64() * 1e3,
        !vad.is_empty()
    );
    Ok(())
}

#[cfg(feature = "llama")]
mod llama {
    use super::{peak_rss_mb, Args};
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{LlamaChatMessage, LlamaModel};
    use llama_cpp_2::sampling::LlamaSampler;
    use std::num::NonZeroU32;
    use std::time::Instant;

    pub fn init() -> Result<LlamaBackend, String> {
        let mut backend = LlamaBackend::init().map_err(|e| format!("llama backend init: {e}"))?;
        if std::env::var_os("S5_LLAMA_LOGS").is_none() {
            backend.void_logs();
        }
        Ok(backend)
    }

    pub fn self_check(backend: &LlamaBackend) -> Result<(), String> {
        let devices = llama_cpp_2::list_llama_ggml_backend_devices();
        let names: Vec<String> =
            devices.iter().map(|d| format!("{} ({})", d.name, d.backend)).collect();
        println!(
            "llama.cpp: backend initialised; devices: [{}]; gpu offload supported: {}; mmap: {}",
            names.join(", "),
            backend.supports_gpu_offload(),
            backend.supports_mmap()
        );
        if devices.is_empty() {
            return Err("llama.cpp registered no backend device".into());
        }
        if devices.iter().any(|d| d.backend != "CPU") {
            return Err("a non-CPU ggml backend is compiled in; the spike must be CPU-only".into());
        }
        Ok(())
    }

    pub fn generate(backend: &LlamaBackend, a: &Args) -> Result<(), String> {
        let path = a.model.as_ref().expect("checked by parse_args");
        let t_load = Instant::now();
        let mparams = LlamaModelParams::default().with_n_gpu_layers(0);
        let model = LlamaModel::load_from_file(backend, path, &mparams)
            .map_err(|e| format!("loading {}: {e}", path.display()))?;
        let cparams = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(a.ctx))
            // llama.cpp ABORTS the process (GGML_ASSERT) if one decode call gets
            // more tokens than n_batch, so let the whole prompt fit.
            .with_n_batch(a.ctx)
            .with_n_threads(a.threads)
            .with_n_threads_batch(a.threads);
        let mut ctx = model
            .new_context(backend, cparams)
            .map_err(|e| format!("creating context: {e}"))?;
        let load_s = t_load.elapsed().as_secs_f64();
        println!(
            "llama.cpp: loaded {} ({:.2} GB, {:.2} B params) in {load_s:.2} s; peak RSS so far {:.0} MB",
            path.display(),
            model.size() as f64 / 1e9,
            model.n_params() as f64 / 1e9,
            peak_rss_mb().unwrap_or(f64::NAN)
        );

        // Use the model's own chat template when it has one.
        let prompt = match model.chat_template(None) {
            Ok(tmpl) => {
                let msg = LlamaChatMessage::new("user".into(), a.prompt.clone())
                    .map_err(|e| e.to_string())?;
                model
                    .apply_chat_template(&tmpl, &[msg], true)
                    .map_err(|e| format!("chat template: {e}"))?
            }
            Err(_) => a.prompt.clone(),
        };
        let vocab = model.vocab();
        let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
        let n_prompt = tokens.len();
        if n_prompt + a.n_predict >= a.ctx as usize {
            return Err(format!(
                "prompt ({n_prompt} tokens) + --n-predict does not fit in --ctx {}",
                a.ctx
            ));
        }

        let mut batch = LlamaBatch::new(512.max(n_prompt), 1);
        let last = n_prompt as i32 - 1;
        for (i, tok) in tokens.iter().enumerate() {
            batch
                .add(*tok, i as i32, &[0], i as i32 == last)
                .map_err(|e| e.to_string())?;
        }
        let t_prompt = Instant::now();
        ctx.decode(&mut batch).map_err(|e| format!("prompt decode: {e}"))?;
        let prompt_s = t_prompt.elapsed().as_secs_f64();

        let mut sampler = LlamaSampler::greedy();
        let mut out = Vec::<u8>::new();
        let mut n_cur = n_prompt as i32;
        let mut n_gen = 0usize;
        let t_gen = Instant::now();
        while n_gen < a.n_predict {
            let tok = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(tok);
            if vocab.is_eog(tok) {
                break;
            }
            out.extend(vocab.token_to_piece(tok, false, None));
            n_gen += 1;
            batch.clear();
            batch.add(tok, n_cur, &[0], true).map_err(|e| e.to_string())?;
            n_cur += 1;
            ctx.decode(&mut batch).map_err(|e| format!("decode: {e}"))?;
        }
        let gen_s = t_gen.elapsed().as_secs_f64();

        println!("--- output ---\n{}\n--------------", String::from_utf8_lossy(&out).trim());
        println!(
            "RESULT threads={} load_s={load_s:.2} prompt_tokens={n_prompt} prompt_s={prompt_s:.3} prompt_tok_s={:.1} gen_tokens={n_gen} gen_s={gen_s:.3} gen_tok_s={:.1} peak_rss_mb={:.0}",
            a.threads,
            n_prompt as f64 / prompt_s,
            n_gen as f64 / gen_s,
            peak_rss_mb().unwrap_or(f64::NAN)
        );
        Ok(())
    }
}

fn run() -> Result<(), String> {
    let a = parse_args()?;
    sherpa_check()?;
    if let Some(vad) = &a.vad {
        sherpa_vad(vad)?;
    }

    #[cfg(feature = "llama")]
    {
        let backend = llama::init()?;
        llama::self_check(&backend)?;
        if a.model.is_some() {
            llama::generate(&backend, &a)?;
        }
    }
    #[cfg(not(feature = "llama"))]
    if a.model.is_some() {
        return Err("built without the `llama` feature; rebuild with default features".into());
    }

    println!(
        "OK: self-check passed; peak RSS {:.0} MB",
        peak_rss_mb().unwrap_or(f64::NAN)
    );
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}
