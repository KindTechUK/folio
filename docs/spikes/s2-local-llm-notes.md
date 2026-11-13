# Spike S2: local LLM notes on long transcripts

> Status: **Go, with conditions** · Run 2026-11-12 to 2026-11-13 on a Mac (a proxy for the reference laptop).
> Plan: [`next-phase-spikes.md`](next-phase-spikes.md#s2-local-llm-on-long-transcripts-the-headline-question).
> Harness: `spikes/s2-llm-notes/` (setup and commands are in its README).

## Verdict

A local 4B model can turn a 15–90-minute conversation into a template note within the target time. This only
works if the notes are drafted **section by section during the recording**. Speed and memory pass. Whether
notes on longer recordings miss more is still open (condition 6).

| Pass criterion | Result | |
|---|---|---|
| 60-min transcript: note ≤ 5 min after stop | 14 s to 2.7 min (sectioned, during recording) | **Pass** |
| 90-min transcript: note ≤ 8 min after stop | 83 s to 2.7 min | **Pass** |
| Peak RAM ≤ 6 GB, ASR unloaded | 4.4 GB (Qwen3.5-4B) and 4.7 GB (Qwen3-4B) at 8k context | **Pass** |
| No significant rise in omissions from 15 to 60 min | Facts were judged only at 46 and 61 min; the AMI meetings have no facts checklist | **Not measured** |

Strategies that start only when the user presses stop fail the speed target, whichever model is used.
A single pass on a 60-minute transcript takes 7–30 minutes.

**Conditions:**
1. **Draft notes section by section during the recording.** Each 10-minute section is processed while the next
   one is being recorded. On this machine each section took 1–4 minutes, so processing keeps up.
2. **Use the section notes as the note.** These are the facts from each section, grouped under the template's
   headings. One reduce call that rewrites them into a shorter note loses about half the facts: recall fell from
   0.72 to 0.39 for Qwen3-4B. Composing a readable note without that loss is open design work.
3. **Load the LLM and ASR together, with an 8k context.** Condition 1 needs both models in memory. The PRD
   currently says they are never loaded together. The total is about 6 GB (see [Memory](#memory)).
4. **Carry two finalists into S3 and S4:** Qwen3-4B-Instruct-2507 and Qwen3.5-4B, both Apache-2.0. Qwen3-4B
   is slightly ahead on quality. Qwen3.5-4B has fewer weakly supported sentences, uses less memory at long
   context, and looked faster, but the speed comparison is not clean. Safeguarding refusals (S4) and
   citation accuracy (S3) decide between them.
5. **Repeat the timings on the reference Windows laptop.** These are Mac numbers taken under background load.
6. **Measure omissions by length before P2 relies on long recordings.** Judge fact recall on a 15-minute
   excerpt and on the full 60 minutes of the same transcript, for example the first 15 minutes of the care
   assessment with the checklist facts that fall in it. Weak support (below) can't stand in for this: it only
   checks the sentences a model wrote, so a note that leaves most facts out can still score well.

## Environment

- **Machine:** Apple M5 Pro (18 cores, 48 GB), macOS 27.2. This is not the reference laptop. Every run used
  **4 threads**, to mimic a U-series laptop's performance cores, and no GPU (`-ngl 0 -dev none`).
- **Runtime:** llama.cpp `llama-server` 0.9.0 (build 12110, commit cd959bae9), Homebrew, `-fa off`,
  temperature 0.2, seed 42, JSON-schema-constrained output.
- **Models:** six candidates, all quantised to Q4_0 and ungated:

  | Model | GGUF source | Licence | SHA-256 |
  |---|---|---|---|
  | Qwen3-4B-Instruct-2507 | `unsloth/Qwen3-4B-Instruct-2507-GGUF` | Apache-2.0 | `e0ba675d86ab277c61701c6793659b2ae801d95e3be791464c321e6fbf613be2` |
  | Qwen3.5-4B | `unsloth/Qwen3.5-4B-GGUF` | Apache-2.0 | `298fcb5fe7a77ccc79745ae24751560c5ac56874caff4bb39b1f2055bd72b8bb` |
  | SmolLM3-3B | `bartowski/HuggingFaceTB_SmolLM3-3B-GGUF` | Apache-2.0 | `1e0e56d458e6b6d5ea18d0e5fe598cca252450308f1648afab49984fceae47da` |
  | Phi-4-mini-instruct | `bartowski/microsoft_Phi-4-mini-instruct-GGUF` | MIT | `2124412a2d3410dd05c5d01796457283812210633165a2a86c909f815971518e` |
  | Ministral-3-3B-Instruct-2512 | `bartowski/mistralai_Ministral-3-3B-Instruct-2512-GGUF` | Apache-2.0 | `c2993f4b86ddcae94031b2e384169889ceecd51b148ceda30c0f826fcce721c8` |
  | Gemma-4-E4B-it | `unsloth/gemma-4-E4B-it-GGUF` | Apache-2.0 | `4a403d2e4d80281063e4f517b1c061ded8476b4011a4fc2ba7dbff707075547e` |

- **Transcripts:**
  - Two synthetic role-plays written for this spike, each with a facts checklist and a set of traps: a social
    care assessment (61 min; 35 facts, 16 traps) and a legal interview (46 min; 29 facts, 16 traps).
  - Four AMI Meeting Corpus buckets of 15, 45, 60 and 90 minutes. These have no checklist, so they are scored
    only automatically.
- **Strategies:** all three are in `run.py`.
  - **single:** the whole transcript in one call.
  - **mapreduce:** facts from each ~10-minute section (map), then one call composes the template (reduce).
  - **sectioned:** the map step only; the note is the facts grouped by template section.

  For "during recording" timings, the maps run while the conversation continues, so only the last section's map
  (and, for mapreduce, the reduce) is left after stop.

## Commands

```bash
cd spikes/s2-llm-notes
./bench.sh <model> ...                                        # llama-bench, 4 and 8 threads
T=synthetic_care_assessment,synthetic_legal_interview
python run.py --run screen-m5 --models <models> --transcripts $T   # screen (run as 3 parallel workers, merged)
python score.py auto --run screen-m5
python score.py packets --run screen-m5                       # 36 blind packets
# ...4 blind judges wrote results/judge/screen-m5/<id>.verdict.json...
python score.py collect --run screen-m5
python run.py --run full --models qwen3-4b,qwen3.5-4b \
  --transcripts ami-15,ami-45,ami-60,ami-90,$T                # timed, one model at a time
python score.py auto --run full
python memory.py <model> 8192; python memory.py <model> 32768 # peak memory
python report.py --run screen-m5 --run full                   # all tables
```

Raw data is in `spikes/s2-llm-notes/results/`: `screen-m5.jsonl`, `full.jsonl`, the matching `.scores.jsonl`
files, `memory.jsonl`, the notes under `notes/`, and the packets, verdicts and model key under `judge/screen-m5/`.

## Quality screen (all six models)

Each note was judged blind. The 36 packets (6 models × 3 strategies × 2 transcripts) hid the model and strategy
behind a hash. Four Claude judges each took 9 packets from one transcript, in shuffled order, so every judge saw
a mix of models. They followed `judge_instructions.md`, checking each fact against the transcript.

- **Fact recall:** present = 1, partial = 0.5.
- **Fully correct:** counts only "present".
- **Unsupported:** note sentences the transcript doesn't support and that no trap covers.
- **Weak support:** the automatic share of sentences whose words barely overlap their cited lines.

| Model | Strategy | Fact recall, care / legal | Fully correct (mean) | Traps hit | Unsupported sentences | Weak support (mean) |
|---|---|---|---:|---:|---:|---:|
| Qwen3-4B-Instruct-2507 | sectioned | 0.74 / 0.71 | 0.58 | 0 | 13 | 0.23 |
| Qwen3.5-4B | sectioned | 0.70 / 0.67 | 0.53 | 2 | 14 | 0.14 |
| Ministral-3-3B | sectioned | 0.70 / 0.66 | 0.50 | 2 | 36 | 0.37 |
| Qwen3-4B-Instruct-2507 | single | 0.66 / 0.64 | 0.36 | 0 | 13 | 0.19 |
| Qwen3.5-4B | single | 0.54 / 0.48 | 0.30 | 0 | 3 | 0.12 |
| Gemma-4-E4B | single | 0.49 / 0.48 | 0.27 | 1 | 0 | 0.05 |
| Phi-4-mini | sectioned | 0.46 / 0.43 | 0.11 | 1 | 12 | 0.32 |
| SmolLM3-3B | sectioned | 0.46 / 0.40 | 0.17 | 0 | 37 | 0.45 |
| Ministral-3-3B | single | 0.43 / 0.40 | 0.14 | 0 | 22 | 0.33 |
| Qwen3-4B-Instruct-2507 | mapreduce | 0.29 / 0.50 | 0.26 | 0 | 7 | 0.18 |
| Ministral-3-3B | mapreduce | 0.41 / 0.34 | 0.13 | 1 | 22 | 0.38 |
| Phi-4-mini | mapreduce | 0.29 / 0.34 | 0.08 | 0 | 9 | 0.35 |
| Qwen3.5-4B | mapreduce | 0.26 / 0.29 | 0.17 | 1 | 9 | 0.15 |
| SmolLM3-3B | mapreduce | 0.14 / 0.15 | 0.05 | 0 | 12 | 0.38 |
| SmolLM3-3B | single | 0.09 / 0.19 | 0.07 | 0 | 10 | 0.61 |
| Phi-4-mini | single | 0.11 / 0.07 | 0.01 | 0 | 2 | 0.75 |
| Gemma-4-E4B | mapreduce | 0.09 / 0.09 | 0.05 | 0 | 0 | 0.12 |
| Gemma-4-E4B | sectioned | 0.09 / 0.09 | 0.05 | 0 | 0 | 0.04 |

**Findings:**
- **Sectioned notes have the highest recall, but they are long.** For the Qwen, Ministral and SmolLM models
  they run to 130–194 sentences, against 35–60 for a single pass. Unsupported sentences per 100 are also low for them: about 4 for Qwen3-4B and 4–5
  for Qwen3.5-4B, against 11 for Ministral and about 18 for Qwen3-4B's single pass.
- **The reduce step is where facts get lost.** Mapreduce starts from the same facts as sectioned but keeps
  about half as many. The template has room for fewer sentences, and the model drops detail to fit.
- **Qwen3-4B and Qwen3.5-4B lead on every view of the data.** Their lead holds whether "partial" counts or not,
  and under each of the four judges.
- **Ministral-3-3B is close on recall,** but it produces about 2.5 times as many unsupported sentences
  (wrong years, wrong names, events that didn't happen).
- **Gemma-4-E4B's map calls ran away.** 11 of its 12 map calls hit the 2,048-token output cap and
  had to be salvaged, which is why its sectioned and mapreduce rows are near zero. Its single pass had no
  unsupported sentences at all. A bigger cap or a different prompt might rescue it, but it was not re-run.
- **Phi-4-mini and SmolLM3-3B often copied transcript lines verbatim,** sometimes repeating one line many
  times, and attributed speech to the wrong person.
- **Errors that recur across models:**
  - a hypothetical stated as fact: "she lost consciousness", which she denied (legal T04);
  - a corrected value kept: lunch club "on Wednesdays", which she corrected to Thursdays (care T11);
  - one person's view given to another;
  - next-step dates moved by a month.

  These are S3's territory.
- **Every Qwen, Ministral, Phi and SmolLM call returned valid JSON** under the schema constraint.

**Judging caveats:**
- The judges differed on borderline calls, and each reported its own rule. For example, a fact with one wrong
  date counted as "absent" for one legal judge and "partial" for the other. One care judge read a scratch copy
  of the transcript that another session overwrote, noticed, and re-read the affected lines.
- The packets were shuffled across models, so these differences add noise rather than bias. The two Qwen models
  lead whether "partial" counts as 0 or as 1.
- A human spot-check of 4 packets is pending: `1dd69ceb`, `9707b866`, `fd583e34` and `bc506e41` in
  `results/judge/screen-m5/`.

## Timing (finalists)

The run ran overnight with one model loaded at a time. Qwen3-4B ran first while the machine was still busy
(load average 17 at its start). Qwen3.5-4B ran later (load about 5). **This makes the speeds of the two
models not directly comparable**, and Qwen3-4B's figures are pessimistic.

| Model | Transcript | Single pass | Map-reduce | Sectioned, all after stop | **Sectioned, during recording** | Slowest 10-min section | Weak support, sectioned |
|---|---|---:|---:|---:|---:|---:|---:|
| Qwen3-4B | AMI (15 min) | 3.0 min | 8.8 min | 4.2 min | **2.0 min** | 2.2 min | 0.22 |
| Qwen3-4B | AMI (45 min) | 26.6 min | 24.0 min | 17.5 min | **2.1 min** | 4.2 min | 0.36 |
| Qwen3-4B | AMI (60 min) | 22.6 min | 22.4 min | 16.4 min | **2.7 min** | 3.1 min | 0.29 |
| Qwen3-4B | AMI (90 min) | 47.0 min | 37.7 min | 26.0 min | **2.7 min** | 3.5 min | 0.28 |
| Qwen3-4B | Care assessment (61 min) | 30.2 min | 24.2 min | 17.2 min | **39 s** | 3.0 min | 0.20 |
| Qwen3-4B | Legal interview (46 min) | 15.0 min | 18.5 min | 12.3 min | **2.2 min** | 2.6 min | 0.26 |
| Qwen3.5-4B | AMI (15 min) | 1.6 min | 3.1 min | 2.3 min | **65 s** | 71 s | 0.38 |
| Qwen3.5-4B | AMI (45 min) | 7.5 min | 10.3 min | 8.6 min | **58 s** | 2.3 min | 0.38 |
| Qwen3.5-4B | AMI (60 min) | 7.3 min | 11.4 min | 9.3 min | **1.8 min** | 2.0 min | 0.10 |
| Qwen3.5-4B | AMI (90 min) | 13.7 min | 18.3 min | 15.0 min | **83 s** | 2.1 min | 0.12 |
| Qwen3.5-4B | Care assessment (61 min) | 9.9 min | 14.0 min | 10.8 min | **14 s** | 1.9 min | 0.12 |
| Qwen3.5-4B | Legal interview (46 min) | 5.0 min | 11.4 min | 8.2 min | **85 s** | 1.7 min | 0.16 |

- **After stop, only the last section remains,** so the wait depends on how much of the last section was
  recorded, not on the length of the recording. A full final section is the worst case: up to 2.3 min for
  Qwen3.5-4B and 4.2 min for Qwen3-4B under load.
- **Processing keeps up with recording.** The slowest 10-minute section took 4.2 minutes. This was not measured
  with live ASR running at the same time; S1 measured live ASR at about 6% CPU on this machine.
- **Weak support did not rise with length** on the AMI meetings (Qwen3.5-4B: 0.38 at 15 and 45 min, 0.10–0.12
  at 60 and 90 min). This says nothing about omissions, which were not measured by length (condition 6).
- **Medians across all calls:**
  - Qwen3-4B: prefill 53 tok/s, generation 16 tok/s.
  - Qwen3.5-4B: prefill 89 tok/s, generation 26 tok/s.

  The `llama-bench` runs (`bench-*.jsonl`) were also taken under load and vary by up to 2.5× between repeats,
  so they don't settle the speed question either.
- **5 of the 68 map calls hit the 2,048-token cap** (both models, the 45- and 90-minute AMI meetings) and were
  salvaged. The product needs a larger cap, about 3k tokens, which still fits in an 8k context, or shorter
  sections.

## Memory

`run.py`'s `peak_rss_mb` uses `ps` RSS. On macOS that number counts memory-mapped weights and shifts with memory
compression, so the 6–11 GB it shows are not real peaks (S1 found the same). `memory.py` instead loads the
weights into ordinary memory and reads `footprint`'s physical-footprint peak for one map call (about 3.2k
prompt tokens):

| Model | Context | Idle after load | Peak during a map call |
|---|---:|---:|---:|
| Qwen3-4B | 8,192 | 4.07 GB | **4.68 GB** |
| Qwen3-4B | 32,768 | 7.41 GB | 8.08 GB |
| Qwen3.5-4B | 8,192 | 3.79 GB | **4.40 GB** |
| Qwen3.5-4B | 32,768 | 4.54 GB | 5.15 GB |

- **An 8k context is enough for the sectioned strategy:** a map prompt is about 3.3k tokens, plus up to 3k of
  output.
- **At 32k context the two models differ.** Qwen3-4B's KV cache grows by about 3.3 GB, while Qwen3.5-4B's hybrid
  attention adds only about 0.75 GB. Single pass on long transcripts needs that 32k context.
- **With live ASR loaded,** the total is about 4.4–4.7 GB for the LLM plus 1.1–1.3 GB for live ASR (S1),
  so **about 6 GB**, before the app itself. That fits a 16 GB laptop. The 8 GB target is tight, and needs
  measuring on the reference laptop.

## Changes to the PRD

- **Notes are drafted during the recording, one section at a time.** After stop, only the final section and
  the save remain. Imported recordings have no "during". Process them in the background with progress shown:
  about 9–11 minutes for 60 minutes of audio on this machine with Qwen3.5-4B.
- **The LLM and ASR are loaded together while recording,** with the LLM at an 8k context. This replaces "never
  resident together". The memory budget is about 6 GB.
- **No single "compose" pass over the whole meeting.** The first version of the note is the section facts
  grouped by template heading. Turning that into shorter prose without losing facts is open work for P2.
- **Map output cap of about 3k tokens,** or sections shorter than 10 minutes.
- **The candidate weights are Qwen3-4B-Instruct-2507 and Qwen3.5-4B** (Apache-2.0, Q4_0). S3 and S4 pick one.

## Still to do

1. Repeat the finalists' timings and memory on the reference Windows laptop, with live ASR running alongside.
2. Measure fact recall at 15 vs 60 minutes on the same transcript (condition 6).
3. A human spot-check of 4 judged packets (listed above).
4. S3: citation validity and the unsupported-sentence flag, starting from the traps these notes fell into.
5. S4: safeguarding refusals for both finalists.
6. Optional: re-run Gemma-4-E4B with a bigger map cap. Its single pass made nothing up.

## History

The first, interim run (before 2026-10-01, `results/screen.jsonl`) covered only Qwen3-4B on the care assessment,
with map-reduce and sectioned. Its numbers (prefill about 55 tok/s; 38 s after stop for sectioned during
recording) agree with this run.
