#!/bin/bash
# Standard llama-bench numbers per model (CPU only), for comparison with published Windows figures.
# Usage: ./bench.sh qwen3-4b gemma4-e4b ...   (keys from run.py MODELS)
set -e
cd "$(dirname "$0")"
mkdir -p results
for key in "$@"; do
  f=$(python3 -c "import run; print(run.MODEL_DIR / run.MODELS['$key'])")
  llama-bench -m "$f" -ngl 0 -dev none -fa 0 -t 4,8 -p 512 -n 128 -d 0,4096 -r 2 -o jsonl > "results/bench-$key.jsonl"
  echo "bench $key done"
done
