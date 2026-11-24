# Spike S2: local LLM notes on long transcripts

The harness for spike S2 in [`docs/spikes/next-phase-spikes.md`](../../docs/spikes/next-phase-spikes.md).
The write-up is in [`docs/spikes/s2-local-llm-notes.md`](../../docs/spikes/s2-local-llm-notes.md).

This is throwaway spike code: plain Python (stdlib only, plus `datasets` for AMI) driving a
**CPU-only `llama-server`**. It runs unchanged on macOS or Windows.

## Setup

1. Install llama.cpp (`brew install llama.cpp`, or a Windows release zip from
   github.com/ggml-org/llama.cpp) so `llama-server` and `llama-bench` are on `PATH`.
2. Download the candidate GGUFs into `~/kindtech-transcribe/models/llm/`. The files are listed in
   `run.py` `MODELS`. All are ungated on Hugging Face (Apache-2.0 / MIT). Use **Q4_0**: the llama.cpp CPU
   backend repacks it, which gives about 2× faster prefill than Q4_K_M.
3. Build the AMI buckets. This streams the transcripts only, not the audio:
   `python prepare_data.py` (needs the `datasets` package).

## Run

```bash
./bench.sh qwen3-4b gemma4-e4b                     # llama-bench pp/tg, 4 and 8 threads
python run.py --run screen --models all \
  --transcripts synthetic_care_assessment,synthetic_legal_interview
python run.py --run full --models <finalists> \
  --transcripts ami-15,ami-45,ami-60,ami-90,synthetic_care_assessment,synthetic_legal_interview
python score.py auto --run screen                  # automatic metrics
python score.py packets --run screen               # blind judge packets (synthetic only)
# ...write results/judge/<run>/<id>.verdict.json per packet, then:
python score.py collect --run screen
python report.py --run screen --run full           # markdown tables
python memory.py qwen3.5-4b 8192                  # peak memory of one map call (macOS footprint)
```

`--threads 4` is the default, mimicking a U-series laptop's performance cores. Every run
records the machine's load average, because background load skews CPU timings.

`peak_rss_mb` in the run results is `ps` RSS. On macOS it counts memory-mapped weights and moves with
memory compression, so use `memory.py` for peak memory.

## Data

- `data/synthetic_care_assessment.json` (61 min) and `data/synthetic_legal_interview.json` (46 min) are
  fictional role-plays written for this spike. Each has a **facts checklist** and **traps** (negations,
  hypotheticals, hearsay, self-corrections, things never said, and views attributed to the wrong
  person).
- `data/ami/`: AMI Meeting Corpus human transcripts in 15/45/60/90-minute buckets. They are
  generated and not committed.
- `results/notes/full/*/ami-*.json` are model-written notes on AMI meetings, which may quote the
  transcripts. The AMI Meeting Corpus is © the AMI Consortium (University of Edinburgh and partners),
  licensed CC-BY-4.0.
