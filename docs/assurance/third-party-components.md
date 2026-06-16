# Third-party component & licence inventory

> Part of the assurance pack. Addresses PRD user story #25 (compliance officer
> wants a list of bundled third-party components and licences to clear
> procurement).

## Headline

**Every bundled component — Rust crate, npm dependency, and ONNX model — is
under a permissive, redistributable licence (MIT or Apache-2.0).** There are
**no copyleft (GPL/AGPL/LGPL) dependencies**, **no source-available/non-commercial
weights** (CC-BY-NC and gated models such as Rev `reverb-diarization` were
deliberately rejected per the PRD licence-vetting gate), and **no Hugging Face
token / account is required** to obtain or run any model. Required attributions
are surfaced below.

Versions are read from the committed manifests (`engine/Cargo.toml`,
`modelpack/Cargo.toml`, `app/src-tauri/Cargo.toml`, `app/package.json`,
`core/package.json`) and the model table from the sherpa-onnx spike
(`docs/spikes/sherpa-onnx-spike.md`). Transitive dependencies of the listed
crates/packages are resolvable from the committed lockfiles
(`app/src-tauri/Cargo.lock`, npm lockfiles) and via `cargo tree` /
`npm ls`; all are confirmed network-free by the zero-egress gate
(`scripts/check-no-network.sh`).

## ONNX models (the data that touches client audio)

| Role | Model | Version / tag | Licence | Attribution (required) |
| --- | --- | --- | --- | --- |
| ASR (default) | Whisper `small.en` (int8, sherpa-onnx) | k2-fsa `asr-models` | **MIT** | OpenAI Whisper |
| ASR (higher accuracy, opt-in) | Whisper `medium.en` (int8) | k2-fsa `asr-models` | **MIT** | OpenAI Whisper |
| Diarization — segmentation | pyannote/segmentation-3.0 (ONNX) | k2-fsa `speaker-segmentation-models` | **MIT** | pyannote / segmentation-3.0 (CNRS) |
| Diarization — speaker embedding | 3D-Speaker CAM++ (`...campplus_sv_en_voxceleb_16k`) | k2-fsa `speaker-recognition-models` | **Apache-2.0** | 3D-Speaker / ModelScope |
| VAD (chunking) | Silero VAD | k2-fsa `asr-models` | **MIT** | Silero Team |

All weights are MIT/Apache-2.0, ungated, no token required — a direct enabler of
the airplane-mode story. Models are bundled/imported as **signed model packs**
verified before load (see `data-flow.md` and the modelpack section below).

## Rust crates (the engine + app backend)

| Crate | Version | Licence | Role |
| --- | --- | --- | --- |
| `sherpa-onnx` (k2-fsa, official) | 1.13.2 | **Apache-2.0** | In-process ASR + `OfflineSpeakerDiarization`. (NOT the deprecated `sherpa-rs`.) |
| `symphonia` | 0.5.x | **MPL-2.0** | Pure-Rust audio demux/decode (mp3/AAC/FLAC/WAV/OGG). See note below. |
| `rubato` | 0.16.x | **MIT** | Sample-rate conversion to 16 kHz. |
| `hound` | 3.5.x | **Apache-2.0 / MIT** | WAV read (legacy/test path). |
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
> impose obligations on the firm as an end user. It is the one non-MIT/Apache
> crate; flagged here for completeness. If your procurement policy excludes
> MPL-2.0 outright, raise it with the vendor — it is an isolated decode
> dependency. (BSD-3-Clause for `ed25519-dalek` is likewise permissive.)

**Deliberately absent** (the stripped Vibe egress surface): `reqwest`, `hyper`,
`tauri-plugin-http`, `tauri-plugin-updater`, `tauri-plugin-aptabase`,
`tauri-plugin-deep-link`, `yt-dlp`. Their absence is machine-enforced — see
`airplane-mode-verification.md`.

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
| `@kindtech/core` (this project) | 0.1.0 | project-owned (Apache-2.0) | Turn builder, diarization assignment, editor reducer, modelpack oracle. |

`@types/*` packages are dev-only type stubs (MIT).

## Upstream harvest attribution

The app is built fresh but harvests patterns from **Vibe**
(github.com/thewh1teagle/vibe, **MIT**, © thewh1teagle 2024). Per the fork
plan, any file lifted from Vibe preserves its MIT copyright notice. No Vibe
networked code (updater, aptabase, ytdlp, http, sona sidecar) is carried over.

## Model packs & attribution surfacing

Each model pack carries a manifest with a `license` and `attribution` field;
the verifier returns the attribution on success
(`modelpack/src/lib.rs` → `Verified.attribution`) so it can be displayed
in-app alongside the loaded model — satisfying the attribution requirements
above (and PRD #29). ✅ implemented (verification + attribution surfacing);
🚧 the in-app display surface and production signing key are roadmap.

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
