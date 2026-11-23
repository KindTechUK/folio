//! In-app audio decode to 16 kHz mono `f32`, pure-Rust (no bundled ffmpeg).
//!
//! [`decode_to_16k_mono`] probes and decodes common meeting/dictation/podcast
//! containers (mp3, AAC-in-M4A/MP4, FLAC, WAV/PCM, OGG/Vorbis) via `symphonia`,
//! averages all channels to mono, then resamples to 16 kHz with `rubato`. This
//! keeps the single auditable binary — Symphonia + Rubato are codec/DSP crates
//! with no network dependencies (see `scripts/check-no-network.sh`).
//!
//! [`crate::load_wav_16k_mono`] (hound, strict 16 kHz/mono/16-bit) is kept for
//! the integration test and example; the Tauri command uses
//! `decode_to_16k_mono`.

use std::path::Path;

use rubato::{FftFixedInOut, Resampler};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::conv::IntoSample;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::Sample;

use crate::EngineError;

/// Target sample rate for the ASR/diarization pipeline.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Decode any supported audio file to 16 kHz mono `f32` samples in `[-1, 1]`.
///
/// Returns `(samples, 16_000)`. Channels are averaged to mono and the signal is
/// resampled to 16 kHz; if the source is already 16 kHz, resampling is skipped.
pub fn decode_to_16k_mono(path: impl AsRef<Path>) -> anyhow::Result<(Vec<f32>, u32)> {
    decode_to_16k_mono_with_progress(path, &|_| {})
}

/// Like [`decode_to_16k_mono`] but reports LOCAL progress in `[0, 1]`; the caller
/// maps it onto its global band (see `PROGRESS_DECODE_END`). Decode occupies the
/// first 85% of the local range, resampling the tail.
pub fn decode_to_16k_mono_with_progress(
    path: impl AsRef<Path>,
    progress: &dyn Fn(f32),
) -> anyhow::Result<(Vec<f32>, u32)> {
    let path = path.as_ref();
    let (mono, src_rate) = decode_to_mono(path, &|f| progress(0.85 * f))?;
    let samples = resample_to_16k(&mono, src_rate, &|f| progress(0.85 + 0.15 * f))?;
    Ok((samples, TARGET_SAMPLE_RATE))
}

/// Length of an audio file in seconds, read from its container header without
/// decoding the audio. `None` when the container doesn't declare it (some MP3s).
pub fn probe_duration_s(path: impl AsRef<Path>) -> anyhow::Result<Option<f64>> {
    let path = path.as_ref();
    let file = std::fs::File::open(path)
        .map_err(|e| EngineError::Audio(format!("opening {}: {e}", path.display())))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| EngineError::Audio(format!("probing {}: {e}", path.display())))?;
    let Some(track) = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    else {
        return Ok(None);
    };
    let params = &track.codec_params;
    Ok(match (params.n_frames, params.sample_rate) {
        (Some(frames), Some(rate)) if rate > 0 => Some(frames as f64 / rate as f64),
        _ => None,
    })
}

/// Decode `path` to mono `f32`, returning `(mono_samples, source_sample_rate)`.
/// `report` is called with decode progress in `[0, 1]` (best-effort: only when
/// the container declares a total frame count; otherwise it stays at 0).
fn decode_to_mono(path: &Path, report: &dyn Fn(f32)) -> anyhow::Result<(Vec<f32>, u32)> {
    let file = std::fs::File::open(path)
        .map_err(|e| EngineError::Audio(format!("opening {}: {e}", path.display())))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| EngineError::Audio(format!("probing {}: {e}", path.display())))?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| EngineError::Audio("no decodable audio track found".into()))?;
    let track_id = track.id;
    // Total per-channel frame count when the container declares it, so we can
    // report decode progress (mono.len() == frames appended so far).
    let total_frames = track.codec_params.n_frames;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| EngineError::Audio(format!("unsupported codec: {e}")))?;

    let mut mono: Vec<f32> = Vec::new();
    let mut src_rate: Option<u32> = None;
    let mut last_pct: u32 = u32::MAX;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // EOF / end of stream: stop cleanly.
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                return Err(EngineError::Audio("stream reset required (unsupported)".into()).into())
            }
            Err(e) => return Err(EngineError::Audio(format!("reading packet: {e}")).into()),
        };
        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                if src_rate.is_none() {
                    src_rate = Some(decoded.spec().rate);
                }
                downmix_into(&decoded, &mut mono);
                // Best-effort decode progress, throttled to whole-percent steps.
                if let Some(total) = total_frames.filter(|&t| t > 0) {
                    let pct = (100 * mono.len() as u64 / total) as u32;
                    if pct != last_pct {
                        last_pct = pct;
                        report((mono.len() as f64 / total as f64).min(1.0) as f32);
                    }
                }
            }
            // Recoverable decode hiccups: skip the packet (mirrors symphonia docs).
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(EngineError::Audio(format!("decoding: {e}")).into()),
        }
    }

    let src_rate = src_rate
        .ok_or_else(|| EngineError::Audio("decoded no audio frames".into()))?;
    if mono.is_empty() {
        return Err(EngineError::Audio("decoded no audio frames".into()).into());
    }
    Ok((mono, src_rate))
}

/// Average all channels of one decoded buffer into `out` (appending mono `f32`).
fn downmix_into(decoded: &AudioBufferRef<'_>, out: &mut Vec<f32>) {
    match decoded {
        AudioBufferRef::F32(buf) => downmix_planar(buf, out),
        AudioBufferRef::U8(buf) => downmix_planar_conv(buf, out),
        AudioBufferRef::U16(buf) => downmix_planar_conv(buf, out),
        AudioBufferRef::U24(buf) => downmix_planar_conv(buf, out),
        AudioBufferRef::U32(buf) => downmix_planar_conv(buf, out),
        AudioBufferRef::S8(buf) => downmix_planar_conv(buf, out),
        AudioBufferRef::S16(buf) => downmix_planar_conv(buf, out),
        AudioBufferRef::S24(buf) => downmix_planar_conv(buf, out),
        AudioBufferRef::S32(buf) => downmix_planar_conv(buf, out),
        AudioBufferRef::F64(buf) => downmix_planar_conv(buf, out),
    }
}

fn downmix_planar(buf: &symphonia::core::audio::AudioBuffer<f32>, out: &mut Vec<f32>) {
    let channels = buf.spec().channels.count();
    let frames = buf.frames();
    if channels == 0 {
        return;
    }
    for f in 0..frames {
        let mut acc = 0.0f32;
        for c in 0..channels {
            acc += buf.chan(c)[f];
        }
        out.push(acc / channels as f32);
    }
}

fn downmix_planar_conv<S>(buf: &symphonia::core::audio::AudioBuffer<S>, out: &mut Vec<f32>)
where
    S: Sample + IntoSample<f32>,
{
    let channels = buf.spec().channels.count();
    let frames = buf.frames();
    if channels == 0 {
        return;
    }
    for f in 0..frames {
        let mut acc = 0.0f32;
        for c in 0..channels {
            let s: f32 = buf.chan(c)[f].into_sample();
            acc += s;
        }
        out.push(acc / channels as f32);
    }
}

/// Resample mono `f32` from `src_rate` to 16 kHz. No-op if already 16 kHz.
/// `report` receives resample progress in `[0, 1]`.
fn resample_to_16k(mono: &[f32], src_rate: u32, report: &dyn Fn(f32)) -> anyhow::Result<Vec<f32>> {
    if src_rate == TARGET_SAMPLE_RATE {
        report(1.0);
        return Ok(mono.to_vec());
    }
    resample(mono, src_rate, TARGET_SAMPLE_RATE, report)
}

/// Fixed-ratio resample of a single mono channel using rubato's FFT resampler.
///
/// Processes in the resampler's fixed input-chunk size; the final partial chunk
/// is zero-padded. Output length is approximately `len * dst / src`.
fn resample(
    mono: &[f32],
    src_rate: u32,
    dst_rate: u32,
    report: &dyn Fn(f32),
) -> anyhow::Result<Vec<f32>> {
    if mono.is_empty() {
        return Ok(Vec::new());
    }
    // chunk_size is a processing hint; 1024 is a good offline default.
    let mut resampler =
        FftFixedInOut::<f32>::new(src_rate as usize, dst_rate as usize, 1024, 1)
            .map_err(|e| EngineError::Audio(format!("building resampler: {e}")))?;

    let mut out: Vec<f32> = Vec::with_capacity(mono.len() * dst_rate as usize / src_rate as usize + 1);
    let mut pos = 0usize;
    let mut in_buf: Vec<Vec<f32>> = vec![Vec::new()];
    let total = mono.len();
    let mut last_pct: u32 = u32::MAX;

    while pos < mono.len() {
        let need = resampler.input_frames_next();
        let end = (pos + need).min(mono.len());
        let mut frame: Vec<f32> = mono[pos..end].to_vec();
        let consumed = frame.len();
        if frame.len() < need {
            frame.resize(need, 0.0); // zero-pad the final partial chunk
        }
        in_buf[0] = frame;
        let resampled = resampler
            .process(&in_buf, None)
            .map_err(|e| EngineError::Audio(format!("resampling: {e}")))?;
        out.extend_from_slice(&resampled[0]);
        pos += consumed;
        let pct = (100 * pos / total) as u32;
        if pct != last_pct {
            last_pct = pct;
            report((pos as f32 / total as f32).min(1.0));
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_48k_to_16k_has_one_third_length() {
        // 1 second of a 48 kHz sine -> ~1 second at 16 kHz (length ~ x1/3).
        let src_rate = 48_000u32;
        let n = src_rate as usize;
        let freq = 440.0f32;
        let sine: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / src_rate as f32).sin())
            .collect();

        let out = resample(&sine, src_rate, 16_000, &|_| {}).unwrap();
        let expected = n / 3;
        let lo = (expected as f32 * 0.97) as usize;
        let hi = (expected as f32 * 1.03) as usize;
        assert!(
            out.len() >= lo && out.len() <= hi,
            "expected ~{expected} samples (±3%), got {}",
            out.len()
        );
        // Resampled sine should stay bounded (no blow-up).
        assert!(out.iter().all(|s| s.abs() <= 1.5), "samples bounded");
    }

    #[test]
    fn stereo_downmix_averages_channels() {
        use symphonia::core::audio::{
            AsAudioBufferRef, AudioBuffer, Channels, SignalSpec,
        };

        let spec = SignalSpec::new(
            44_100,
            Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
        );
        let mut buf = AudioBuffer::<f32>::new(3, spec);
        buf.render_reserved(Some(3));
        // L = [1, 0, -1], R = [0, 1, 1] -> mono avg = [0.5, 0.5, 0.0]
        {
            let (l, r) = buf.chan_pair_mut(0, 1);
            l.copy_from_slice(&[1.0, 0.0, -1.0]);
            r.copy_from_slice(&[0.0, 1.0, 1.0]);
        }

        let mut out = Vec::new();
        let aref = buf.as_audio_buffer_ref();
        downmix_into(&aref, &mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.0]);
    }

    /// Decodes a real `downloads/*.m4a`; run with `--include-ignored` when one is present.
    #[test]
    #[ignore = "needs downloads/*.m4a"]
    fn decode_real_m4a_if_present() {
        let downloads = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../downloads");
        let path = std::fs::read_dir(&downloads)
            .expect("downloads/ dir must exist when running this ignored test")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .find(|p| p.extension().and_then(|e| e.to_str()) == Some("m4a"))
            .expect("downloads/ must contain an .m4a");

        let (samples, rate) = decode_to_16k_mono(&path).expect("decode m4a");
        assert_eq!(rate, 16_000);
        assert!(!samples.is_empty(), "decoded samples must be non-empty");
        let secs = samples.len() as f64 / rate as f64;
        eprintln!(
            "decoded {}: {} samples = {:.1}s @ {} Hz mono",
            path.display(),
            samples.len(),
            secs,
            rate
        );
        assert!(secs > 1.0, "expected more than 1s of audio, got {secs:.1}s");
    }
}
