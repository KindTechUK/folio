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
  require_regular_file "$dest"
  verify_pinned_asset "$ASSET_MANIFEST" "$dest" "$asset"
}

# Build the producer once up front so per-pack runs don't interleave build logs.
( cd "$REPO_ROOT" && cargo build --release --quiet --manifest-path tools/modelpack-sign/Cargo.toml )

# Upstream weights are fetched per file from the sherpa-onnx Hugging Face repos
# at fixed revisions: GitHub release assets are mutable (upstream re-exported
# the Parakeet tarball on 2026-11-07 with different weights). Each revision
# holds exactly the weights shipped in v0.2.1.
HF="https://huggingface.co/csukuangfj"
WHISPER_SMALL_HF="$HF/sherpa-onnx-whisper-small.en/resolve/d9533f69affd85061aee349af7fea5cb2996dbbe"
WHISPER_MEDIUM_HF="$HF/sherpa-onnx-whisper-medium.en/resolve/251ab4521f354490e8f2c206fbd0b7f3f6b0a7ec"
PYANNOTE_HF="$HF/sherpa-onnx-pyannote-segmentation-3-0/resolve/9403a6902bb58e3d5ae8c7e77c3422de279db2e0"
CAMPPLUS_HF="$HF/speaker-embedding-models/resolve/0743f301363dec56491a490f6d6cbc9d67f9a3bf"

# Silero VAD is shared by all ASR packs (the engine always needs it). It stays
# on the GitHub release asset: no sherpa-onnx Hugging Face repo holds these bytes.
dl "$BASE/asr-models/silero_vad.onnx" "$work/silero_vad.onnx" "silero_vad.onnx"

build_asr() {
  local model="$1" # small | medium
  local dir="sherpa-onnx-whisper-${model}.en"
  local src
  case "$model" in
    small) src="$WHISPER_SMALL_HF" ;;
    medium) src="$WHISPER_MEDIUM_HF" ;;
    *) echo "unknown whisper model: $model" >&2; return 1 ;;
  esac
  echo "==> ASR pack: ${model}.en"
  local stage="$work/stage-${model}"
  mkdir -p "$stage/${dir}"
  local f
  for f in "${model}.en-encoder.int8.onnx" "${model}.en-decoder.int8.onnx" "${model}.en-tokens.txt"; do
    dl "$src/$f" "$stage/${dir}/$f" "whisper-${model}.en/$f"
  done
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
# Fetched per file from Hugging Face at a fixed revision (see above); this
# revision holds the weights shipped since v0.2.0.
PARAKEET_HF="$HF/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/resolve/1ab9323565ddb038682214b292f588070a538ce2"

build_parakeet() {
  local dir="sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8"
  echo "==> ASR pack: parakeet-tdt-0.6b-v2 (int8, default)"
  local stage="$work/stage-parakeet"
  mkdir -p "$stage/${dir}"
  local f
  for f in encoder.int8.onnx decoder.int8.onnx joiner.int8.onnx tokens.txt; do
    dl "$PARAKEET_HF/$f" "$stage/${dir}/$f" "parakeet-tdt-0.6b-v2-int8/$f"
  done
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
stage="$work/stage-diar"
mkdir -p "$stage/sherpa-onnx-pyannote-segmentation-3-0"
dl "$PYANNOTE_HF/model.onnx" "$stage/sherpa-onnx-pyannote-segmentation-3-0/model.onnx" "pyannote-segmentation-3-0/model.onnx"
dl "$CAMPPLUS_HF/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx" "$stage/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx" "3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx"
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
