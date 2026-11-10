# Spike S2: local LLM notes on long transcripts (interim)

> Status: **interim, no verdict yet** · Run before 2026-10-01 · Recorded 2026-11-10.
> Plan: [`next-phase-spikes.md`](next-phase-spikes.md#s2-local-llm-on-long-transcripts-the-headline-question).
> Harness: `spikes/s2-llm-notes/` (setup and commands are in its README).

## What ran

Only the first screening run finished before the development laptop was returned:

- **Model:** Qwen3-4B-Instruct-2507, Q4_0, through a CPU-only `llama-server` (4 threads, 32k context).
  The other five candidates in `run.py` (Qwen3.5-4B, SmolLM3-3B, Phi-4-mini, Ministral-3-3B,
  Gemma-4-E4B) were not run.
- **Transcripts:** the synthetic care assessment (61 min, 680 lines) ran to completion. The synthetic
  legal interview (46 min) stopped after 5 of its map calls, so it has no summary row.
- **Strategies:** map-reduce and sectioned (both defined in `run.py`). The single-pass strategy was not
  run.
- **Quality:** not scored. No blind-judge verdicts were written, so there are no omission or
  fabrication figures yet.
- **Machine:** the exact model was not recorded. The load average was 7.6–7.9 during the run, so the
  machine was busy and the timings are pessimistic. This is not the reference Windows laptop.

## Raw numbers: synthetic care assessment (61 min)

From `spikes/s2-llm-notes/results/screen.jsonl`.

| Stage | Wall time | Prompt tokens | Prefill tok/s | Generation tok/s | Peak RSS |
|---|---|---|---|---|---|
| Map, each ~10-min section (6 calls) | 135–163 s | 3,092–3,271 | 55–57 | 17–19 | 4.3–5.8 GB |
| Map, final short section | 38 s | 618 | 101 | 30 | 4.3 GB |
| Reduce (194 facts into the template) | 394 s | 5,969 | 39 | 12 | 4.7 GB |

Every call returned valid JSON and finished with `stop`; none needed salvaging.

| Strategy | Time from stop to note | Target (60 min) |
|---|---|---|
| Map-reduce, all after stop | 1,353 s (22.5 min) | ≤ 5 min |
| Sectioned, all after stop | 959 s (16 min) | ≤ 5 min |
| Map-reduce, maps during recording (only last map + reduce after stop) | 432 s (7.2 min) | ≤ 5 min |
| Sectioned, maps during recording (only last map after stop) | 38 s | ≤ 5 min |

The 38 s figure is flattered by a short final section (618 prompt tokens). With a full 10-minute final
section, the last map takes about 135–163 s, so the realistic worst case is **about 2.5–3 minutes**.

## What this suggests so far

- **After-stop processing is too slow on this machine.** Both strategies take 16–23 minutes for a
  60-minute recording, against a 5-minute target. Prefill at about 55 tok/s dominates, as the plan
  predicted.
- **Processing during recording meets the target.** The sectioned strategy leaves only the final
  section's map after stop. This needs the live capture of P1: the recording has to be in the app, and
  its sections have to reach the model while the conversation is still going.
- **Memory fits.** Peak RSS was 5.8 GB, inside the 6 GB limit, with ASR not loaded.
- **The JSON contract held** for every call with this model.

None of this is a verdict. The quality scoring and the other models are still to run, and the speed
figures need repeating on an idle machine and then on the reference Windows laptop.

## Still to do for a verdict

1. Run the other five candidates (`python run.py --run screen --models all ...`) on an idle machine.
2. Finish the legal-interview run, and add the AMI 15/45/60/90-minute buckets.
3. Write the blind-judge verdicts (`score.py packets`, then `judge_instructions.md`) and collect them.
4. Repeat the finalists on the reference Windows laptop.
5. Record the verdict and what changes in the PRD, in this file.
