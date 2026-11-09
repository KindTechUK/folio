#!/usr/bin/env bash
#
# e2e.sh — reusable end-to-end verification harness for the transcription pipeline.
#
#   scripts/e2e.sh <audio> [--speakers N] [--model small.en|medium.en|parakeet]
#
# Steps:
#   1. If <audio> is not already a 16 kHz mono 16-bit WAV, decode it via ffmpeg to a
#      temp WAV.
#   2. Build (release) + run the engine example `transcribe_file`, which loads the WAV,
#      runs ASR + diarization + merge, and writes the `{segments:[...]}` sidecar to
#      out/<name>.engine.json. SHERPA_ONNX_LIB_DIR is pointed at the vendored
#      prebuilt libs so the sherpa-onnx-sys build script does NOT re-download.
#   3. Run the core harness (scripts/e2e_core.mjs) over that sidecar: buildTurns +
#      a full editor round-trip with invariant assertions, and write a human-readable
#      speaker-labelled transcript to out/e2e_transcript.txt.

set -euo pipefail

# ---- locate repo root (this script lives in <root>/scripts) ----
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ENGINE_DIR="$ROOT/engine"
CORE_DIR="$ROOT/core"
OUT_DIR="$ROOT/out"

# ---- args ----
if [[ $# -lt 1 ]]; then
  echo "usage: e2e.sh <audio> [--speakers N] [--model small.en|medium.en|parakeet]" >&2
  exit 2
fi

AUDIO="$1"; shift

SPEAKERS=""
MODEL="small.en"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --speakers) SPEAKERS="$2"; shift 2 ;;
    --model)    MODEL="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ ! -f "$AUDIO" ]]; then
  echo "audio file not found: $AUDIO" >&2
  exit 2
fi

# Absolutize the audio path: the engine example runs with cwd=engine/, so any
# relative path would resolve against the wrong directory.
AUDIO="$(cd "$(dirname "$AUDIO")" && pwd)/$(basename "$AUDIO")"

mkdir -p "$OUT_DIR"

# ---- derive sidecar output name from the input basename ----
BASE="$(basename "$AUDIO")"
STEM="${BASE%.*}"
SAFE_STEM="$(printf '%s' "$STEM" | tr -c 'A-Za-z0-9._-' '_')"
SIDECAR="$OUT_DIR/${SAFE_STEM}.engine.json"
TRANSCRIPT="$OUT_DIR/e2e_transcript.txt"

# ---- step 1: ensure a 16 kHz mono 16-bit WAV ----
need_decode=1
if command -v ffprobe >/dev/null 2>&1; then
  read -r CODEC SR CH < <(
    ffprobe -v error -select_streams a:0 \
      -show_entries stream=codec_name,sample_rate,channels \
      -of "default=noprint_wrappers=1:nokey=1" "$AUDIO" 2>/dev/null | tr '\n' ' '
  ) || true
  if [[ "$CODEC" == "pcm_s16le" && "$SR" == "16000" && "$CH" == "1" ]]; then
    need_decode=0
  fi
fi

TMP_WAV=""
cleanup() { [[ -n "$TMP_WAV" && -f "$TMP_WAV" ]] && rm -f "$TMP_WAV" || true; }
trap cleanup EXIT

if [[ "$need_decode" -eq 1 ]]; then
  if ! command -v ffmpeg >/dev/null 2>&1; then
    echo "ffmpeg not found and input is not 16k mono 16-bit WAV: $AUDIO" >&2
    exit 1
  fi
  TMP_WAV="$(mktemp -t e2e_XXXXXX).wav"
  echo "[e2e] decoding -> 16k mono 16-bit WAV: $TMP_WAV" >&2
  ffmpeg -hide_banner -loglevel error -y -i "$AUDIO" \
    -ac 1 -ar 16000 -acodec pcm_s16le "$TMP_WAV"
  WAV="$TMP_WAV"
else
  echo "[e2e] input already 16k mono 16-bit WAV; using as-is" >&2
  WAV="$AUDIO"
fi

# ---- step 2: engine release example ----
# Point the sherpa-onnx-sys build script at the vendored prebuilt libs so it
# does NOT re-download (see engine/README.md). Auto-detect the lib dir.
PREBUILT_ROOT="$ENGINE_DIR/target/sherpa-onnx-prebuilt"
if [[ -z "${SHERPA_ONNX_LIB_DIR:-}" ]]; then
  LIB_DIR="$(find "$PREBUILT_ROOT" -type d -name lib 2>/dev/null | head -n1 || true)"
  if [[ -n "$LIB_DIR" ]]; then
    export SHERPA_ONNX_LIB_DIR="$LIB_DIR"
  fi
fi
if [[ -n "${SHERPA_ONNX_LIB_DIR:-}" ]]; then
  echo "[e2e] SHERPA_ONNX_LIB_DIR=$SHERPA_ONNX_LIB_DIR" >&2
else
  echo "[e2e] WARNING: SHERPA_ONNX_LIB_DIR not set and no vendored libs found; build may download" >&2
fi

ENGINE_ARGS=("$WAV" "$SIDECAR" "--model" "$MODEL")
if [[ -n "$SPEAKERS" ]]; then
  ENGINE_ARGS+=("--speakers" "$SPEAKERS")
fi

echo "[e2e] running engine example (release): transcribe_file ${ENGINE_ARGS[*]}" >&2
(
  cd "$ENGINE_DIR"
  cargo run --release --example transcribe_file -- "${ENGINE_ARGS[@]}"
)

if [[ ! -s "$SIDECAR" ]]; then
  echo "[e2e] engine produced no sidecar at $SIDECAR" >&2
  exit 1
fi
echo "[e2e] sidecar: $SIDECAR" >&2

# ---- step 3: core harness ----
# vite-node (from core/node_modules) transpiles the core TS imports on the fly.
VITE_NODE="$CORE_DIR/node_modules/.bin/vite-node"
if [[ ! -x "$VITE_NODE" ]]; then
  echo "[e2e] vite-node not found at $VITE_NODE (run 'npm install' in core/)" >&2
  exit 1
fi

echo "[e2e] running core harness over sidecar" >&2
(
  cd "$CORE_DIR"
  E2E_SIDECAR="$SIDECAR" E2E_TRANSCRIPT="$TRANSCRIPT" \
    "$VITE_NODE" "$SCRIPT_DIR/e2e_core.mjs"
)

echo "[e2e] OK — sidecar=$SIDECAR transcript=$TRANSCRIPT" >&2
