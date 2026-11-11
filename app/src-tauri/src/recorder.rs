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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
/// grows and forced to disk every few seconds, so the file is readable even if
/// `finish` never runs (crash, power loss).
pub(crate) struct RecordingFile {
    writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    /// Second handle on the same file, for `sync_data`.
    sync: std::fs::File,
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
        let sync = file
            .try_clone()
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let writer = hound::WavWriter::new(std::io::BufWriter::new(file), spec)
            .map_err(|e| format!("cannot start {}: {e}", path.display()))?;
        let mut rec = Self { writer, sync, since_flush: 0, written: 0 };
        rec.flush()?;
        system::approve_recording_in(library, &path)?;
        Ok((rec, path))
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

    /// Append `seconds` of silence (time the microphone was unavailable), so
    /// the recording stays aligned with the clock.
    pub(crate) fn append_silence(&mut self, seconds: f64) -> Result<(), String> {
        let mut left = (seconds.max(0.0) * SAMPLE_RATE as f64).round() as usize;
        let zeros = vec![0.0f32; SAMPLE_RATE as usize];
        while left > 0 {
            let n = left.min(zeros.len());
            self.append(&zeros[..n])?;
            left -= n;
        }
        Ok(())
    }

    /// Write buffered audio, update the header to cover it, and force both to
    /// disk.
    pub(crate) fn flush(&mut self) -> Result<(), String> {
        self.since_flush = 0;
        self.writer.flush().map_err(|e| format!("writing recording: {e}"))?;
        self.sync.sync_data().map_err(|e| format!("writing recording: {e}"))
    }

    pub(crate) fn written(&self) -> usize {
        self.written
    }

    pub(crate) fn finish(self) -> Result<(), String> {
        self.writer.finalize().map_err(|e| format!("finishing recording: {e}"))?;
        self.sync.sync_data().map_err(|e| format!("finishing recording: {e}"))
    }
}

/// Read `len` samples starting at sample `start` from a 16-bit mono WAV that
/// may still be being written (only what is on disk is returned).
pub(crate) fn read_span(path: &Path, start: usize, len: usize) -> Result<Vec<f32>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut riff = [0u8; 12];
    f.read_exact(&mut riff).map_err(|e| format!("reading {}: {e}", path.display()))?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err(format!("not a WAV file: {}", path.display()));
    }
    // Walk the chunks to the audio data.
    let mut pos = 12u64;
    let data = loop {
        let mut head = [0u8; 8];
        f.read_exact(&mut head).map_err(|e| format!("reading {}: {e}", path.display()))?;
        let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as u64;
        if &head[0..4] == b"data" {
            break pos + 8;
        }
        pos += 8 + size + (size & 1);
        f.seek(SeekFrom::Start(pos)).map_err(|e| format!("reading {}: {e}", path.display()))?;
    };
    f.seek(SeekFrom::Start(data + 2 * start as u64))
        .map_err(|e| format!("reading {}: {e}", path.display()))?;
    let mut bytes = Vec::with_capacity(2 * len);
    f.take(2 * len as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("reading {}: {e}", path.display()))?;
    Ok(bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0)
        .collect())
}

/// The recording's path, resolved, if it is one of this app's recordings
/// (directly inside `<library>/recordings/`).
fn own_recording(library: &Path, path: &Path) -> Result<PathBuf, String> {
    let dir = recordings_dir(library)
        .canonicalize()
        .map_err(|e| format!("cannot resolve recordings folder: {e}"))?;
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", path.display()))?;
    if canonical.parent() != Some(dir.as_path()) {
        return Err(format!("not a Folio recording: {}", path.display()));
    }
    Ok(canonical)
}

/// Where the notes typed during a recording are kept until it stops:
/// `<recording>.notes.txt`, next to the WAV.
pub(crate) fn notes_path(recording: &Path) -> PathBuf {
    recording.with_extension("notes.txt")
}

/// Save the notes for an in-progress recording, replacing the file atomically
/// so a crash leaves either the old notes or the new ones.
pub(crate) fn save_recording_notes_in(library: &Path, recording: &Path, text: &str) -> Result<(), String> {
    let target = notes_path(&own_recording(library, recording)?);
    let tmp = target.with_extension("txt.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("cannot save notes: {e}"))?;
    std::fs::rename(&tmp, &target).map_err(|e| format!("cannot save notes: {e}"))
}

/// Remove a recording's notes file, if there is one.
fn remove_notes(recording: &Path) -> Result<(), String> {
    match std::fs::remove_file(notes_path(recording)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("cannot delete notes for {}: {e}", recording.display()))
        }
        _ => Ok(()),
    }
}

/// Delete a recording this app created, with its notes, and drop its approval.
/// Refuses paths outside `<library>/recordings/`.
pub(crate) fn discard_recording_in(library: &Path, path: &Path) -> Result<(), String> {
    let canonical = own_recording(library, path)?;
    remove_notes(&canonical)?;
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
    /// The device failed (error callback) or went quiet (watchdog).
    Lost,
    /// The system's default microphone changed; follow it.
    Switch,
}

enum Audio {
    Frames(Vec<f32>),
    Pause,
    Resume,
    /// A different (or the same) device was opened after a loss; `gap_s` of
    /// unrecorded time passed.
    Reopened(Input, f64),
    End,
}

/// The format of an opened input device.
#[derive(Clone)]
struct Input {
    rate: u32,
    channels: u16,
    name: String,
    /// Telephone-band capture: a headset in call mode (Bluetooth HFP) or any
    /// device delivering 16 kHz or less.
    narrowband: bool,
}

/// One closed utterance for the decoder. Past [`MAX_QUEUED`] of backlog, only
/// its position is queued and the decoder reads the samples back from the WAV.
enum Utterance {
    Samples(usize, Vec<f32>),
    Span(usize, usize),
}

impl Utterance {
    fn len(&self) -> usize {
        match self {
            Utterance::Samples(_, s) => s.len(),
            Utterance::Span(_, n) => *n,
        }
    }
}

/// Backlog held in memory before utterances are queued as WAV spans.
const MAX_QUEUED: usize = 120 * SAMPLE_RATE as usize;
/// No callbacks for this long while recording means the device is gone.
const STALL: Duration = Duration::from_secs(3);
/// Reconnection attempts after a device loss.
const RETRY_DELAYS: [u64; 5] = [1, 2, 4, 8, 8];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Level {
    level: f32,
    speaking: bool,
    elapsed_s: f64,
    /// Seconds of speech waiting to be transcribed.
    backlog_s: f64,
}

#[derive(Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Warnings {
    /// The microphone delivers digital silence (muted, blocked by privacy
    /// settings, or a driver problem).
    silent: bool,
    narrowband: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    message: Option<String>,
    /// Show briefly, then clear.
    transient: bool,
}

/// Raises a warning after a run of digital silence and clears it when sound
/// returns. Room noise, however quiet, is never exactly zero.
pub(crate) struct SilenceWatch {
    run: usize,
    limit: usize,
    warned: bool,
}

impl SilenceWatch {
    pub(crate) fn new(limit_samples: usize) -> Self {
        Self { run: 0, limit: limit_samples, warned: false }
    }

    /// Returns `Some(true)` when the warning starts, `Some(false)` when it clears.
    pub(crate) fn feed(&mut self, samples: &[f32]) -> Option<bool> {
        let mut change = None;
        for &s in samples {
            if s.abs() > 1e-6 {
                self.run = 0;
                if self.warned {
                    self.warned = false;
                    change = Some(false);
                }
            } else {
                self.run += 1;
                if !self.warned && self.run >= self.limit {
                    self.warned = true;
                    change = Some(true);
                }
            }
        }
        change
    }

    pub(crate) fn reset(&mut self) -> Option<bool> {
        self.run = 0;
        std::mem::replace(&mut self.warned, false).then_some(false)
    }
}

/// Time the microphone was unavailable while recording (not while paused),
/// which is written to the WAV as silence when the device comes back.
pub(crate) struct OutageClock {
    counted: Duration,
    since: Option<Instant>,
}

impl OutageClock {
    pub(crate) fn start(lost_at: Instant, paused: bool) -> Self {
        Self { counted: Duration::ZERO, since: (!paused).then_some(lost_at) }
    }

    pub(crate) fn pause(&mut self, now: Instant) {
        if let Some(since) = self.since.take() {
            self.counted += now.saturating_duration_since(since);
        }
    }

    pub(crate) fn resume(&mut self, now: Instant) {
        self.since.get_or_insert(now);
    }

    pub(crate) fn gap(&self, now: Instant) -> Duration {
        self.counted + self.since.map_or(Duration::ZERO, |s| now.saturating_duration_since(s))
    }
}

/// Whether a stream error means the device must be reopened. Glitches (xruns),
/// automatic rerouting and refused real-time priority leave the stream running.
fn needs_reopen(kind: cpal::ErrorKind) -> bool {
    matches!(
        kind,
        cpal::ErrorKind::DeviceNotAvailable
            | cpal::ErrorKind::StreamInvalidated
            | cpal::ErrorKind::HostUnavailable
            | cpal::ErrorKind::DeviceBusy
            | cpal::ErrorKind::BackendError
    )
}

struct Active {
    path: PathBuf,
    cfg: EngineConfig,
    control: Sender<Control>,
    capture: JoinHandle<()>,
    process: JoinHandle<Result<(), String>>,
    decode: JoinHandle<()>,
    segments: Arc<Mutex<Vec<AsrSegment>>>,
    cancel: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
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

/// Milliseconds since `t0`, for the stall watchdog.
fn ms_since(t0: Instant) -> u64 {
    t0.elapsed().as_millis() as u64
}

/// Open the default microphone on the current thread, not yet started (call
/// `play` once its format has been announced). Frames go to `tx`; device
/// errors and every callback are reported for the watchdog.
fn open_input(
    app: AppHandle,
    tx: Sender<Audio>,
    lost: Sender<Control>,
    last_frame: Arc<std::sync::atomic::AtomicU64>,
    t0: Instant,
) -> Result<(cpal::Stream, Input), String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or_else(|| "No microphone found. Connect one and try again.".to_string())?;
    let description = device.description().ok();
    let name = description
        .as_ref()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|| "the default microphone".to_string());
    let bluetooth = description
        .as_ref()
        .is_some_and(|d| d.interface_type() == cpal::InterfaceType::Bluetooth);
    let supported = device.default_input_config().map_err(mic_error)?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let input = Input {
        rate: config.sample_rate,
        channels: config.channels,
        narrowband: bluetooth || config.sample_rate <= SAMPLE_RATE,
        name,
    };
    let on_error = move |e: cpal::Error| {
        if needs_reopen(e.kind()) {
            let _ = lost.send(Control::Lost);
        } else if e.kind() == cpal::ErrorKind::PermissionDenied {
            let _ = app.emit("record-error", mic_error(e));
        }
    };
    macro_rules! build {
        ($t:ty) => {{
            let (tx, last_frame) = (tx.clone(), last_frame.clone());
            device.build_input_stream::<$t, _, _>(
                config.clone(),
                move |data: &[$t], _| {
                    last_frame.store(ms_since(t0), Ordering::Relaxed);
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
    Ok((stream, input))
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

/// The name of the system's current default microphone.
fn default_input_name() -> Option<String> {
    let device = cpal::default_host().default_input_device()?;
    device.description().ok().map(|d| d.name().to_string())
}

/// Tell the UI which microphone is recording.
fn announce_device(app: &AppHandle, name: &str) {
    let _ = app.emit("record-device", name);
}

fn status(app: &AppHandle, message: Option<String>, transient: bool) {
    let _ = app.emit("record-status", Status { message, transient });
}

/// Capture thread: owns the stream (not `Send` on every platform), turns
/// control messages into stream pause/play plus markers in the audio queue,
/// and reopens the default microphone when the device fails or goes quiet.
fn capture_thread(
    app: AppHandle,
    tx: Sender<Audio>,
    control_tx: Sender<Control>,
    control: Receiver<Control>,
    ready: Sender<Result<Input, String>>,
) {
    let t0 = Instant::now();
    let last_frame = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let open = || open_input(app.clone(), tx.clone(), control_tx.clone(), last_frame.clone(), t0);
    let start = |s: &cpal::Stream| {
        last_frame.store(ms_since(t0), Ordering::Relaxed);
        s.play().map_err(mic_error)
    };
    let (mut stream, mut device_name) =
        match open().and_then(|(s, input)| start(&s).map(|()| (s, input))) {
            Ok((stream, input)) => {
                let name = input.name.clone();
                let _ = ready.send(Ok(input));
                announce_device(&app, &name);
                (Some(stream), name)
            }
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
    let mut paused = false;
    // Every reconnection attempt failed; a new default microphone can still recover.
    let mut gave_up = false;
    loop {
        let msg = match control.recv_timeout(Duration::from_secs(1)) {
            Ok(msg) => msg,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let quiet = ms_since(t0).saturating_sub(last_frame.load(Ordering::Relaxed));
                if !paused && stream.is_some() && quiet >= STALL.as_millis() as u64 {
                    Control::Lost
                } else if (stream.is_some() || gave_up)
                    && default_input_name().is_some_and(|name| name != device_name)
                {
                    Control::Switch
                } else {
                    continue;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match msg {
            Control::Pause => {
                paused = true;
                if let Some(s) = &stream {
                    let _ = s.pause();
                }
                let _ = tx.send(Audio::Pause);
            }
            Control::Resume => {
                paused = false;
                let _ = tx.send(Audio::Resume);
                if let Some(s) = &stream {
                    let _ = s.play();
                    last_frame.store(ms_since(t0), Ordering::Relaxed);
                }
            }
            Control::Stop => break,
            Control::Switch => {
                // Stop the old device first, so its last frames are queued before
                // the new format is announced.
                stream = None;
                let switched_at = Instant::now();
                match open() {
                    Ok((s, input)) => {
                        let gap = if paused { 0.0 } else { switched_at.elapsed().as_secs_f64() };
                        device_name = input.name.clone();
                        let _ = tx.send(Audio::Reopened(input, gap));
                        if !paused && start(&s).is_err() {
                            let _ = control_tx.send(Control::Lost);
                        }
                        stream = Some(s);
                        gave_up = false;
                        announce_device(&app, &device_name);
                        status(&app, Some(format!("Switched to {device_name}.")), true);
                    }
                    // The new default can't be opened: treat it like a lost device.
                    Err(_) => {
                        let _ = control_tx.send(Control::Lost);
                    }
                }
            }
            Control::Lost => {
                if gave_up {
                    continue;
                }
                stream = None;
                // Audio stopped arriving at the last callback.
                let lost_at = t0 + Duration::from_millis(last_frame.load(Ordering::Relaxed));
                let mut outage = OutageClock::start(lost_at, paused);
                status(&app, Some("Microphone disconnected — reconnecting…".into()), false);
                let mut stop = false;
                for delay in RETRY_DELAYS {
                    match control.recv_timeout(Duration::from_secs(delay)) {
                        Ok(Control::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                            stop = true;
                            break;
                        }
                        // Pause/Resume still reach the processor, so it drops or keeps
                        // frames correctly once a device is back.
                        Ok(Control::Pause) => {
                            paused = true;
                            outage.pause(Instant::now());
                            let _ = tx.send(Audio::Pause);
                        }
                        Ok(Control::Resume) => {
                            paused = false;
                            outage.resume(Instant::now());
                            let _ = tx.send(Audio::Resume);
                        }
                        _ => {}
                    }
                    if let Ok((s, input)) = open() {
                        // Announce the new format before any of its frames are queued.
                        let name = input.name.clone();
                        device_name = name.clone();
                        let now = Instant::now();
                        let _ = tx.send(Audio::Reopened(input, outage.gap(now).as_secs_f64()));
                        // That gap is now in the recording; count afresh if this attempt fails.
                        outage = OutageClock::start(now, paused);
                        if !paused && start(&s).is_err() {
                            continue;
                        }
                        announce_device(&app, &name);
                        status(&app, Some(format!("Reconnected to {name}.")), true);
                        gave_up = false;
                        stream = Some(s);
                        break;
                    }
                }
                if stop {
                    break;
                }
                if stream.is_none() {
                    gave_up = true;
                    status(&app, None, false);
                    let _ = app.emit(
                        "record-error",
                        "The microphone could not be reopened. Press Stop to keep what was recorded.",
                    );
                }
            }
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
    input: Input,
    audio: Receiver<Audio>,
    utterances: Sender<Utterance>,
    backlog: Arc<AtomicUsize>,
) -> Result<(), String> {
    let vad = engine.vad(VadProfile::LIVE).map_err(|e| e.to_string())?;
    let mut resampler = StreamResampler::new(input.rate, input.channels).map_err(|e| e.to_string())?;
    let mut p = Processor {
        app,
        file,
        vad,
        utterances,
        backlog,
        base: 0,
        since_level: 0,
        level_acc: 0.0,
        silence: SilenceWatch::new(3 * SAMPLE_RATE as usize),
        warnings: Warnings { silent: false, narrowband: input.narrowband },
    };
    p.emit_warnings();
    // Frames are dropped here while paused, even if the device ignores pause.
    let mut paused = false;
    while let Ok(msg) = audio.recv() {
        match msg {
            Audio::Frames(_) if paused => {}
            Audio::Frames(frames) => {
                let out = resampler.push(&frames).map_err(|e| e.to_string())?;
                p.samples(&out)?;
            }
            Audio::Pause => {
                paused = true;
                p.close_utterance();
                p.file.flush()?;
                let change = p.silence.reset();
                p.set_silent(change);
            }
            Audio::Resume => {
                paused = false;
                p.restart_vad();
            }
            Audio::Reopened(input, gap_s) => {
                let tail = resampler.finish().map_err(|e| e.to_string())?;
                p.samples(&tail)?;
                p.close_utterance();
                p.file.append_silence(gap_s)?;
                p.restart_vad();
                let change = p.silence.reset();
                p.set_silent(change);
                resampler = StreamResampler::new(input.rate, input.channels).map_err(|e| e.to_string())?;
                if p.warnings.narrowband != input.narrowband {
                    p.warnings.narrowband = input.narrowband;
                    p.emit_warnings();
                }
            }
            Audio::End => break,
        }
    }
    let tail = resampler.finish().map_err(|e| e.to_string())?;
    p.samples(&tail)?;
    p.close_utterance();
    p.file.finish()
}

struct Processor {
    app: AppHandle,
    file: RecordingFile,
    vad: engine::Vad,
    utterances: Sender<Utterance>,
    /// Samples queued for the decoder and not yet transcribed.
    backlog: Arc<AtomicUsize>,
    /// VAD positions restart after a pause; `base` maps them back onto the WAV.
    base: usize,
    since_level: usize,
    level_acc: f64,
    silence: SilenceWatch,
    warnings: Warnings,
}

impl Processor {
    fn send(&self, closed: Vec<(usize, Vec<f32>)>) {
        for (start, chunk) in closed {
            let start = self.base + start;
            let queued = self.backlog.fetch_add(chunk.len(), Ordering::Relaxed);
            let utt = if queued + chunk.len() > MAX_QUEUED {
                Utterance::Span(start, chunk.len())
            } else {
                Utterance::Samples(start, chunk)
            };
            let _ = self.utterances.send(utt);
        }
    }

    fn close_utterance(&mut self) {
        let closed = self.vad.flush();
        self.send(closed);
    }

    fn restart_vad(&mut self) {
        self.vad.reset();
        self.base = self.file.written();
    }

    fn emit_warnings(&self) {
        let _ = self.app.emit("record-warnings", &self.warnings);
    }

    fn set_silent(&mut self, change: Option<bool>) {
        if let Some(silent) = change {
            self.warnings.silent = silent;
            self.emit_warnings();
        }
    }

    fn samples(&mut self, samples: &[f32]) -> Result<(), String> {
        self.file.append(samples)?;
        let closed = self.vad.accept(samples);
        self.send(closed);
        let change = self.silence.feed(samples);
        self.set_silent(change);
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
                    backlog_s: self.backlog.load(Ordering::Relaxed) as f64 / SAMPLE_RATE as f64,
                },
            );
            self.since_level = 0;
            self.level_acc = 0.0;
        }
        Ok(())
    }
}

/// Decoder thread: transcribe utterances in order and emit each one. After
/// Stop it reports progress through the remaining backlog.
#[allow(clippy::too_many_arguments)]
fn decode_thread(
    app: AppHandle,
    engine: SherpaEngine,
    path: PathBuf,
    utterances: Receiver<Utterance>,
    segments: Arc<Mutex<Vec<AsrSegment>>>,
    mut recognizer: engine::Recognizer,
    backlog: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
) {
    let mut built = Instant::now();
    // Samples transcribed since Stop, for the drain progress bar.
    let mut drained = 0usize;
    while let Ok(utt) = utterances.recv() {
        let len = utt.len();
        if !cancel.load(Ordering::Relaxed) {
            let decoded = match utt {
                Utterance::Samples(start, chunk) => recognizer.decode_utterance(start, &chunk),
                Utterance::Span(start, n) => read_span(&path, start, n)
                    .ok()
                    .and_then(|chunk| recognizer.decode_utterance(start, &chunk)),
            };
            if let Some(seg) = decoded {
                let _ = app.emit("record-utterance", &seg);
                segments.lock().unwrap().push(seg);
            }
        }
        let remaining = backlog.fetch_sub(len, Ordering::Relaxed).saturating_sub(len);
        if stopping.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
            drained += len;
            let total = drained + remaining;
            let _ = app.emit(
                "transcribe-progress",
                engine::Progress::new(
                    "transcribing",
                    (drained / SAMPLE_RATE as usize) as u32,
                    (total / SAMPLE_RATE as usize) as u32,
                    0.9 * drained as f32 / total.max(1) as f32,
                ),
            );
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
) -> Result<StartedRecording, String> {
    let library = crate::library::library_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<RecorderState>();
        if state.0.lock().unwrap().is_some() {
            return Err("A recording is already in progress.".to_string());
        }
        let mut cfg = EngineConfig::new("");
        cfg.asr_model = crate::commands::parse_asr_model(model.as_deref());
        cfg.models_dir = crate::commands::resolve_models_dir(&app, cfg.asr_model, false)?;
        let engine = SherpaEngine::new(&cfg).map_err(|e| e.to_string())?;
        let recognizer = engine.recognizer().map_err(|e| e.to_string())?;

        let (audio_tx, audio_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (capture_app, capture_control) = (app.clone(), control_tx.clone());
        let capture = std::thread::spawn(move || {
            capture_thread(capture_app, audio_tx, capture_control, control_rx, ready_tx)
        });
        let input = ready_rx
            .recv()
            .map_err(|_| "The microphone thread stopped unexpectedly.".to_string())??;

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
        let cancel = Arc::new(AtomicBool::new(false));
        let stopping = Arc::new(AtomicBool::new(false));
        let backlog = Arc::new(AtomicUsize::new(0));
        let decode = {
            let (app, path, segments) = (app.clone(), path.clone(), segments.clone());
            let (backlog, cancel, stopping) = (backlog.clone(), cancel.clone(), stopping.clone());
            std::thread::spawn(move || {
                decode_thread(
                    app, decode_engine, path, utt_rx, segments, recognizer, backlog, cancel,
                    stopping,
                )
            })
        };
        let process_app = app.clone();
        let process = std::thread::spawn(move || {
            let result = process_thread(
                process_app.clone(),
                engine,
                file,
                input,
                audio_rx,
                utt_tx,
                backlog,
            );
            if let Err(e) = &result {
                // Report at once: capture may still be running, but nothing more is saved.
                let _ = process_app.emit(
                    "record-error",
                    format!(
                        "Recording stopped saving: {e}. What was recorded so far is kept; press Stop to create the transcript."
                    ),
                );
            }
            result
        });

        *state.0.lock().unwrap() = Some(Active {
            path: path.clone(),
            cfg,
            control: control_tx,
            capture,
            process,
            decode,
            segments,
            cancel,
            stopping,
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
    cfg: EngineConfig,
    segments: Vec<AsrSegment>,
}

/// Stop capture and wait for the worker threads. Always returns the recording
/// (the WAV stays readable up to its last header update even if writing
/// failed); a processing failure was already reported to the UI as it
/// happened. `discarding` skips transcription of any queued utterances.
fn finish(app: &AppHandle, discarding: bool) -> Result<Finished, String> {
    let active = app
        .state::<RecorderState>()
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| "No recording is in progress.".to_string())?;
    if discarding {
        active.cancel.store(true, Ordering::Relaxed);
    } else {
        active.stopping.store(true, Ordering::Relaxed);
    }
    let _ = active.control.send(Control::Stop);
    let _ = active.capture.join();
    if let Ok(Err(e)) | Err(e) = active.process.join().map_err(|_| "panicked".to_string()) {
        eprintln!("recording processing failed: {e}");
    }
    let _ = active.decode.join();
    let mut segments = active.segments.lock().unwrap().clone();
    segments.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    Ok(Finished { path: active.path, cfg: active.cfg, segments })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoppedRecording {
    transcript: engine::Transcript,
    /// Whether the transcript carries speaker labels.
    speakers_labelled: bool,
    /// Something the user should know about the result.
    note: Option<String>,
}

/// Stop recording and build the transcript: the live utterances, with speaker
/// labels from diarizing the whole recording if `diarize` is set. If labelling
/// fails, the transcript is returned without labels and with a note.
#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    diarize: Option<bool>,
    num_speakers: Option<u32>,
) -> Result<StoppedRecording, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let done = finish(&app, false)?;
        let mut result = StoppedRecording {
            transcript: merge::from_asr(&done.segments),
            speakers_labelled: false,
            note: None,
        };
        if diarize.unwrap_or(false) {
            let _ = app.emit("transcribe-progress", engine::Progress::new("diarizing", 0, 1, 0.95));
            match label_speakers(&app, &done, num_speakers) {
                Ok(transcript) => {
                    result.transcript = transcript;
                    result.speakers_labelled = true;
                }
                Err(e) => {
                    result.note = Some(format!(
                        "Speakers couldn't be labelled, so this transcript has none. ({e})"
                    ))
                }
            }
        }
        let _ = app.emit("transcribe-progress", engine::Progress::new("done", 1, 1, 1.0));
        // The notes file stays until the app has saved the transcript with
        // the notes (`forget_recording_notes`).
        Ok(result)
    })
    .await
    .map_err(|e| format!("recording task failed: {e}"))?
}

/// Diarize the whole recording and attach speakers to the live utterances.
fn label_speakers(
    app: &AppHandle,
    done: &Finished,
    num_speakers: Option<u32>,
) -> Result<engine::Transcript, String> {
    let mut cfg = done.cfg.clone();
    cfg.diarize = true;
    cfg.models_dir = crate::commands::resolve_models_dir(app, cfg.asr_model, true)?;
    let (samples, sr) = load_wav_16k_mono(&done.path).map_err(|e| e.to_string())?;
    let eng = SherpaEngine::new(&cfg).map_err(|e| e.to_string())?;
    let spans = eng.diarize(&samples, sr, num_speakers).map_err(|e| e.to_string())?;
    Ok(merge::merge(&done.segments, &spans))
}

/// Remove a finished recording's notes file once the transcript holding the
/// notes is saved.
#[tauri::command]
pub async fn forget_recording_notes(app: AppHandle, audio_path: String) -> Result<(), String> {
    let library = crate::library::library_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        remove_notes(&own_recording(&library, Path::new(&audio_path))?)
    })
    .await
    .map_err(|e| format!("recording task failed: {e}"))?
}

/// Save the notes typed during the current recording (kept next to the WAV
/// until Stop, so a crash doesn't lose them).
#[tauri::command]
pub async fn save_recording_notes(app: AppHandle, text: String) -> Result<(), String> {
    let library = crate::library::library_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        // Hold the lock while writing: Stop takes the recording under the same
        // lock, so a save can't land after Stop has removed the notes file.
        let state = app.state::<RecorderState>();
        let guard = state.0.lock().unwrap();
        let active = guard.as_ref().ok_or_else(|| "No recording is in progress.".to_string())?;
        save_recording_notes_in(&library, &active.path, &text)
    })
    .await
    .map_err(|e| format!("recording task failed: {e}"))?
}

/// Stop recording and delete the audio. Nothing is saved.
#[tauri::command]
pub async fn discard_recording(app: AppHandle) -> Result<(), String> {
    let library = crate::library::library_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let done = finish(&app, true)?;
        discard_recording_in(&library, &done.path)
    })
    .await
    .map_err(|e| format!("recording task failed: {e}"))?
}

/// On app exit, finish the WAV so the audio is kept intact. No transcript is
/// created; the recording stays in the recordings folder.
pub(crate) fn finish_on_exit(app: &AppHandle) {
    let _ = finish(app, false);
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

        let report = system::delete_transcript_authorized_in(&library, "s1", |_, _| true).unwrap();

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

    #[test]
    fn deleting_a_transcript_of_a_recording_also_deletes_notes_left_by_a_crash() {
        let library = unique_tmp("delete-notes");
        let (file, path) = RecordingFile::create_in(&library, "Recording test").unwrap();
        file.finish().unwrap();
        save_recording_notes_in(&library, &path, "Client agreed to mediation.").unwrap();
        crate::library::save_session_in(
            &library,
            &serde_json::json!({ "id": "s1", "audioPath": path.to_string_lossy() }),
        )
        .unwrap();

        let report = system::delete_transcript_authorized_in(&library, "s1", |_, _| true).unwrap();

        assert!(report.failed.is_empty(), "{:?}", report.failed);
        let left: Vec<_> = std::fs::read_dir(recordings_dir(&library)).unwrap().collect();
        assert!(left.is_empty(), "{left:?}");
        let _ = std::fs::remove_dir_all(&library);
    }

    #[test]
    fn notes_are_kept_next_to_the_recording_and_discarding_deletes_both() {
        let library = unique_tmp("notes");
        let (file, path) = RecordingFile::create_in(&library, "Recording test").unwrap();
        file.finish().unwrap();

        save_recording_notes_in(&library, &path, "first draft").unwrap();
        save_recording_notes_in(&library, &path, "Client agreed to mediation.").unwrap();
        let notes = recordings_dir(&library).join("Recording test.notes.txt");
        assert_eq!(std::fs::read_to_string(&notes).unwrap(), "Client agreed to mediation.");

        discard_recording_in(&library, &path).unwrap();

        let left: Vec<_> = std::fs::read_dir(recordings_dir(&library)).unwrap().collect();
        assert!(left.is_empty(), "{left:?}");
        let _ = std::fs::remove_dir_all(&library);
    }

    fn tone(n: usize) -> Vec<f32> {
        (0..n).map(|i| (i as f32 * 0.05).sin() * 0.5).collect()
    }

    #[test]
    fn silence_for_a_microphone_gap_keeps_the_recording_aligned_with_the_clock() {
        let library = unique_tmp("gap");
        let (mut file, path) = RecordingFile::create_in(&library, "Recording test").unwrap();
        let first = tone(16_000);
        file.append(&first).unwrap();
        file.append_silence(2.5).unwrap();
        file.append(&first).unwrap();
        file.finish().unwrap();

        let (samples, _) = load_wav_16k_mono(&path).unwrap();
        assert_eq!(samples.len(), 72_000);
        assert!(samples[16_000..56_000].iter().all(|&s| s == 0.0));
        for (a, b) in samples[56_000..].iter().zip(&first) {
            assert!((a - b).abs() < 1e-4);
        }
        let _ = std::fs::remove_dir_all(&library);
    }

    #[test]
    fn read_span_returns_the_samples_written_there_from_an_unfinished_recording() {
        let library = unique_tmp("span");
        let (mut file, path) = RecordingFile::create_in(&library, "Recording test").unwrap();
        let written = ramp(3 * 16_000);
        file.append(&written).unwrap();
        file.flush().unwrap();
        std::mem::forget(file);

        let span = read_span(&path, 16_000, 8_000).unwrap();

        assert_eq!(span.len(), 8_000);
        for (a, b) in span.iter().zip(&written[16_000..24_000]) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
        let _ = std::fs::remove_dir_all(&library);
    }

    #[test]
    fn digital_silence_warns_once_and_clears_when_sound_returns() {
        let mut watch = SilenceWatch::new(3 * 16_000);
        let zeros = vec![0.0f32; 1_600];
        let changes: Vec<bool> = (0..40).filter_map(|_| watch.feed(&zeros)).collect();
        assert_eq!(changes, vec![true]); // 4 s of zeros: one warning

        assert_eq!(watch.feed(&tone(1_600)), Some(false));
    }

    #[test]
    fn quiet_room_noise_is_not_mistaken_for_a_silent_microphone() {
        let mut watch = SilenceWatch::new(3 * 16_000);
        // About -60 dBFS of noise for 10 s.
        let noise: Vec<f32> = (0..1_600).map(|i| if i % 2 == 0 { 0.001 } else { -0.001 }).collect();
        for _ in 0..100 {
            assert_eq!(watch.feed(&noise), None);
        }
    }

    #[test]
    fn a_reconnection_gap_counts_only_time_spent_recording() {
        let t = Instant::now();
        let at = |secs: u64| t + Duration::from_secs(secs);

        // Lost while recording; paused 2 s later; resumed 10 s after that; back 1 s later.
        let mut clock = OutageClock::start(at(0), false);
        clock.pause(at(2));
        clock.resume(at(12));
        assert_eq!(clock.gap(at(13)), Duration::from_secs(3));

        // Lost while paused; resumed after 5 s; back 1 s later.
        let mut clock = OutageClock::start(at(0), true);
        clock.resume(at(5));
        assert_eq!(clock.gap(at(6)), Duration::from_secs(1));
    }
}
