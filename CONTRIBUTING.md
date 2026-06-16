# Contributing to Folio

Thanks for your interest in contributing. This project is a privacy-first,
**zero-egress**, CPU-only local transcription desktop app. The most important
invariant is that **no audio and no telemetry ever leaves the user's machine** —
please keep that in mind for every change.

## Repository layout

| Path | What it is |
|------|------------|
| `app/` | Tauri v2 desktop app — React 19 + Vite frontend (`app/src`) and the Rust backend (`app/src-tauri`). |
| `core/` | Framework-independent TypeScript: turn building, diarization assignment, the editor reducer, and model-pack validation. |
| `engine/` | Rust ASR + speaker diarization over `sherpa-onnx` (CPU-only, no Python at runtime). |
| `modelpack/` | Rust model-pack **verifier** (fail-closed: validate → sha256 → ed25519 → engine-version gate). Mirrors `core/src/modelpack.ts`. |
| `tools/modelpack-sign/` | Vendor-side **producer** CLI that creates signed model packs. Reuses `modelpack/` so signer and verifier never drift. Not shipped in the app. |
| `transcribe.py` | Legacy standalone Python WhisperX CLI (predates the Rust engine). Documented but not part of the released app. |
| `docs/` | Assurance/compliance, design, PRD, and spike docs (also the mkdocs site source). |
| `scripts/` | Dev/CI helpers, including the zero-egress gate. |

## Prerequisites

- [Rust](https://rustup.rs/) (stable) + Cargo.
- Node.js 20+ and npm.
- `ffmpeg` (audio decode for the Python CLI / e2e harness).
- Tauri's platform deps for desktop builds — see the
  [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/). On Linux:
  `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`.

## Build & test

Each component is independent. From the repo root:

```bash
# Shared TS core
cd core && npm ci && npm run verify          # tsc --noEmit && vitest run

# Frontend (also builds the dist that Tauri embeds)
cd app  && npm ci && npm run build && npx vitest run

# Rust crates
cargo test --manifest-path engine/Cargo.toml
cargo test --manifest-path modelpack/Cargo.toml
cargo test --manifest-path tools/modelpack-sign/Cargo.toml

# Desktop app (produces installers under app/src-tauri/target/release/bundle/)
cd app && npm run tauri build
```

> The first `engine`/app build downloads the prebuilt `sherpa-onnx` C library at
> **build time** (a one-time fetch, overridable via `SHERPA_ONNX_LIB_DIR` — see
> `engine/README.md`). This is a build-time fetch, **not** runtime egress.

## The zero-egress gate

The product's core guarantee is that the shipped binaries link **no** networking
crates. CI enforces this, and you can prove it locally:

```bash
bash scripts/check-no-network.sh
```

If you add a dependency that pulls in an HTTP client (reqwest/hyper/ureq/etc.) or a
Tauri network plugin, this gate fails by design. Don't add network capability to the
app or engine.

## Model packs

The app only loads **signed** model packs (`manifest.json` + `weights.bin`), verified
against a vendor public key embedded at build time. To experiment locally:

```bash
# 1. Generate a dev keypair
cargo run --manifest-path tools/modelpack-sign/Cargo.toml -- keygen --out-dir /tmp/keys

# 2. Build a signed pack from a directory of model files
cargo run --manifest-path tools/modelpack-sign/Cargo.toml -- pack \
  --model-dir <models> --out /tmp/pack --key /tmp/keys/private.pkcs8.pem \
  --id asr-whisper-small.en --version 1.0.0 \
  --engine sherpa-onnx --engine-min-version 1.13.2 \
  --task asr --license MIT --attribution "OpenAI Whisper small.en + Silero VAD"

# 3. Build the app trusting that key, then import the pack via the Models screen
VENDOR_PUBKEY_SPKI_PEM="$(cat /tmp/keys/public.spki.pem)" \
  cargo build --manifest-path app/src-tauri/Cargo.toml
```

See [docs](docs/index.md) → *Model packs* for the full pipeline.

## Pull requests

- Keep changes focused; match the style and comment density of the surrounding code.
- Run the relevant tests and `scripts/check-no-network.sh` before opening a PR.
- CI (`.github/workflows/ci.yml`) must be green.

## License

By contributing, you agree that your contributions are licensed under the
[Apache License 2.0](LICENSE).
