# engine

In-process, **CPU-only** ASR + speaker diarization, with **no Python at runtime**.
A thin Rust adapter over the official **k2-fsa [`sherpa-onnx`](https://crates.io/crates/sherpa-onnx)**
crate (Apache-2.0) that emits the project's JSON transcript contract
(`core/src/types.ts`):

```json
{ "segments": [ { "start": 0.9, "end": 3.3, "text": "...", "speaker": "SPEAKER_00",
                  "words": [ { "word": "and", "start": 0.9, "end": 1.3, "speaker": "SPEAKER_00" } ] } ] }
```

This is a faithful Rust port of the validated Python spike scripts in `../models/`
(`asr_spike.py`, `diar_spike.py`, `merge_spike.py`). It exists to **de-risk native
linking + library bundling** ahead of the Windows-CPU build — the single biggest
unknown for shipping a Rust binary. We are **not** the deprecated `sherpa-rs`.

## Pipeline

1. **ASR** — Whisper int8 (`small.en` default, `medium.en` available) via
   `OfflineRecognizer`, chunked by **Silero VAD** (threshold 0.5, min_silence/min_speech
   0.25 s, **max_speech 25 s** to stay under Whisper's 30 s window). Each chunk is decoded;
   word timing comes from per-token timestamps grouped on the leading-space / `Ġ` marker,
   with a fallback that spreads the chunk span uniformly across words (see "Token
   timestamps" below).
2. **Diarization** — `OfflineSpeakerDiarization`: pyannote segmentation-3.0 +
   3D-Speaker (CAM++) embedding, `FastClustering` (threshold 0.5; fixed `num_clusters`
   when `num_speakers` is set, else `-1` = auto), `min_duration_on 0.3`, `min_duration_off 0.5`.
   Speakers labeled `SPEAKER_{nn:02}`.
3. **Merge** — each word gets the speaker of the diarized span it overlaps most
   (nearest-by-midpoint fallback; `UNKNOWN` if there are no spans). Segment speaker =
   duration-weighted majority of its words. Identical logic to `core`'s `assignSpeakers`.

## Public API (`src/lib.rs`)

- `Word`, `Segment`, `Transcript` — serde shapes mirroring `core/src/types.ts`.
- `SpeakerSpan`, `AsrSegment` — intermediate types.
- `EngineConfig { models_dir, asr_model: AsrModel(SmallEn|MediumEn), num_threads, num_speakers }`
  — defaults: `SmallEn`, `num_threads = 4`, `num_speakers = None` (auto).
- `trait TranscriptionEngine { transcribe(..); diarize(.., num_speakers); }`
- `SherpaEngine` — the concrete impl (`SherpaEngine::new(&cfg)` validates model files exist).
- `process(engine, samples, sample_rate, &cfg) -> Transcript` — transcribe + diarize + merge.
- `load_wav_16k_mono(path) -> (Vec<f32>, u32)` — 16 kHz mono 16-bit PCM, normalized `i16/32768`.

## Build

```sh
cargo build
```

The `sherpa-onnx` crate (via `sherpa-onnx-sys`) statically links prebuilt C
libraries (the `static` default feature). On the **first** build the sys crate's
build script **downloads** the platform archive from
`https://github.com/k2-fsa/sherpa-onnx/releases/download/v{VERSION}/...`.
This is a one-time **build-time** dev fetch — not client/runtime egress.

### Offline / pinned build (required for the hardened Windows build later)

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

(Pinning this in `engine/.cargo/config.toml` + a CI `cargo tree` / network gate is
deferred to PR2.)

### Windows-CPU target

The crate publishes Windows static archives (`...-win-x64-static-lib`), so the same
`SHERPA_ONNX_LIB_DIR` vendoring path applies. **Actual Windows validation is deferred**
to a Windows box / CI — this host build (aarch64-apple-darwin) proves the API and the
contract, not the Windows toolchain.

## Tests

Unit tests (merge logic) run with plain `cargo test`. The end-to-end integration test
is `#[ignore]`d and **gated on model presence** — if `../models/slice_120_420.wav` or any
required model file is missing it prints a skip notice and passes. Run it explicitly:

```sh
cargo test -- --include-ignored --nocapture
```

It loads the 5-min slice, runs `process` with `small.en` + `num_speakers=Some(3)`, writes
`../out/engine_slice.json`, and asserts: ≥1 segment; every word has a non-null speaker in
the diarized set; word/segment starts non-decreasing; and RTF < 1.0 (a sanity gate, not a
benchmark). Measured RTF on this Mac was ~0.20 (Python spike small.en was 0.109 — same
ballpark; the gap is partly the timestamp fallback, see below).

## Token timestamps

The downloaded `*-decoder.int8.onnx` Whisper models were exported **without
cross-attention outputs**, so sherpa-onnx cannot emit per-token timestamps
(`enable_token_timestamps=true` logs a warning and returns none). The engine therefore
uses the spike's documented **fallback**: spread each VAD chunk's span uniformly across
its words. Per the PRD, word timestamps are a **playback aid only** (±200–500 ms), so this
is acceptable. To get native token timestamps later, re-export the decoder with attention
outputs (`scripts/whisper/export-onnx-with-attention.py` in the sherpa-onnx repo); the
grouping path is already implemented and will activate automatically when timestamps are present.
