# Third-party component & licence inventory

> Part of the assurance pack. Lists the bundled third-party components and
> licences a compliance officer needs to clear procurement.

## Headline

**Every bundled component — Rust crate, npm dependency, and ONNX model — is
under a permissive, redistributable licence** (MIT, Apache-2.0, BSD-3-Clause, or
CC-BY-4.0 for the default ASR model; `symphonia` is MPL-2.0, see the note below).
There are **no copyleft (GPL/AGPL/LGPL) dependencies**, **no
source-available/non-commercial weights** (CC-BY-NC and gated models such as Rev
`reverb-diarization` were deliberately rejected during licence vetting), and **no
Hugging Face token / account is required** to obtain or run any model. Required
attributions are surfaced below.

Versions are read from the committed manifests (`engine/Cargo.toml`,
`modelpack/Cargo.toml`, `app/src-tauri/Cargo.toml`, `app/package.json`,
`core/package.json`) and the model table from the release pack build
(`scripts/build-model-packs.sh`, with upstream assets pinned by SHA-256 in
`scripts/modelpack-assets.sha256`). Transitive dependencies of the listed
crates/packages are resolvable from the committed lockfiles
(`app/src-tauri/Cargo.lock`, npm lockfiles) and via `cargo tree` /
`npm ls`; all are confirmed network-free by the zero-egress gate
(`scripts/check-no-network.sh`).

## ONNX models (the data that touches client audio)

| Role | Model | Version / tag | Licence | Attribution (required) |
| --- | --- | --- | --- | --- |
| ASR (default) | NVIDIA Parakeet-TDT-0.6B-v2 (int8, sherpa-onnx) | k2-fsa Hugging Face (`csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8`, pinned revision) | **CC-BY-4.0** | NVIDIA |
| ASR (alternative) | Whisper `small.en` (int8) | k2-fsa Hugging Face (`csukuangfj/sherpa-onnx-whisper-small.en`, pinned revision) | **MIT** | OpenAI Whisper |
| ASR (alternative, slower) | Whisper `medium.en` (int8) | k2-fsa Hugging Face (`csukuangfj/sherpa-onnx-whisper-medium.en`, pinned revision) | **MIT** | OpenAI Whisper |
| Diarization — segmentation | pyannote/segmentation-3.0 (ONNX) | k2-fsa Hugging Face (`csukuangfj/sherpa-onnx-pyannote-segmentation-3-0`, pinned revision) | **MIT** | pyannote / segmentation-3.0 (CNRS) |
| Diarization — speaker embedding | 3D-Speaker CAM++ (`...campplus_sv_en_voxceleb_16k`) | k2-fsa Hugging Face (`csukuangfj/speaker-embedding-models`, pinned revision) | **Apache-2.0** | 3D-Speaker / ModelScope |
| VAD (chunking) | Silero VAD | k2-fsa `asr-models` | **MIT** | Silero Team |

All weights are MIT, Apache-2.0, or CC-BY-4.0 (attribution required), ungated, no token required — a direct enabler of
the airplane-mode story. Models are bundled/imported as **signed model packs**
verified before load (see `data-flow.md` and the modelpack section below).

## Rust crates (the engine + app backend)

| Crate | Version | Licence | Role |
| --- | --- | --- | --- |
| `sherpa-onnx` (k2-fsa, official) | 1.13.2 | **Apache-2.0** | In-process ASR + `OfflineSpeakerDiarization`. (NOT the deprecated `sherpa-rs`.) |
| `symphonia` | 0.5.x | **MPL-2.0** | Pure-Rust audio demux/decode (mp3/AAC/FLAC/WAV/OGG). See note below. |
| `rubato` | 0.16.x | **MIT** | Sample-rate conversion to 16 kHz. |
| `hound` | 3.5.x | **Apache-2.0** | WAV read, and writing recordings made in the app. |
| `cpal` | 0.18.x | **Apache-2.0** | Microphone capture for the Record page (CoreAudio on macOS, WASAPI on Windows, ALSA on Linux). No network code. |
| `chrono` | 0.4.x | **Apache-2.0 / MIT** | Local date and time for recording file names. |
| `serde` / `serde_json` | 1.x | **Apache-2.0 / MIT** | Serialization. |
| `anyhow` | 1.x | **Apache-2.0 / MIT** | Error handling. |
| `thiserror` | 2.x | **Apache-2.0 / MIT** | Error types. |
| `tauri` / `tauri-build` | 2.x | **Apache-2.0 / MIT** | Desktop shell (protocol-asset feature only). |
| `tauri-plugin-dialog` | 2.x | **Apache-2.0 / MIT** | File open/save dialogs (scoped). |
| `tauri-plugin-fs` | 2.x | **Apache-2.0 / MIT** | Scoped local file read/write. |
| `ed25519-dalek` | 2.x | **BSD-3-Clause** | Model-pack signature verification. |
| `sha2` | 0.10.x | **Apache-2.0 / MIT** | Model-pack weights hashing. |
| `base64` | 0.22.x | **Apache-2.0 / MIT** | Signature decoding. |
| `zip` | 2.x | **MIT** | Extract verified model packs (a pack's `weights.bin` is a zip) into the per-user verified-models cache. |

> **`symphonia` licence note (MPL-2.0):** MPL-2.0 is a permissive, file-level
> copyleft that is redistributable in a commercial product and does **not**
> impose obligations on the firm as an end user. It is the only weak-copyleft
> crate; flagged here for completeness. If your procurement policy excludes
> MPL-2.0 outright, raise it with the vendor — it is an isolated decode
> dependency. (BSD-3-Clause for `ed25519-dalek` is likewise permissive.)

**Deliberately absent:** `reqwest`, `hyper`, `tauri-plugin-http`,
`tauri-plugin-updater`, `tauri-plugin-aptabase`, `tauri-plugin-deep-link`, and
any other crate on the network-crate denylist. Their absence is
machine-enforced — see `airplane-mode-verification.md`. No URL/YouTube ingestion
(e.g. `yt-dlp`) exists either.

**Not shipped to the firm:** the producer signing tool `tools/modelpack-sign`
(and its extra dev deps `clap`, `walkdir`, `rand_core`) is a **vendor-side**
crate used to build/sign model packs. It is a standalone crate, NOT linked into
the app or engine binary, so none of its dependencies reach the firm's install.

## npm / frontend dependencies (the review editor)

| Package | Version | Licence | Role |
| --- | --- | --- | --- |
| `react` / `react-dom` | ^19 | **MIT** | Review-editor UI. |
| `@tauri-apps/api` | ^2 | **Apache-2.0 / MIT** | Tauri IPC bridge. |
| `@tauri-apps/plugin-dialog` | ^2 | **Apache-2.0 / MIT** | File open/save. |
| `@tauri-apps/plugin-fs` | ^2 | **Apache-2.0 / MIT** | Scoped file I/O. |
| `docx` | ^9.7.1 | **MIT** | In-process DOCX export (no network). |
| `vite` / `typescript` / `vitest` / testing-library | dev only | **MIT / Apache-2.0** | Build + test tooling (not shipped). |
| `core/` (this project) | — | project-owned (Apache-2.0) | Turn builder, diarization assignment, editor reducer. Bundled from TS source via the `@core` alias, not an npm dependency. |

`@types/*` packages are dev-only type stubs (MIT).

## Model packs & attribution surfacing

Each model pack carries a manifest with a `license` and `attribution` field;
the verifier returns the attribution on success
(`modelpack/src/lib.rs` → `Verified.attribution`) so it can be displayed
in-app alongside the loaded model — satisfying the attribution requirements
above. ✅ implemented: the Models screen shows each pack's attribution.

## How to regenerate this inventory

```sh
cargo tree --manifest-path app/src-tauri/Cargo.toml -e normal
cargo tree --manifest-path engine/Cargo.toml -e normal
cargo tree --manifest-path modelpack/Cargo.toml -e normal
( cd app && npm ls --all )
( cd core && npm ls --all )
```

Licences for each crate are in its `Cargo.toml` `license` field / its
crates.io page; for npm packages, in each package's `LICENSE` / `package.json`.
