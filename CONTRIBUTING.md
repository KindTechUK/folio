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
| `docs/` | User docs and the assurance/compliance pack (the mkdocs site source). |
| `scripts/` | Dev/CI helpers: the zero-egress gate, model-pack release build, manual e2e harness. |
| `fixtures/` | Test-only model-pack fixtures and keys for the conformance tests. |

## Prerequisites

- [Rust](https://rustup.rs/) (stable) + Cargo.
- Node.js 22+ and npm.
- Tauri's platform deps for desktop builds — see the
  [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/). On Linux:
  `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `libgtk-3-dev`,
  `patchelf`.
- Optional: `ffmpeg`, only for the manual e2e harness (`scripts/e2e.sh`) with non-WAV
  input.

## Build & test (quickstart)

From the repo root:

```bash
(cd core && npm ci && npm run verify)                    # tsc --noEmit && vitest run
(cd app && npm ci && npm run build && npx vitest run)    # frontend (also builds app/dist)
cargo test --manifest-path app/src-tauri/Cargo.toml      # app backend (needs app/dist)
cargo test --manifest-path engine/Cargo.toml
cargo test --manifest-path modelpack/Cargo.toml
cargo test --manifest-path tools/modelpack-sign/Cargo.toml
(cd app && npm run tauri dev)                            # run the app
```

> The first `engine`/app build downloads the prebuilt `sherpa-onnx` C library at
> **build time** (a one-time fetch, overridable via `SHERPA_ONNX_LIB_DIR` — see
> `engine/README.md`). This is a build-time fetch, **not** runtime egress.

Release installers, the `VENDOR_PUBKEY_SPKI_PEM` requirement for release builds, and
per-component commands are in [Build from source](docs/build-from-source.md).

`scripts/e2e.sh` is a manual, model-gated end-to-end harness (it needs locally
downloaded models under `models/`); it does **not** run in CI.

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

# 3. Run the app trusting that key, then import the pack via the Models screen
(cd app && VENDOR_PUBKEY_SPKI_PEM="$(cat /tmp/keys/public.spki.pem)" npm run tauri dev)
```

See [Model packs](docs/model-packs.md) for the full pipeline.

## Pull requests

- Keep changes focused; match the style and comment density of the surrounding code.
- Run the relevant tests and `scripts/check-no-network.sh` before opening a PR.
- CI (`.github/workflows/ci.yml`, plus `docs.yml` for docs changes) must be green.

## License

By contributing, you agree that your contributions are licensed under the
[Apache License 2.0](LICENSE).
