# engine

In-process, **CPU-only** ASR + speaker diarization, with **no Python at runtime**.
A thin Rust adapter over the official **k2-fsa [`sherpa-onnx`](https://crates.io/crates/sherpa-onnx)**
crate (Apache-2.0) that emits the project's JSON transcript contract
(`core/src/types.ts`):

```json
{ "segments": [ { "start": 0.9, "end": 3.3, "text": "...", "speaker": "SPEAKER_00",
                  "words": [ { "word": "and", "start": 0.9, "end": 1.3, "speaker": "SPEAKER_00" } ] } ] }
```

It is the production engine linked into the app (`app/src-tauri`). It uses the
official crate, **not** the deprecated `sherpa-rs`.

## Pipeline

1. **ASR** — via `OfflineRecognizer`: NVIDIA **Parakeet-TDT-0.6B-v2** int8 (the default;
   NeMo transducer, English, native word timestamps) or Whisper int8 (`small.en`,
   `medium.en`). Audio is chunked by **Silero VAD** (threshold 0.5, min_speech 0.25 s;
   min_silence / max_speech = 0.8 s / 120 s for Parakeet, 0.25 s / 25 s for Whisper to
   stay under its 30 s window). Whisper word timing comes from per-token timestamps
   grouped on the leading-space / `Ġ` marker, with a fallback that spreads the chunk span
   uniformly across words (see "Token timestamps" below).
2. **Diarization** (opt-in: `EngineConfig.diarize`, default `false`) — `OfflineSpeakerDiarization`: pyannote segmentation-3.0 +
   3D-Speaker (CAM++) embedding, `FastClustering` (threshold 0.5; fixed `num_clusters`
   when `num_speakers` is set, else `-1` = auto), `min_duration_on 0.3`, `min_duration_off 0.5`.
   Speakers labeled `SPEAKER_{nn:02}`.
3. **Merge** — each word gets the speaker of the diarized span it overlaps most
   (nearest-by-midpoint fallback; `UNKNOWN` if there are no spans). Segment speaker =
   duration-weighted majority of its words. Identical logic to `core`'s `assignSpeakers`.
   Without diarization, `merge::from_asr` builds a speaker-less transcript.

## Public API (`src/lib.rs`)

- `Word`, `Segment`, `Transcript` — serde shapes mirroring `core/src/types.ts`.
- `SpeakerSpan`, `AsrSegment` — intermediate types.
- `EngineConfig { models_dir, asr_model: AsrModel(SmallEn|MediumEn|ParakeetV2), num_threads, num_speakers, diarize }`
  — defaults: `ParakeetV2`, `num_threads = 4`, `num_speakers = None` (auto), `diarize = false`.
- `required_model_files(asr_model, diarize)` — the relative model paths a configuration
  needs (shared with the app's verified-pack resolver).
- `trait TranscriptionEngine { transcribe(.., progress); diarize(.., num_speakers); }`
- `SherpaEngine` — the concrete impl (`SherpaEngine::new(&cfg)` validates model files exist).
- `process(engine, samples, sample_rate, &cfg, progress) -> Transcript` — transcribe, then
  diarize + merge when `cfg.diarize`.
- `Progress` / `ProgressSink` — staged progress events (`PROGRESS_*` fraction bands).
- `decode_to_16k_mono[_with_progress](path)` — any supported audio (symphonia) → 16 kHz mono `f32`.
- `load_wav_16k_mono(path) -> (Vec<f32>, u32)` — 16 kHz mono 16-bit PCM, normalized `i16/32768`.

## Build

```sh
cargo build
```

The `sherpa-onnx` crate (via `sherpa-onnx-sys`) statically links prebuilt C
libraries (the explicitly enabled `static` feature). On the **first** build the sys crate's
build script **downloads** the platform archive from
`https://github.com/k2-fsa/sherpa-onnx/releases/download/v{VERSION}/...`.
This is a one-time **build-time** dev fetch — not client/runtime egress.

### Offline / pinned build

The build script reads, in order:

| Env var | Effect |
| --- | --- |
| `SHERPA_ONNX_LIB_DIR` | Use prebuilt libs in this dir; **no download**. |
| `SHERPA_ONNX_ARCHIVE_DIR` | Use a pre-downloaded `.tar.bz2` archive from this dir; no download. |

For a provably-offline build, vendor the libs and point at them, e.g.:

```sh
export SHERPA_ONNX_LIB_DIR=/abs/path/to/vendored/sherpa-onnx/lib
cargo build --offline
```

The network-crate gate runs in CI (job `gate` in `.github/workflows/ci.yml`); CI does
not yet vendor the libs, so CI builds perform the one-time build-time fetch.

### Windows-CPU target

The crate publishes Windows static archives (`...-win-x64-static-lib`), so the same
`SHERPA_ONNX_LIB_DIR` vendoring path applies. CI's `windows` job builds and tests the
engine and the app backend with MSVC static linking.

## Tests

Unit tests run with plain `cargo test` (CI runs them on Linux and Windows). The
end-to-end integration test is `#[ignore]`d because it needs local models: it fails if
`../models/slice_120_420.wav` or a required small.en / diarization model file is missing.
Run it explicitly:

```sh
cargo test -- --include-ignored --nocapture
```

It loads the 5-min slice, runs `process` with `small.en`, diarization on and
`num_speakers=Some(3)`, and asserts: ≥1 segment; every word has a speaker and exactly 3
speakers are found; word/segment starts non-decreasing; and RTF < 1.0 (a sanity gate, not a
benchmark). Measured RTF on an Apple Silicon Mac was ~0.20 (an earlier Python sherpa-onnx run of
small.en measured 0.109; the gap is partly the timestamp fallback, see below).

## Token timestamps

Parakeet emits native word timestamps; this section applies to Whisper only. The
downloaded `*-decoder.int8.onnx` Whisper models were exported **without
cross-attention outputs**, so sherpa-onnx cannot emit per-token timestamps
(`enable_token_timestamps=true` logs a warning and returns none). The engine therefore
uses a **fallback**: spread each VAD chunk's span uniformly across
its words. Word timestamps are a **playback aid only** (±200–500 ms), so this
is acceptable. To get native token timestamps later, re-export the decoder with attention
outputs (`scripts/whisper/export-onnx-with-attention.py` in the sherpa-onnx repo); the
grouping path is already implemented and will activate automatically when timestamps are present.
