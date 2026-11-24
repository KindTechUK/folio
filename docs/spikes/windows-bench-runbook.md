# Runbook: the speech-to-text bench on a Windows laptop

> Purpose: measure accuracy, speed and memory on the machines our users have, not on a Mac or a CI
> runner. Background: [`../research/voice2text-lessons-2026-11.md`](../research/voice2text-lessons-2026-11.md)
> (what to test) and [`../research/windows-device-tiers-2026-11.md`](../research/windows-device-tiers-2026-11.md)
> (which machines). Tool: [`tools/asr-bench`](../../tools/asr-bench/README.md).

## 1. Machines

Buy in this order; the first is the one that matters.

| # | Machine | Why | Approx. cost |
|---|---|---|---|
| 1 | **Refurbished Dell Latitude 3400, i5-8265U, 8 GB (1×8 GB), SSD** | The typical donated or refurbished charity laptop: the Essentials floor | ~£110 (charity refurbisher) |
| 2 | New Intel N150 laptop, 8 GB | The cheapest new machine: 4 E-cores, single-channel RAM | ~£250–350 |
| 3 | Lenovo V15 G4 or ASUS ExpertBook P1, i5-13420H, 8 GB | What an SME buys new today, no NPU | £333–£476 ex VAT |
| 4 | ThinkPad E14 Gen 7, Core Ultra 5 225U, 16 GB | Standard tier with a 12 TOPS NPU | £727–£764 ex VAT |

Record each machine's exact model, CPU, RAM layout (Task Manager → Performance → Memory → "Slots used")
and Windows build before the first run.

## 2. Prepare the laptop

1. Windows 11 Pro, fully updated, then **pause updates for 1 week** (Settings → Windows Update).
2. Mains power. Power mode **Best performance** (Settings → System → Power). Screen and sleep: **Never**.
3. Close everything that starts with Windows except Defender. Don't add Defender exclusions: users won't have them.
4. Reboot, wait 5 minutes for start-up work to settle, then open a terminal.

## 3. Copy across (USB stick, no network needed)

Built on a Mac, copied to `C:\bench\` on the laptop:

- `asr-bench.exe`: from the **bench-build** GitHub Actions workflow (Actions → bench-build → Run
  workflow → download the `asr-bench-windows-x64` artifact).
- `pack\`: the bench pack (`uv run --script tools/asr-bench/pack/build_pack.py <dir>`, then delete
  its `.cache`). About 0.5 GB.
- `models\`: `silero_vad.onnx` and one folder per model from the sherpa-onnx `asr-models` release
  (`tools/asr-bench/README.md` §2). Parakeet v2 is required; the candidates are below.

Then check the binary and the machine:

```powershell
cd C:\bench
.\asr-bench.exe machine
.\asr-bench.exe score "twenty five dollars" '$25'
$env:FOLIO_MODELS_DIR = "C:\bench\models"
```

## 4. Run the cells

Each command writes `results\<tag>.jsonl` and resumes if interrupted, so a cell can be stopped and
restarted. Run **one cell at a time**, never two in parallel: they would share the CPU and spoil each
other's timings. On the floor machine a long-form cell takes roughly 30–60 minutes (170 minutes of
audio at RTF 0.15–0.3; to be measured).

Shorthand: `$P = "C:\bench\pack"`, `$B = ".\asr-bench.exe"`.

**A. Baseline: what the app does today** (always first)

```powershell
& $B run --pack $P --set ami-sdm
& $B run --pack $P --set ami-ihm
& $B run --pack $P --set earnings22
& $B run --pack $P --set ami-sdm-long
& $B run --pack $P --set earnings22-long
```

**B. Decoding cells D1–D4** (Parakeet v2; from `voice2text-lessons` §1b)

| Cell | Question | Command additions | Sets |
|---|---|---|---|
| D1 | Does VAD gating lose speech? | `--segmenter vad-pad:0.8/120/300`, `--segmenter energy:30/5` | ami-sdm-long, earnings22-long |
| D2 | How long should pieces be? | `--segmenter energy:15/3`, `energy:60/5`, `energy:120/10` | ami-sdm-long |
| D3 | Live utterances or re-decode? | `--segmenter vad:0.8/15` (the live profile) | ami-sdm-long, earnings22-long |
| D4 | Greedy or beam? | `--decoding beam:4`, `--decoding beam:8` | ami-sdm, earnings22 |
| threads | How many recognizer threads? | `--threads 2`, `--threads 8` (on a 4-core/8-thread CPU) | ami-sdm-long |

**C. Candidate models** (default segmenter; add the winning D1 segmenter as a second row if it differs)

| Model (`--model`) | Download folder | Watch for |
|---|---|---|
| `parakeet-tdt-0.6b-ultra` | `sherpa-onnx-nemo-parakeet-tdt-0.6b-ultra-int8` | A drop-in for v2 if it wins |
| `parakeet-unified-en-0.6b` | `sherpa-onnx-nemo-parakeet-unified-en-0.6b-int8-non-streaming` | Read its licence before shipping |
| `parakeet-tdt-0.6b-redux` | `sherpa-onnx-nemo-parakeet-tdt-0.6b-redux-int8` | Smaller; for 8 GB machines |
| `qwen3-asr-0.6b` | `sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25` | RTF on this CPU; the `timed` column (word timestamps) |
| `cohere-transcribe` | `sherpa-onnx-cohere-transcribe-14-lang-int8-2026-04-01` | RTF and peak memory (1.7 GB weights); `timed` |

Run each on `ami-sdm`, `ami-ihm` and `earnings22` first. Only models that beat v2 there go on to the
long-form sets.

**D. Background load** (the realistic case)

Repeat the A baseline for `ami-sdm-long` with a Teams call running (join a test meeting alone, camera
off) and Edge open on 10 ordinary tabs. Use a distinct tag:

```powershell
& $B run --pack $P --set ami-sdm-long --tag ami-sdm-long_v2_teams-load
```

While it runs, take one Task Manager screenshot (Performance tab: CPU and Memory) at the halfway mark.
The bench records its own peak memory; the screenshot shows what else was using the machine.

## 5. Bring the results back

Copy `C:\bench\results\` off the laptop into `tools/asr-bench/results/<machine-name>/`, which git
ignores. Then report on any machine:

```sh
B=tools/asr-bench/target/release/asr-bench
$B report --pack ~/folio-bench-pack --baseline ami-sdm_parakeet-tdt-0.6b-v2_vad-0.8-120_greedy \
  --out latitude-3400.md tools/asr-bench/results/latitude-3400/*.jsonl
```

Each table row carries the machine it ran on. Write the findings into a spike doc next to this one,
with the report tables and the machine details from §1.

## What a result should look like

These numbers are from an Apple M5 Pro with sherpa-onnx 2.0.1, recorded 2026-11-22. The full table is in
[`voice2text-lessons` §1b](../research/voice2text-lessons-2026-11.md#first-results-apple-m5-pro-2026-11-22-parakeet-v2-tools-asr-bench).

| Set | `vad:0.8/120` (today) | `energy:30/5` |
|---|--:|--:|
| ami-sdm-long (3 meetings, room mic) | 38.8% | 25.3% |
| earnings22-long (6 calls) | 8.6% | 7.3% |

The laptop's WER should match the Mac's closely, because it's the same model and decoder; check the
`empty` column stays near the Mac's. Speed and memory are what the laptop is for, especially peak memory
on 8 GB with the background load from §4D.

**Build `asr-bench.exe` from a branch that uses sherpa-onnx 2.0.1 or later.** 1.13.2 returns empty
text for some speech (upstream #3997), which the `empty` column and a high deletion share give away.
