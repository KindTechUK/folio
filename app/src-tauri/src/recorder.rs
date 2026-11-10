//! In-app recording with a live transcript.
//!
//! The microphone is captured on its own thread (cpal), converted to 16 kHz
//! mono, appended to a WAV in `<library>/recordings/`, and cut into utterances
//! by the live VAD profile. A decoder thread transcribes each utterance as it
//! closes and emits it to the UI. On stop, the WAV is finalised and the
//! utterances (optionally with speaker labels from diarizing the whole WAV)
//! become a normal transcript.
//!
//! The WAV header is rewritten every few seconds, so a crash leaves a playable
//! file missing at most the last few seconds. Recordings are approved in the
//! same registry as picked files, so "Delete transcript and audio" removes
//! them like any other source recording.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use engine::{
    load_wav_16k_mono, merge, AsrSegment, EngineConfig, SherpaEngine, StreamResampler,
    TranscriptionEngine, VadProfile,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::system;

const SAMPLE_RATE: u32 = 16_000;
/// Rewrite the WAV header this often (in 16 kHz samples): at most this much
/// audio is lost if the app is killed.
const FLUSH_EVERY: usize = 5 * SAMPLE_RATE as usize;
/// How often the UI gets a level/elapsed update (in 16 kHz samples).
const LEVEL_EVERY: usize = SAMPLE_RATE as usize / 10;
/// Rebuild the recognizer after this long, at a quiet moment: memory grows
/// slowly when one recognizer decodes hundreds of utterances (spike S1 soak).
const RECOGNIZER_REFRESH: Duration = Duration::from_secs(30 * 60);

pub(crate) fn recordings_dir(library: &Path) -> PathBuf {
    library.join("recordings")
}

// ---------------------------------------------------------------------------
// Recording file (pure: no device, no Tauri)
// ---------------------------------------------------------------------------

/// A 16 kHz mono 16-bit WAV being written. The header is kept valid as it
/// grows, so the file is readable even if `finish` never runs.
pub(crate) struct RecordingFile {
    writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    since_flush: usize,
    written: usize,
}

impl RecordingFile {
    /// Create `<library>/recordings/<name>.wav` (never overwriting) and approve
    /// it for deletion. Returns the file and its path.
    pub(crate) fn create_in(library: &Path, name: &str) -> Result<(Self, PathBuf), String> {
        let dir = recordings_dir(library);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let mut path = dir.join(format!("{name}.wav"));
        let mut n = 2;
        let file = loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(f) => break f,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    path = dir.join(format!("{name} ({n}).wav"));
                    n += 1;
                }
                Err(e) => return Err(format!("cannot create {}: {e}", path.display())),
            }
        };
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(std::io::BufWriter::new(file), spec)
            .map_err(|e| format!("cannot start {}: {e}", path.display()))?;
        writer.flush().map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        system::approve_recording_in(library, &path)?;
        Ok((Self { writer, since_flush: 0, written: 0 }, path))
    }

    /// Append 16 kHz mono samples in `[-1, 1]`.
    pub(crate) fn append(&mut self, samples: &[f32]) -> Result<(), String> {
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            self.writer.write_sample(v).map_err(|e| format!("writing recording: {e}"))?;
        }
        self.written += samples.len();
        self.since_flush += samples.len();
        if self.since_flush >= FLUSH_EVERY {
            self.flush()?;
        }
        Ok(())
    }

    /// Write buffered audio and update the header to cover it.
    pub(crate) fn flush(&mut self) -> Result<(), String> {
        self.since_flush = 0;
        self.writer.flush().map_err(|e| format!("writing recording: {e}"))
    }

    pub(crate) fn written(&self) -> usize {
        self.written
    }

    pub(crate) fn finish(self) -> Result<(), String> {
        self.writer.finalize().map_err(|e| format!("finishing recording: {e}"))
    }
}

/// Delete a recording this app created and drop its approval. Refuses paths
/// outside `<library>/recordings/`.
pub(crate) fn discard_recording_in(library: &Path, path: &Path) -> Result<(), String> {
    let dir = recordings_dir(library)
        .canonicalize()
        .map_err(|e| format!("cannot resolve recordings folder: {e}"))?;
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", path.display()))?;
    if canonical.parent() != Some(dir.as_path()) {
        return Err(format!("not a Folio recording: {}", path.display()));
    }
    std::fs::remove_file(&canonical)
        .map_err(|e| format!("cannot delete {}: {e}", canonical.display()))?;
    system::forget_approved_recording_in(library, &canonical);
    Ok(())
}

// ---------------------------------------------------------------------------
// Live session (device + threads)
// ---------------------------------------------------------------------------

enum Control {
    Pause,
    Resume,
    Stop,
}

enum Audio {
    Frames(Vec<f32>),
    Pause,
    Resume,
    End,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Level {
    level: f32,
    speaking: bool,
    elapsed_s: f64,
}

struct Active {
    path: PathBuf,
    diarize: bool,
    cfg: EngineConfig,
    control: Sender<Control>,
    capture: JoinHandle<()>,
    process: JoinHandle<Result<(), String>>,
    decode: JoinHandle<()>,
    segments: Arc<Mutex<Vec<AsrSegment>>>,
}

#[derive(Default)]
pub struct RecorderState(Mutex<Option<Active>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedRecording {
    audio_path: String,
}

fn to_f32<T>(data: &[T]) -> Vec<f32>
where
    T: cpal::Sample,
    f32: cpal::FromSample<T>,
{
    data.iter().map(|s| s.to_sample::<f32>()).collect()
}

/// Open the default microphone on the current thread. Returns the stream
/// (paused) and its rate and channel count.
fn open_input(
    tx: Sender<Audio>,
    app: AppHandle,
) -> Result<(cpal::Stream, u32, u16), String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or_else(|| "No microphone found. Connect one and try again.".to_string())?;
    let supported = device.default_input_config().map_err(mic_error)?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let (rate, channels) = (config.sample_rate, config.channels);
    let err_app = app.clone();
    let on_error = move |e: cpal::Error| {
        let _ = err_app.emit("record-error", format!("Microphone error: {e}"));
    };
    macro_rules! build {
        ($t:ty) => {{
            let tx = tx.clone();
            device.build_input_stream::<$t, _, _>(
                config.clone(),
                move |data: &[$t], _| {
                    let _ = tx.send(Audio::Frames(to_f32(data)));
                },
                on_error,
                None,
            )
        }};
    }
    let stream = match format {
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::F64 => build!(f64),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::I32 => build!(i32),
        cpal::SampleFormat::U16 => build!(u16),
        cpal::SampleFormat::U8 => build!(u8),
        cpal::SampleFormat::I8 => build!(i8),
        other => return Err(format!("Unsupported microphone sample format: {other:?}")),
    }
    .map_err(mic_error)?;
    Ok((stream, rate, channels))
}

fn mic_error(e: cpal::Error) -> String {
    let hint = if cfg!(target_os = "windows") {
        " Check Settings > Privacy & security > Microphone, and allow desktop apps to use it."
    } else if cfg!(target_os = "macos") {
        " Check System Settings > Privacy & Security > Microphone, and allow Folio."
    } else {
        ""
    };
    format!("Cannot use the microphone: {e}.{hint}")
}

/// Capture thread: owns the stream (not `Send` on every platform) and turns
/// control messages into stream pause/play plus markers in the audio queue.
fn capture_thread(
    app: AppHandle,
    tx: Sender<Audio>,
    control: Receiver<Control>,
    ready: Sender<Result<(u32, u16), String>>,
) {
    let (stream, rate, channels) = match open_input(tx.clone(), app) {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    if let Err(e) = stream.play() {
        let _ = ready.send(Err(mic_error(e)));
        return;
    }
    let _ = ready.send(Ok((rate, channels)));
    while let Ok(msg) = control.recv() {
        match msg {
            Control::Pause => {
                let _ = stream.pause();
                let _ = tx.send(Audio::Pause);
            }
            Control::Resume => {
                let _ = tx.send(Audio::Resume);
                let _ = stream.play();
            }
            Control::Stop => break,
        }
    }
    drop(stream);
    let _ = tx.send(Audio::End);
}

/// Processing thread: resample, write, VAD, and hand utterances to the decoder.
fn process_thread(
    app: AppHandle,
    engine: SherpaEngine,
    file: RecordingFile,
    mut resampler: StreamResampler,
    audio: Receiver<Audio>,
    utterances: Sender<(usize, Vec<f32>)>,
) -> Result<(), String> {
    let vad = engine.vad(VadProfile::LIVE).map_err(|e| e.to_string())?;
    let mut p = Processor { app, file, vad, utterances, base: 0, since_level: 0, level_acc: 0.0 };
    while let Ok(msg) = audio.recv() {
        match msg {
            Audio::Frames(frames) => {
                let out = resampler.push(&frames).map_err(|e| e.to_string())?;
                p.samples(&out)?;
            }
            Audio::Pause => {
                let closed = p.vad.flush();
                p.send(closed);
                p.file.flush()?;
            }
            Audio::Resume => {
                p.vad.reset();
                p.base = p.file.written();
            }
            Audio::End => break,
        }
    }
    let tail = resampler.finish().map_err(|e| e.to_string())?;
    p.samples(&tail)?;
    let closed = p.vad.flush();
    p.send(closed);
    p.file.finish()
}

struct Processor {
    app: AppHandle,
    file: RecordingFile,
    vad: engine::Vad,
    utterances: Sender<(usize, Vec<f32>)>,
    /// VAD positions restart after a pause; `base` maps them back onto the WAV.
    base: usize,
    since_level: usize,
    level_acc: f64,
}

impl Processor {
    fn send(&self, closed: Vec<(usize, Vec<f32>)>) {
        for (start, chunk) in closed {
            let _ = self.utterances.send((self.base + start, chunk));
        }
    }

    fn samples(&mut self, samples: &[f32]) -> Result<(), String> {
        self.file.append(samples)?;
        let closed = self.vad.accept(samples);
        self.send(closed);
        self.level_acc += samples.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>();
        self.since_level += samples.len();
        if self.since_level >= LEVEL_EVERY {
            let rms = (self.level_acc / self.since_level as f64).sqrt() as f32;
            let _ = self.app.emit(
                "record-level",
                Level {
                    level: (rms * 4.0).min(1.0),
                    speaking: self.vad.speaking(),
                    elapsed_s: self.file.written() as f64 / SAMPLE_RATE as f64,
                },
            );
            self.since_level = 0;
            self.level_acc = 0.0;
        }
        Ok(())
    }
}

/// Decoder thread: transcribe utterances in order and emit each one.
fn decode_thread(
    app: AppHandle,
    engine: SherpaEngine,
    utterances: Receiver<(usize, Vec<f32>)>,
    segments: Arc<Mutex<Vec<AsrSegment>>>,
    mut recognizer: engine::Recognizer,
) {
    let mut built = Instant::now();
    while let Ok((start, chunk)) = utterances.recv() {
        if let Some(seg) = recognizer.decode_utterance(start, &chunk) {
            let _ = app.emit("record-utterance", &seg);
            segments.lock().unwrap().push(seg);
        }
        if built.elapsed() >= RECOGNIZER_REFRESH {
            if let Ok(fresh) = engine.recognizer() {
                recognizer = fresh;
            }
            built = Instant::now();
        }
    }
}

fn local_name() -> String {
    chrono::Local::now().format("Recording %Y-%m-%d %H-%M-%S").to_string()
}

/// Start recording from the default microphone. Loads the model, opens the
/// device, and returns the path of the new recording.
#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    model: Option<String>,
    diarize: Option<bool>,
) -> Result<StartedRecording, String> {
    let library = crate::library::library_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<RecorderState>();
        if state.0.lock().unwrap().is_some() {
            return Err("A recording is already in progress.".to_string());
        }
        let mut cfg = EngineConfig::new("");
        cfg.diarize = diarize.unwrap_or(false);
        cfg.asr_model = crate::commands::parse_asr_model(model.as_deref());
        cfg.models_dir = crate::commands::resolve_models_dir(&app, cfg.asr_model, cfg.diarize)?;
        let engine = SherpaEngine::new(&cfg).map_err(|e| e.to_string())?;
        let recognizer = engine.recognizer().map_err(|e| e.to_string())?;

        let (audio_tx, audio_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let capture_app = app.clone();
        let capture = std::thread::spawn(move || {
            capture_thread(capture_app, audio_tx, control_rx, ready_tx)
        });
        let (rate, channels) = ready_rx
            .recv()
            .map_err(|_| "The microphone thread stopped unexpectedly.".to_string())??;

        let resampler = StreamResampler::new(rate, channels).map_err(|e| e.to_string())?;
        let (file, path) = match RecordingFile::create_in(&library, &local_name()) {
            Ok(v) => v,
            Err(e) => {
                let _ = control_tx.send(Control::Stop);
                let _ = capture.join();
                return Err(e);
            }
        };

        let segments = Arc::new(Mutex::new(Vec::new()));
        let (utt_tx, utt_rx) = mpsc::channel();
        let decode_engine = SherpaEngine::new(&cfg).map_err(|e| e.to_string())?;
        let (decode_app, decode_segments) = (app.clone(), segments.clone());
        let decode = std::thread::spawn(move || {
            decode_thread(decode_app, decode_engine, utt_rx, decode_segments, recognizer)
        });
        let process_app = app.clone();
        let process = std::thread::spawn(move || {
            process_thread(process_app, engine, file, resampler, audio_rx, utt_tx)
        });

        *state.0.lock().unwrap() = Some(Active {
            path: path.clone(),
            diarize: cfg.diarize,
            cfg,
            control: control_tx,
            capture,
            process,
            decode,
            segments,
        });
        Ok(StartedRecording { audio_path: path.to_string_lossy().into_owned() })
    })
    .await
    .map_err(|e| format!("recording task failed: {e}"))?
}

fn send_control(app: &AppHandle, msg: Control) -> Result<(), String> {
    let state = app.state::<RecorderState>();
    let guard = state.0.lock().unwrap();
    let active = guard.as_ref().ok_or_else(|| "No recording is in progress.".to_string())?;
    active.control.send(msg).map_err(|_| "The recording has stopped.".to_string())
}

#[tauri::command]
pub fn pause_recording(app: AppHandle) -> Result<(), String> {
    send_control(&app, Control::Pause)
}

#[tauri::command]
pub fn resume_recording(app: AppHandle) -> Result<(), String> {
    send_control(&app, Control::Resume)
}

struct Finished {
    path: PathBuf,
    diarize: bool,
    cfg: EngineConfig,
    segments: Vec<AsrSegment>,
}

/// Stop capture and wait until the file is finished and every utterance is
/// transcribed. Returns the utterances in time order.
fn finish(app: &AppHandle) -> Result<Finished, String> {
    let active = app
        .state::<RecorderState>()
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| "No recording is in progress.".to_string())?;
    let _ = active.control.send(Control::Stop);
    let _ = active.capture.join();
    let written = active
        .process
        .join()
        .map_err(|_| "The recording thread failed.".to_string())?;
    let _ = active.decode.join();
    written?;
    let mut segments = active.segments.lock().unwrap().clone();
    segments.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    Ok(Finished { path: active.path, diarize: active.diarize, cfg: active.cfg, segments })
}

/// Stop recording and build the transcript: the live utterances, with speaker
/// labels from diarizing the whole recording if speakers were requested.
#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    num_speakers: Option<u32>,
) -> Result<engine::Transcript, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let done = finish(&app)?;
        if !done.diarize {
            let _ = app.emit("transcribe-progress", engine::Progress::new("done", 1, 1, 1.0));
            return Ok(merge::from_asr(&done.segments));
        }
        let _ = app.emit("transcribe-progress", engine::Progress::new("diarizing", 0, 1, 0.95));
        let (samples, sr) = load_wav_16k_mono(&done.path).map_err(|e| e.to_string())?;
        let eng = SherpaEngine::new(&done.cfg).map_err(|e| e.to_string())?;
        let spans = eng.diarize(&samples, sr, num_speakers).map_err(|e| e.to_string())?;
        let _ = app.emit("transcribe-progress", engine::Progress::new("done", 1, 1, 1.0));
        Ok(merge::merge(&done.segments, &spans))
    })
    .await
    .map_err(|e| format!("recording task failed: {e}"))?
}

/// Stop recording and delete the audio. Nothing is saved.
#[tauri::command]
pub async fn discard_recording(app: AppHandle) -> Result<(), String> {
    let library = crate::library::library_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let done = finish(&app)?;
        discard_recording_in(&library, &done.path)
    })
    .await
    .map_err(|e| format!("recording task failed: {e}"))?
}

/// On app exit, finish the WAV so the audio is kept intact. No transcript is
/// created; the recording stays in the recordings folder.
pub(crate) fn finish_on_exit(app: &AppHandle) {
    let _ = finish(app);
}

/// Let the asset protocol serve recordings for playback. Granted at runtime,
/// for this folder only, so the static config never names app data.
pub(crate) fn allow_playback(app: &AppHandle) -> Result<(), String> {
    let dir = recordings_dir(&crate::library::library_dir(app)?);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    app.asset_protocol_scope()
        .allow_directory(&dir, false)
        .map_err(|e| format!("cannot allow playback of recordings: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_tmp(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("folio-rec-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ramp(n: usize) -> Vec<f32> {
        (0..n).map(|i| ((i % 200) as f32 / 100.0) - 1.0).collect()
    }

    #[test]
    fn a_recording_abandoned_without_finishing_is_a_readable_wav_with_every_flushed_sample() {
        let library = unique_tmp("crash");
        let (mut file, path) = RecordingFile::create_in(&library, "Recording test").unwrap();
        file.append(&ramp(3 * 16_000)).unwrap();
        file.flush().unwrap();
        // A crash: the writer is never finished or dropped.
        std::mem::forget(file);

        let (samples, sr) = load_wav_16k_mono(&path).unwrap();
        assert_eq!(sr, 16_000);
        assert_eq!(samples.len(), 48_000);
        let _ = std::fs::remove_dir_all(&library);
    }

    #[test]
    fn delete_transcript_and_audio_removes_a_recording_and_its_session() {
        let library = unique_tmp("delete");
        let (mut file, path) = RecordingFile::create_in(&library, "Recording test").unwrap();
        file.append(&ramp(16_000)).unwrap();
        file.finish().unwrap();
        crate::library::save_session_in(
            &library,
            &serde_json::json!({ "id": "s1", "audioPath": path.to_string_lossy() }),
        )
        .unwrap();

        let report = system::delete_matter_authorized_in(&library, "s1", |_, _| true).unwrap();

        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert!(!path.exists());
        assert!(!library.join("s1.json").exists());
        let _ = std::fs::remove_dir_all(&library);
    }

    #[test]
    fn discarding_deletes_the_recording_and_its_approval() {
        let library = unique_tmp("discard");
        let (file, path) = RecordingFile::create_in(&library, "Recording test").unwrap();
        file.finish().unwrap();
        let canonical = path.canonicalize().unwrap();

        discard_recording_in(&library, &path).unwrap();

        assert!(!path.exists());
        assert!(!system::approved_recordings_in(&library).unwrap().contains(&canonical));
        let _ = std::fs::remove_dir_all(&library);
    }
}
