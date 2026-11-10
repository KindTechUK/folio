# Build from source

## Prerequisites

- [Rust](https://rustup.rs/) (stable) + Cargo
- Node.js 22+ and npm (20.19 is the minimum the frontend test tooling accepts)
- Tauri's platform dependencies — see the
  [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/).
  On Debian/Ubuntu: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`,
  `libgtk-3-dev`, `patchelf`.
- Optional: `ffmpeg`, only for the manual e2e harness (see [below](#manual-end-to-end-harness)).

!!! note "sherpa-onnx native library"
    The first `engine`/app build downloads the prebuilt `sherpa-onnx` C library at
    **build time** (a one-time fetch, overridable with `SHERPA_ONNX_LIB_DIR` or
    `SHERPA_ONNX_ARCHIVE_DIR` — see `engine/README.md`). This is a build-time fetch,
    **not** runtime egress.

## Components

Run each from the repo root:

| Path | Build | Test |
|------|-------|------|
| `core/` | (no build; TS source) | `(cd core && npm ci && npm run verify)` |
| `app/` frontend | `(cd app && npm ci && npm run build)` | `(cd app && npx vitest run)` |
| `app/src-tauri` | `cargo build --manifest-path app/src-tauri/Cargo.toml` | `cargo test --manifest-path app/src-tauri/Cargo.toml` |
| `engine/` | `cargo build --manifest-path engine/Cargo.toml` | `cargo test --manifest-path engine/Cargo.toml` |
| `modelpack/` | `cargo build --manifest-path modelpack/Cargo.toml` | `cargo test --manifest-path modelpack/Cargo.toml` |
| `tools/modelpack-sign/` | `cargo build --manifest-path tools/modelpack-sign/Cargo.toml` | `cargo test --manifest-path tools/modelpack-sign/Cargo.toml` + `bash scripts/test-modelpack-security.sh` |

`app/src-tauri` embeds the frontend at compile time, so build the frontend (`app/dist`)
before any `cargo build`/`cargo test` of it.

## Run the app (development)

```bash
(cd app && npm ci && npm run tauri dev)
```

A debug-only `KINDTECH_MODELS_DIR` env var points the transcribe path at a raw models
directory; release builds ignore it and load only verified signed packs.

## Build the desktop app

Release builds **require** `VENDOR_PUBKEY_SPKI_PEM` — the SPKI public key the app will
trust for model packs. `build.rs` fails a release build if it is missing, invalid, or
equal to the committed test key. For a local installer without your own key, build in
debug mode:

```bash
(cd app && npm ci && npm run tauri build -- --debug)
```

To build a release app that trusts your own model-pack signing key (see
[Model packs](model-packs.md) for the signing side):

```bash
(cd app && npm ci && VENDOR_PUBKEY_SPKI_PEM="$(cat ../keys/public.spki.pem)" \
  npm run tauri build -- --features hardened)
```

Installers land under `app/src-tauri/target/{release,debug}/bundle/`:

- macOS → `dmg/*.dmg` (built for the host architecture)
- Windows → `nsis/*.exe`
- Linux → `appimage/*.AppImage`, `deb/*.deb`

## The zero-egress gate

The product guarantee is that the shipped binaries link **no** networking crates. Prove
it locally:

```bash
bash scripts/check-no-network.sh
```

CI runs the same gate (job `gate` in `.github/workflows/ci.yml`). Don't add an HTTP
client or a Tauri network plugin to the app or engine.

## Manual end-to-end harness

`scripts/e2e.sh <audio> [--diarize] [--speakers N] [--model parakeet|small.en|medium.en]`
runs the engine example over a real recording and pushes the result through the `core` pipeline. It
needs locally downloaded models under `models/` (gitignored) and, for non-WAV input,
`ffmpeg`. It is a manual check and **does not run in CI**.
