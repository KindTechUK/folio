# Spike: sherpa-onnx ASR + speaker diarization on CPU (de-risk the engine choice)

> Date: 2026-06-10 · Time-boxed half-day spike · Informs the PRD at
> `docs/prd/local-private-transcription.md` ("Diarization specifics", "Further Notes").
> Goal: measure the *inferred* sherpa-onnx engine choice (Python-free, CPU-only, no HF gating)
> on real multi-speaker audio and return an honest GO / NO-GO / GO-WITH-CONDITIONS verdict.

## TL;DR verdict

**GO-WITH-CONDITIONS.**

- **ASR works and is fast on CPU.** small.en RTF **0.11**, medium.en RTF **0.36** on a 5-min real
  podcast slice (Mac arm64, 4 threads, int8). Both well under realtime.
- **Diarization is fast** (RTF ~0.05–0.06) and its *structure* matches pyannote when the speaker
  count is known.
- **The biggest risk is auto speaker-count.** With the default cluster threshold, auto-count
  **over-segmented to 12 speakers** vs pyannote's 3 in a clean 3-speaker window; even tuning the
  threshold up to 0.9 still gave 5. Given the **known** count (=3) the result was clean and
  correctly shaped. This matters much less for the firm's real 1–2 speaker case, but the auto-detect
  user story (#32) needs guarding.
- **Released Whisper ONNX models do not emit token timestamps** (no cross-attention outputs). We
  fall back to spreading the VAD-chunk span across words — which is exactly the PRD's "timestamps
  are a playback aid only / re-spread span" model, so this is acceptable, not a blocker.
- **Licenses are all clean** (Apache-2.0 / MIT). No gated weights, no HF token needed.
- **Integration proven:** the sherpa sidecar flowed through the committed core `buildTurns`
  unchanged → **18 turns, 3 speakers**, monotonic. Core suite restored to its **69-test** baseline.

**Recommended shipping default: Whisper small.en (int8) for ASR**, with medium.en offered as a
"higher accuracy / slower" model pack. See extrapolation below — medium.en may be too slow on a
weak office laptop for the impatient-fee-earner case.

---

## Environment

- macOS arm64 (darwin 24.6), Apple Silicon. **This is a fast proxy for the CPU-only Windows target.
  A typical UK-law-firm office laptop (e.g. an i5/i7 U-series, or a constrained VM) will be
  meaningfully SLOWER — assume 2–4× the RTF below. The Mac numbers here are NOT the shipping number.**
- ffmpeg 8.0 on PATH. CPU only — no CUDA/GPU anywhere (`provider="cpu"`).
- Clean venv via `uv` (kept separate from the existing whisperx/torch `.venv`).
- 4 decode threads (`num_threads=4`).

## Setup — exact commands

```bash
# Clean, isolated venv (does not touch the existing whisperx .venv)
uv venv .venv-sherpa --python 3.11
uv pip install --python .venv-sherpa/bin/python sherpa-onnx numpy
# -> sherpa-onnx==1.13.2 (Apache-2.0), numpy==2.4.6

# Cut a representative 5-min slice (16 kHz mono WAV)
ffmpeg -ss 120 -t 300 -i "downloads/Scams & Scandals, Are We Dating the Same Fraudster？.m4a" \
       -ar 16000 -ac 1 -y models/slice_120_420.wav
```

### Slice window
- **[120s, 420s]** (00:02:00 – 00:07:00) of the 25.8-min podcast.
- Chosen from `out/fraudster_v3.json`: a clean **3-speaker** conversational stretch
  (SPEAKER_03 ~140s, SPEAKER_05 ~54s, SPEAKER_09 ~34s) — deliberately avoiding the
  6–7-speaker churn windows elsewhere in the clip.

## Models — names, URLs, on-disk sizes, licenses

All from the `k2-fsa/sherpa-onnx` GitHub releases (no Hugging Face token, no gating).

| Role | Asset | Release tag | On-disk (used) | License |
|---|---|---|---|---|
| ASR (default rec.) | `sherpa-onnx-whisper-small.en` | `asr-models` | encoder int8 **113M** + decoder int8 **256M** + tokens 816K | **MIT** (OpenAI Whisper) |
| ASR (higher acc.) | `sherpa-onnx-whisper-medium.en` | `asr-models` | encoder int8 **369M** + decoder int8 **561M** + tokens 816K | **MIT** (OpenAI Whisper) |
| Diarization seg. | `sherpa-onnx-pyannote-segmentation-3-0` (`model.onnx`) | `speaker-segmentation-models` | **5.7M** | **MIT** (pyannote/segmentation-3.0, CNRS) |
| Speaker embedding | `3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx` | `speaker-recongition-models` *(repo's spelling)* | **28M** | **Apache-2.0** (3D-Speaker / ModelScope) |
| VAD (chunking) | `silero_vad.onnx` | `asr-models` | 632K | **MIT** (Silero) |

Download URLs (pattern):
```
https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-small.en.tar.bz2
https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-medium.en.tar.bz2
https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2
https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx
https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx
```
Note the ASR tarballs ship **both** fp32 and int8 ONNX (small.en tarball 636MB, medium.en 1.9GB
download). We used **int8** for CPU. **License-vetting: PASS** — everything is MIT/Apache-2.0,
nothing CC-BY-NC and nothing gated. (Contrast the rejected Rev `reverb-diarization` in the PRD.)

---

## Results

### ASR (RTF = processing_time / audio_duration; 300s slice, 4 threads, int8, CPU)

| Model | Disk (int8 enc+dec) | proc time | **RTF** | words |
|---|---|---|---|---|
| **small.en** | ~370M | 32.7s | **0.109** | 738 |
| **medium.en** | ~930M | 107.2s | **0.357** | 728 |

Transcription quality (spot-check) is clean and readable for both on this conversational audio.
Long audio is handled by **Silero-VAD chunking** (Whisper truncates at 30s; we segment into
≤25s speech chunks and offset timestamps) — 92 speech chunks for the slice.

**Token-timestamp caveat (finding):** the released Whisper ONNX decoders were exported *without*
cross-attention outputs, so `enable_token_timestamps=True` produces no per-token times
(sherpa logs a warning). We therefore derive word times by spreading each VAD chunk's [start,end]
uniformly across its words. This is consistent with the PRD's contract ("word-level timestamps are
a playback aid only… re-spread the span across words"). To get *true* per-word times we'd re-export
with `scripts/whisper/export-onnx-with-attention.py` (needs torch) — out of scope for this spike,
flagged as a known follow-up if word-highlight precision proves too coarse in the editor.

### Diarization (300s slice, 4 threads, CPU)

| Mode | proc time | RTF | detected speakers | vs pyannote (3 in window) |
|---|---|---|---|---|
| **auto** (threshold 0.5) | 17.7s | 0.059 | **12** | wildly over-segmented |
| auto threshold 0.7 | — | — | 7 | still over |
| auto threshold 0.9 | — | — | 5 | still over |
| **known count = 3** | 15.9s | 0.053 | **3** | clean, correct shape |

Diarization is cheap (RTF ~0.05). The headline problem is **auto speaker-count over-segmentation**
on this overlapping multi-speaker podcast.

### sherpa (known=3) vs pyannote, same window

| | Dominant | Mid | Minor |
|---|---|---|---|
| pyannote (3 spk) | 140s | 54s | 34s |
| sherpa known=3 | 195s | 42s | 16s |

Same **gross structure** (one dominant + two minor speakers). sherpa's dominant speaker absorbs
more time (195 vs 140s) — it folds some short interjections into the main speaker rather than
splitting them out. Turn *boundaries* are broadly aligned; sherpa is not catastrophically
mis-segmented when given the count. pyannote is the **incumbent reference, not ground truth** — the
human relabel step is the accuracy backstop per the PRD.

### Integration: sherpa sidecar → core `buildTurns`

- Merged medium.en ASR words + known-3 diarization spans (per-word speaker by max overlap, segment
  speaker by duration-weighted majority) into our contract shape →
  `out/fraudster_sherpa.json` (`{segments:[{start,end,text,speaker,words:[{word,start,end,speaker}]}]}`).
- A throwaway vitest (`core/scripts/spike.test.ts`) read that sidecar, called `buildTurns` from the
  committed `../src/index.js`, and asserted non-empty + monotonic turns. **Green.**
  Output: **18 turns, speakers {SPEAKER_00, SPEAKER_01, SPEAKER_02}.**
- Throwaway test **deleted**; committed core suite restored to **69 tests / 5 files (all pass)**.
  No committed core source or test was modified. **The Python-free engine output flows through the
  existing pipeline unchanged.**

---

## Honest verdict: GO-WITH-CONDITIONS

The engine choice is sound. ASR and diarization both run comfortably faster than realtime on CPU,
all weights are MIT/Apache (no gating, no HF token — a real win for the airplane-mode story), and
the output drops straight into the existing core with zero changes. **GO** on the architecture.

**Conditions:**
1. **Do not ship blind auto speaker-count.** Default cluster threshold over-segments (12 vs 3 here).
   Either (a) expose a "number of speakers" hint in the UI (user story #32 already anticipates an
   optional explicit count), defaulting to a conservative threshold; or (b) tune + validate the
   threshold on the firm's *own* 1–2-speaker audio before relying on auto. For the firm's real case
   (dictation / 1–2-party client calls) over-segmentation is far less likely to bite, and the
   relabel step cleans up the rest — but the multi-party interview case needs the hint.
2. **Confirm word-highlight feel is acceptable with spread-from-VAD-chunk timestamps**, or commit to
   re-exporting Whisper with attention outputs for true per-word times. Native ±200–500ms is the
   PRD's stated tolerance; this likely passes, but verify in the actual karaoke editor.
3. **Pick the default model by laptop class.** small.en is the safe default; medium.en as an
   opt-in higher-accuracy pack.

**Biggest risk:** speaker **misattribution** via auto-count over-segmentation on multi-party audio —
exactly the risk the PRD flagged. Mitigated by (a) the firm's audio skewing 1–2 speakers and
(b) the human relabel backstop.

### Extrapolation to a Windows office laptop

Combined pipeline RTF on this Mac: small.en ≈ 0.11 + 0.06 = **~0.17**; medium.en ≈ 0.36 + 0.06 = **~0.42**.
A locked-down office laptop CPU will likely be **2–4× slower**. Conservative Windows estimates:

| Recording | small.en (RTF ~0.34–0.68) | medium.en (RTF ~0.84–1.7) |
|---|---|---|
| 30 min | **~10–20 min** | ~25–50 min |
| 60 min | **~20–40 min** | ~50–100 min (around/over realtime) |

**A fee-earner would tolerate small.en** for a 30–60 min recording (10–40 min, run-and-go, like a
batch job — acceptable for a non-interactive "process this file" step, especially vs the incumbent
"~an hour of CPU time"). **medium.en on a weak laptop risks approaching or exceeding realtime for a
60-min file**, which is the line where impatience sets in. → **Ship small.en as default**; medium.en
as an explicit "slower, more accurate" pack. Numbers must be re-measured on real target hardware
before promising turnaround times to the buyer.

---

## Reproduction

```bash
cd ~/kindtech-transcribe
uv venv .venv-sherpa --python 3.11
uv pip install --python .venv-sherpa/bin/python sherpa-onnx numpy

# Slice
ffmpeg -ss 120 -t 300 -i "downloads/Scams & Scandals, Are We Dating the Same Fraudster？.m4a" \
       -ar 16000 -ac 1 -y models/slice_120_420.wav

# Models (into models/): the 5 assets listed above (tar xjf the tarballs).

cd models
# ASR (script: models/asr_spike.py)
../.venv-sherpa/bin/python asr_spike.py sherpa-onnx-whisper-small.en  small.en  4 slice_120_420.wav asr_small.json
../.venv-sherpa/bin/python asr_spike.py sherpa-onnx-whisper-medium.en medium.en 4 slice_120_420.wav asr_medium.json
# Diarization (script: models/diar_spike.py)
../.venv-sherpa/bin/python diar_spike.py slice_120_420.wav 4 diar_auto.json       # auto count
../.venv-sherpa/bin/python diar_spike.py slice_120_420.wav 4 diar_known3.json 3   # known count
# Merge into our contract (script: models/merge_spike.py)
../.venv-sherpa/bin/python merge_spike.py asr_medium.json diar_known3.json ../out/fraudster_sherpa.json

# Integration proof (throwaway, then delete):
# create core/scripts/spike.test.ts -> reads out/fraudster_sherpa.json, calls buildTurns
cd ../core && npx vitest run scripts/spike.test.ts   # -> {"turns":18,"speakers":[...3...]}
rm scripts/spike.test.ts && rmdir scripts
npx vitest run   # -> 69 tests pass (baseline restored)
```

Spike scratch scripts live in `models/` (`asr_spike.py`, `diar_spike.py`, `merge_spike.py`) and the
`models/` dir + `.venv-sherpa` are gitignored. Nothing committed.
