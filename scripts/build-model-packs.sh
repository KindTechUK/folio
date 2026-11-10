#!/usr/bin/env bash
# build-model-packs.sh — download upstream sherpa-onnx weights and produce the
# SIGNED model packs the app distributes.
#
# Each pack is a directory of { manifest.json, weights.bin } produced by
# tools/modelpack-sign. The staged layout MUST mirror the engine's
# `required_model_files()` (engine/src/lib.rs), because the pack zip is extracted
# verbatim into the verified-models cache:
#
#   asr-parakeet-tdt-0.6b-v2/ sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/{encoder,decoder,joiner}.int8.onnx
#                           sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/tokens.txt
#                           silero_vad.onnx            (the DEFAULT ASR model)
#   asr-whisper-small.en/   sherpa-onnx-whisper-small.en/{small.en-encoder,small.en-decoder}.int8.onnx
#                           sherpa-onnx-whisper-small.en/small.en-tokens.txt
#                           silero_vad.onnx
#   asr-whisper-medium.en/  (same shape, medium.en)
#   diarization/            sherpa-onnx-pyannote-segmentation-3-0/model.onnx
#                           3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx
#
# Usage:
#   VERSION=0.1.0 KEY=/path/to/private.pkcs8.pem OUT=dist-packs \
#     bash scripts/build-model-packs.sh
set -euo pipefail

VERSION="${VERSION:?set VERSION (e.g. 0.1.0; must match ^\d+\.\d+\.\d+$)}"
KEY="${KEY:?set KEY (path to the vendor PKCS8 private PEM)}"
OUT="${OUT:-dist-packs}"
ENGINE="sherpa-onnx"
ENGINE_MIN="1.13.2" # must be <= the app's reported engine version (models.rs)

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="https://github.com/k2-fsa/sherpa-onnx/releases/download"
SIGN=(cargo run --release --quiet --manifest-path "$REPO_ROOT/tools/modelpack-sign/Cargo.toml" --)
ASSET_MANIFEST="$REPO_ROOT/scripts/modelpack-assets.sha256"
source "$REPO_ROOT/scripts/modelpack-security.sh"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$OUT"

dl() {
  local url="$1"
  local dest="$2"
  local asset="$3"
  echo "  ↓ $url"
  curl -fsSL "$url" -o "$dest"
  verify_pinned_asset "$ASSET_MANIFEST" "$dest" "$asset"
}

# Build the producer once up front so per-pack runs don't interleave build logs.
( cd "$REPO_ROOT" && cargo build --release --quiet --manifest-path tools/modelpack-sign/Cargo.toml )

# Silero VAD is shared by all ASR packs (the engine always needs it).
dl "$BASE/asr-models/silero_vad.onnx" "$work/silero_vad.onnx" "silero_vad.onnx"

build_asr() {
  local model="$1" # small | medium
  local dir="sherpa-onnx-whisper-${model}.en"
  echo "==> ASR pack: ${model}.en"
  dl "$BASE/asr-models/${dir}.tar.bz2" "$work/${dir}.tar.bz2" "${dir}.tar.bz2"
  tar xjf "$work/${dir}.tar.bz2" -C "$work"
  reject_archive_links "$work/$dir"
  local stage="$work/stage-${model}"
  mkdir -p "$stage/${dir}"
  require_regular_file "$work/${dir}/${model}.en-encoder.int8.onnx"
  require_regular_file "$work/${dir}/${model}.en-decoder.int8.onnx"
  require_regular_file "$work/${dir}/${model}.en-tokens.txt"
  cp "$work/${dir}/${model}.en-encoder.int8.onnx" "$stage/${dir}/"
  cp "$work/${dir}/${model}.en-decoder.int8.onnx" "$stage/${dir}/"
  cp "$work/${dir}/${model}.en-tokens.txt"        "$stage/${dir}/"
  cp "$work/silero_vad.onnx" "$stage/"
  "${SIGN[@]}" pack \
    --model-dir "$stage" --out "$OUT/asr-whisper-${model}.en" --key "$KEY" \
    --id "asr-whisper-${model}.en" --version "$VERSION" \
    --engine "$ENGINE" --engine-min-version "$ENGINE_MIN" \
    --task asr --license "MIT" \
    --attribution "OpenAI Whisper ${model}.en (MIT) + Silero VAD (MIT); packaged by k2-fsa/sherpa-onnx"
}

# Parakeet-TDT-0.6B-v2 (English) — the DEFAULT ASR model. NeMo transducer, so
# the layout is encoder/decoder/joiner + tokens (no `<stem>-` prefix). Same
# sherpa-onnx engine as the Whisper packs; license is CC-BY-4.0 (NVIDIA).
build_parakeet() {
  local dir="sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8"
  echo "==> ASR pack: parakeet-tdt-0.6b-v2 (int8, default)"
  dl "$BASE/asr-models/${dir}.tar.bz2" "$work/${dir}.tar.bz2" "${dir}.tar.bz2"
  tar xjf "$work/${dir}.tar.bz2" -C "$work"
  reject_archive_links "$work/$dir"
  local stage="$work/stage-parakeet"
  mkdir -p "$stage/${dir}"
  require_regular_file "$work/${dir}/encoder.int8.onnx"
  require_regular_file "$work/${dir}/decoder.int8.onnx"
  require_regular_file "$work/${dir}/joiner.int8.onnx"
  require_regular_file "$work/${dir}/tokens.txt"
  cp "$work/${dir}/encoder.int8.onnx" "$stage/${dir}/"
  cp "$work/${dir}/decoder.int8.onnx" "$stage/${dir}/"
  cp "$work/${dir}/joiner.int8.onnx"  "$stage/${dir}/"
  cp "$work/${dir}/tokens.txt"        "$stage/${dir}/"
  cp "$work/silero_vad.onnx" "$stage/"
  "${SIGN[@]}" pack \
    --model-dir "$stage" --out "$OUT/asr-parakeet-tdt-0.6b-v2" --key "$KEY" \
    --id "asr-parakeet-tdt-0.6b-v2" --version "$VERSION" \
    --engine "$ENGINE" --engine-min-version "$ENGINE_MIN" \
    --task asr --license "CC-BY-4.0" \
    --attribution "NVIDIA Parakeet-TDT-0.6B-v2 (CC-BY-4.0) + Silero VAD (MIT); ONNX export & packaging by k2-fsa/sherpa-onnx"
}

build_parakeet
build_asr small
build_asr medium

echo "==> Diarization pack: pyannote segmentation-3.0 + 3D-Speaker CAM++"
dl "$BASE/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2" "$work/seg.tar.bz2" "sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"
tar xjf "$work/seg.tar.bz2" -C "$work"
reject_archive_links "$work/sherpa-onnx-pyannote-segmentation-3-0"
require_regular_file "$work/sherpa-onnx-pyannote-segmentation-3-0/model.onnx"
dl "$BASE/speaker-recongition-models/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx" "$work/campplus.onnx" "3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx"
require_regular_file "$work/campplus.onnx"
stage="$work/stage-diar"
mkdir -p "$stage/sherpa-onnx-pyannote-segmentation-3-0"
cp "$work/sherpa-onnx-pyannote-segmentation-3-0/model.onnx" "$stage/sherpa-onnx-pyannote-segmentation-3-0/"
cp "$work/campplus.onnx" "$stage/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx"
"${SIGN[@]}" pack \
  --model-dir "$stage" --out "$OUT/diarization-pyannote-campplus" --key "$KEY" \
  --id "diarization-pyannote-campplus" --version "$VERSION" \
  --engine "$ENGINE" --engine-min-version "$ENGINE_MIN" \
  --task diarization --license "MIT AND Apache-2.0" \
  --attribution "pyannote segmentation-3.0 (MIT) + 3D-Speaker CAM++ (Apache-2.0); packaged by k2-fsa/sherpa-onnx"

# Zip each pack dir (manifest.json + weights.bin) into one downloadable asset,
# then checksum. Users unzip and import the folder from the app's Models screen.
echo "==> Zipping packs + checksums"
( cd "$OUT" && for p in */; do p="${p%/}"; rm -f "$p.zip"; zip -r -q "$p.zip" "$p"; done )
( cd "$OUT" && sha256sum ./*.zip > SHA256SUMS )

echo "Done. Packs in $OUT:"
ls -la "$OUT"
