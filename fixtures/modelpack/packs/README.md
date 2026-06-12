# Test model packs

These are **TEST placeholder packs** for the verified-pack resolver tests in
`app/src-tauri/src/models.rs`. They are signed with the **TEST key**
(`fixtures/modelpack/keys/test-private.pkcs8.pem`), whose public SPKI is the
in-app `DEV_TEST_PUBKEY_SPKI_PEM` fallback, so the committed fixtures verify in
dev/CI builds. They must **never** be served to real users.

The model files inside each pack's `weights.bin` (a zip) are **empty** — they
exist only to satisfy the resolver's required-file existence check
(`engine::required_model_files`), not to run inference.

## Packs

- `asr-small-en/` — `id = sightline/whisper-small.en`, `engine = sherpa-onnx`,
  `task = asr`. Its `weights.bin` zips the 4 ASR+VAD relative paths for
  `small.en` (`sherpa-onnx-whisper-small.en/small.en-{encoder,decoder}.int8.onnx`,
  `…-tokens.txt`, `silero_vad.onnx`) — all empty.

## Regenerating

```sh
TMP=$(mktemp -d)
mkdir -p "$TMP/sherpa-onnx-whisper-small.en"
: > "$TMP/sherpa-onnx-whisper-small.en/small.en-encoder.int8.onnx"
: > "$TMP/sherpa-onnx-whisper-small.en/small.en-decoder.int8.onnx"
: > "$TMP/sherpa-onnx-whisper-small.en/small.en-tokens.txt"
: > "$TMP/silero_vad.onnx"
cargo run --manifest-path tools/modelpack-sign/Cargo.toml -- pack \
  --model-dir "$TMP" --out fixtures/modelpack/packs/asr-small-en \
  --key fixtures/modelpack/keys/test-private.pkcs8.pem \
  --id sightline/whisper-small.en --version 1.0.0 --engine sherpa-onnx \
  --engine-min-version 1.13.2 --task asr --license MIT \
  --attribution "Sightline Test ASR small.en"
rm -rf "$TMP"
```
