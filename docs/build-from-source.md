# Build from source

## Prerequisites

- [Rust](https://rustup.rs/) (stable) + Cargo
- Node.js 20+ and npm
- Tauri's platform dependencies — see the
  [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/).
  On Debian/Ubuntu: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`,
  `libgtk-3-dev`, `patchelf`.

!!! note "sherpa-onnx native library"
    The first `engine`/app build downloads the prebuilt `sherpa-onnx` C library at
    **build time** (a one-time fetch, overridable with `SHERPA_ONNX_LIB_DIR` — see
    `engine/README.md`). This is a build-time fetch, **not** runtime egress.

## Components

| Path | Build | Test |
|------|-------|------|
| `core/` | (no build; TS source) | `npm ci && npm run verify` |
| `app/` frontend | `npm ci && npm run build` | `npx vitest run` |
| `app/src-tauri` | `cargo build --manifest-path app/src-tauri/Cargo.toml` | (covered via `app/` vitest) |
| `engine/` | `cargo build --manifest-path engine/Cargo.toml` | `cargo test --manifest-path engine/Cargo.toml` |
| `modelpack/` | `cargo build --manifest-path modelpack/Cargo.toml` | `cargo test --manifest-path modelpack/Cargo.toml` |
| `tools/modelpack-sign/` | `cargo build --manifest-path tools/modelpack-sign/Cargo.toml` | `cargo test --manifest-path tools/modelpack-sign/Cargo.toml` |

## Build the desktop app

```bash
cd app
npm ci
npm run tauri build
```

Installers land under `app/src-tauri/target/release/bundle/`:

- macOS → `dmg/*.dmg`
- Windows → `nsis/*.exe`
- Linux → `appimage/*.AppImage`, `deb/*.deb`

To build an app that trusts your own model-pack signing key, inject the public key:

```bash
VENDOR_PUBKEY_SPKI_PEM="$(cat keys/public.spki.pem)" npm run tauri build
```

See [Model packs](model-packs.md) for the signing side.

## The zero-egress gate

The product guarantee is that the shipped binaries link **no** networking crates. Prove
it locally:

```bash
bash scripts/check-no-network.sh
```

CI runs the same gate (`.github/workflows/ci.yml`). Don't add an HTTP client or a Tauri
network plugin to the app or engine.
