# asr-bench

Speech-to-text evaluation bench. It decodes a **bench pack** with the same engine and sherpa-onnx build
the app ships, scores it the way the [Open ASR Leaderboard](https://huggingface.co/spaces/hf-audio/open_asr_leaderboard)
does, and reports word error rate with 95% intervals, speed and peak memory. Dev/eval tool only: not
linked into the app, and it makes no network calls. Background: `docs/research/voice2text-lessons-2026-11.md`.
Running it on a Windows laptop: `docs/spikes/windows-bench-runbook.md`.

## 1. Build a pack (once, needs network)

```sh
uv run --script tools/asr-bench/pack/build_pack.py ~/folio-bench-pack   # ~0.5 GB, plus ~3 GB of downloads in .cache
rm -rf ~/folio-bench-pack/.cache
```

| set | what | clips |
|---|---|---|
| `ami-sdm` | AMI meetings on the room microphone, 2–35 s clips | 150 |
| `ami-ihm` | the leaderboard's AMI-Cleaned (headset microphones) | 150 |
| `earnings22` | earnings calls in short chunks, original transcripts | 150 |
| `ami-sdm-long` | whole meetings IS1009a, ES2004a, TS3003a on the room microphone (55 min) | 3 |
| `earnings22-long` | the leaderboard's 6 cleaned Earnings22 calls (115 min) | 6 |

The short sets are voice2text's samples (same seed and rules), so scores compare with its grid. Long-form
sets exist for the segmentation questions: segmentation only matters on long audio.

## 2. Get models

Each model is a folder from the sherpa-onnx `asr-models` release, next to `silero_vad.onnx`:

```sh
M=~/folio-bench-models; mkdir -p $M; cd $M
curl -LO https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx
curl -L https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2 | tar xj
```

Model names (`--model`) and their folders are in `engine/src/lib.rs` (`AsrModel::stem` / `dir`):
`parakeet-tdt-0.6b-v2` (shipped), `parakeet-tdt-0.6b-v3`, `parakeet-tdt-0.6b-ultra`,
`parakeet-tdt-0.6b-redux`, `parakeet-unified-en-0.6b`, `qwen3-asr-0.6b`, `cohere-transcribe`,
`small.en`, `medium.en`.

## 3. Run, then report

```sh
B=tools/asr-bench/target/release/asr-bench   # cargo build --release --manifest-path tools/asr-bench/Cargo.toml
P=~/folio-bench-pack; export FOLIO_MODELS_DIR=~/folio-bench-models

$B run --pack $P --set ami-sdm                                   # Parakeet v2, today's file path
$B run --pack $P --set ami-sdm --model parakeet-tdt-0.6b-ultra
$B run --pack $P --set ami-sdm-long --segmenter energy:30/5      # D1: no VAD gating
$B report --pack $P --baseline ami-sdm_parakeet-tdt-0.6b-v2_vad-0.8-120_greedy results/*.jsonl
```

`run` writes `results/<tag>.jsonl` (one row per clip, then a summary with load time and peak memory)
and resumes where it stopped. Options:

- `--segmenter`: `vad:<silence>/<max>` (default `vad:0.8/120`, the app's file path; `vad:0.8/15` is
  live capture), `vad-pad:<silence>/<max>/<pad_ms>`, `energy:<piece>/<search>` (voice2text's rule:
  cut at the quietest 100 ms near each mark, decode everything), `whole`.
- `--decoding`: `greedy` (shipped) or `beam:<paths>`. Watch the `empty` column: sherpa-onnx #3267
  reported beam search returning nothing on Parakeet-TDT.
- `--hotwords FILE --hotwords-score S`: one phrase per line; transducers need `beam:`.
- `--threads N` (the app uses 4), `--limit N`, `--tag T`.

`asr-bench score "<reference>" "<hypothesis>"` shows how a pair normalises and scores;
`asr-bench machine` prints the CPU, cores, RAM and OS that every run also records.

## Scoring

`src/normalize.rs` and `src/align.rs` port the leaderboard's English normaliser (commit 2aaa07a)
and kaldialign's compound-merging edit distance. `tests/oasr_parity.rs` checks them against
outputs of the Python originals (`tests/fixtures/make_golden.py`; see `NOTICE`). Intervals
resample clips 4,000 times with a fixed seed; a Δ against the baseline resamples the same clips
for both runs. Sets with fewer than 10 units (the long-form sets) show no interval.
