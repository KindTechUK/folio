# Model packs

The app never bundles raw model weights. Instead it loads **signed model packs** and
verifies them **fail-closed** before use. This keeps the binary small, lets models be
updated independently, and gives IT/compliance a clear provenance trail.

## What a pack is

A pack is a directory of two files:

```
<pack-name>/
├── manifest.json   # metadata + sha256 + ed25519 signature
└── weights.bin     # a zip of the model files, laid out as the engine expects
```

`manifest.json` carries nine fields: `id`, `version`, `engine`, `engine_min_version`,
`task`, `license`, `attribution`, `sha256`, and `signature`.

## How verification works

When you import a pack (or at transcription time), the verifier checks, in order, and
**stops at the first failure**:

1. **Structure** — required fields, types, semver format, `sha256` is 64 lowercase hex.
2. **Integrity** — `sha256(weights.bin)` matches the manifest (constant-time compare).
3. **Signature** — an **ed25519** signature over the canonicalized manifest, verified
   against the **vendor public key compiled into the app**.
4. **Engine gate** — the pack's `engine_min_version` must be ≤ the app's engine version.

The verifier (`modelpack/`, Rust) is a byte-for-byte mirror of the TypeScript oracle
(`core/src/modelpack.ts`); a cross-language conformance test keeps them identical.

## The trust key

The app trusts exactly **one** vendor public key, injected at **compile time**:

```rust
// app/src-tauri/src/models.rs
fn vendor_pubkey_pem() -> &'static str {
    option_env!("VENDOR_PUBKEY_SPKI_PEM").unwrap_or(DEV_TEST_PUBKEY_SPKI_PEM)
}
```

The test-key fallback applies to debug/test builds only: `build.rs` fails a release build
if `VENDOR_PUBKEY_SPKI_PEM` is missing, invalid, or equal to the committed test key.

So packs only verify in an app built with the **matching** public key. The official
releases sign packs with the project's private key and compile the matching public key
into the app (both stored as CI secrets — `MODELPACK_SIGNING_KEY` and
`VENDOR_PUBKEY_SPKI_PEM`). If you build the app yourself with your own key, sign your
packs with the corresponding private key.

## Packs shipped with each release

Built by [`scripts/build-model-packs.sh`](https://github.com/KindTechUK/folio/blob/main/scripts/build-model-packs.sh)
from upstream [k2-fsa/sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) weights:

| Pack id | task | contents | license |
|---------|------|----------|---------|
| `asr-parakeet-tdt-0.6b-v2` | asr | NVIDIA Parakeet-TDT-0.6B-v2 (int8, default) + Silero VAD | CC-BY-4.0 (VAD: MIT) |
| `asr-whisper-small.en` | asr | Whisper small.en (int8) + Silero VAD | MIT |
| `asr-whisper-medium.en` | asr | Whisper medium.en (int8) + Silero VAD | MIT |
| `diarization-pyannote-campplus` | diarization | pyannote segmentation-3.0 + 3D-Speaker CAM++ | MIT AND Apache-2.0 |

Upstream weights come from the `asr-models`, `speaker-segmentation-models`, and
`speaker-recongition-models` (sic) release tags of k2-fsa/sherpa-onnx, except Parakeet,
which is fetched from the sherpa-onnx Hugging Face repository at a fixed revision
(ungated, no token). The build script checks each download against the SHA-256 pinned in
[`scripts/modelpack-assets.sha256`](https://github.com/KindTechUK/folio/blob/main/scripts/modelpack-assets.sha256).

Indicative CPU speed (5-minute English conversation, Apple Silicon, 4 threads, int8):
Whisper `small.en` ran at a real-time factor of ~0.11 and `medium.en` at ~0.36;
Parakeet is roughly 6× faster than `small.en` and is the only one with native word
timestamps. Diarization adds ~0.05 RTF. Automatic speaker counting tends to
over-segment multi-speaker audio, so give the expected speaker count when you know it.

## Build & sign your own

```bash
# 1. One-time: generate a keypair.
cargo run --manifest-path tools/modelpack-sign/Cargo.toml -- keygen --out-dir keys

# 2. Sign a pack from a directory laid out as the engine expects, e.g.
#    <models>/sherpa-onnx-whisper-small.en/{small.en-encoder,small.en-decoder}.int8.onnx
#    <models>/sherpa-onnx-whisper-small.en/small.en-tokens.txt
#    <models>/silero_vad.onnx
cargo run --manifest-path tools/modelpack-sign/Cargo.toml -- pack \
  --model-dir <models> --out asr-whisper-small.en --key keys/private.pkcs8.pem \
  --id asr-whisper-small.en --version 1.0.0 \
  --engine sherpa-onnx --engine-min-version 1.13.2 \
  --task asr --license MIT \
  --attribution "OpenAI Whisper small.en + Silero VAD"

# 3. Run the app trusting your key, then import the pack via the Models screen.
(cd app && VENDOR_PUBKEY_SPKI_PEM="$(cat ../keys/public.spki.pem)" npm run tauri dev)
```

The `pack` command self-verifies before writing, so a pack that's produced is guaranteed
to pass the in-app verifier (with the matching key). To check an existing pack:

```bash
cargo run --manifest-path tools/modelpack-sign/Cargo.toml -- verify \
  --pack asr-whisper-small.en --pubkey keys/public.spki.pem
```

The reusable [`scripts/build-model-packs.sh`](https://github.com/KindTechUK/folio/blob/main/scripts/build-model-packs.sh)
downloads the upstream weights and builds all four release packs in one go:

```bash
VERSION=1.0.0 KEY=keys/private.pkcs8.pem OUT=dist-packs bash scripts/build-model-packs.sh
```
