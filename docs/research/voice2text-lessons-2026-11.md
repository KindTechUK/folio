# Lessons from voice2text: model eval, vocabulary, and learning vocabulary (Nov 2026)

> Recorded 2026-11-20. Read from `lucharo/voice2text` @ f151cd8 (source, `internal/`, `utils/`), and the
> k2-fsa sherpa-onnx `asr-models` release and the `sherpa-onnx` 1.13.8 crate Folio already links.
> Purpose: decide what to borrow for (1) Folio's speech-to-text eval bench before the Windows
> reference-laptop runs, (2) a custom vocabulary, and (3) proposing vocabulary without making users
> type it in. v2t's accuracy numbers are its maintainer's Mac runs (MLX / Core ML). They have not been
> rerun on Folio's int8 CPU path.

## Summary

- **Eval.** v2t's ASR grid is much more rigorous than our `live_replay` WER, and most of its method is
  cheap to copy: the Open ASR Leaderboard scorer, seeded 150-clip samples with bootstrap intervals, an
  AMI **room-microphone** set, and a model registry that checks our scores against published ones.
- **Models.** Every model that beat or matched Parakeet in v2t's grid, except Bark and Phonon, now has
  an int8 sherpa-onnx export. Our existing crate has configs for Qwen3-ASR and Cohere Transcribe, so
  testing them needs no new runtime. The ones worth a cell on the Windows laptop are **Parakeet Ultra**
  (a drop-in), **Parakeet Unified EN**, **Qwen3-ASR 0.6B** and **Cohere Transcribe**.
- **Decoding.** v2t decodes every sample, whole or in ~30 s pieces cut at the quietest moment, with
  no VAD gating. Folio decodes only what Silero VAD calls speech, and keeps the live utterances as the
  record. Section 1b has six eval cells (D1–D6), from VAD gating through beam search and hotwords to a
  Nemotron or Unified streaming preview.
- **Mac results (2026-11-23).**
  - **VAD gating loses speech.** Energy cuts take the room-mic meetings from 38.8% to about 25% WER,
    and halve peak memory.
  - Piece length and beam search make no real difference.
  - Unified EN beats Parakeet v2 on short clips but not on long meetings.
  - Cohere is the most accurate, but has no word timestamps, which the editor's speaker labels and
    cursor need. So the default stays in the Parakeet family.
  - Ultra, v3, Redux and Qwen3-ASR drop out.
- **Vocabulary.** v2t has no ASR-level biasing. A plain-text dictionary holds two kinds of line:
  `heard => written` replacements, which are deterministic, and `Term` hints for the LLM cleanup. A
  word guard stops the LLM from adding words that were not said. Folio can do better at the ASR step:
  sherpa-onnx exposes hotwords, and Qwen3-ASR takes a hotwords prompt.
- **Proposing vocabulary.** v2t learns from corrections the user already makes, using its keyboard and
  an agent skill that turns a raw→corrected diff into entries and proves each one fires. Folio's review
  editor produces exactly that diff on every transcript, so it is a richer source than v2t has.

## 1. Speech-to-text eval

### What v2t does (`utils/asr_grid/`, `utils/model_intake/`)

| Practice | Detail | Folio today |
|---|---|---|
| Leaderboard scorer | Copies the Open ASR Leaderboard's normaliser and `kaldialign merge_compounds=True` alignment at a pinned commit (`utils/asr_grid/oasr/`, Apache-2.0). "to night" vs "tonight" costs nothing. On full LibriSpeech, their own earlier scorer read 1.90 / 3.64% where the leaderboard's read 1.48 / 3.14%. | Own lowercase/strip `wer()` in `live_replay.rs` |
| Sample + interval | 150 seeded clips per set, 95% interval from 4,000 bootstrap resamples (about ±1.5 points on LibriSpeech), and a **paired** bootstrap of each model against the baseline. Gaps inside the interval count as ties. | Two AMI meetings, point estimates |
| Sets matched to the question | `ami-sdm` (room mic, 3 meetings), `ami-ihm` (the leaderboard's headset AMI), `earnings22` (names, numbers, jargon), `ls-long` (long audio), and `-full` splits for close calls. | AMI Mix-Headset only |
| Model registry | `models.toml` holds each model's published WERs with sources, plus each engine/precision "system" with disk size and ship status. A notebook (`intake.py`) plots our scores against published ones, WER against wait, and WER against disk. | None |
| Fair timing | `race` runs systems in one process, rotating their order every clip, and logs load, free memory and disk. Cells run one at a time, and a cell spoiled by swapping is rerun. Peak memory is recorded per clip. | `live_replay` measures one model at a time |
| Unlabelled real audio | `silver.py` builds a consensus (medoid) reference from three strong systems. They note this **cannot measure vocabulary fixes**, so dictionary changes are judged on labelled sets only. | n/a |

The finding that matters most for Folio: **the same Parakeet v3 makes 12.1% errors on AMI headset
audio and 34.9% on the room microphone** (127 identical segments). A laptop on a table recording a
meeting is the room-mic case. Our S1 accuracy numbers (Mix-Headset, ~23%) sit between the two and
understate what a reviewer will see.

### Proposal for Folio's bench

1. Port the `oasr/` scorer (Apache-2.0, keep its NOTICE) and score with it. Keep our current scorer only
   to compare with S1's figures.
2. Sets: **`ami-sdm`** (primary, as it matches our use), `ami-ihm` (to compare with model cards),
   `earnings22` (closest public proxy for names and numbers in legal work), and long-form audio. Add a
   small **labelled vocabulary set** of names, case references and organisations for section 2, since
   silver references can't score those.
3. Report a bootstrap interval and a paired difference against Parakeet v2, the current default.
4. Add a small `models.toml`-style registry. When our int8 CPU score for a model is far from its
   published score, that flags a scoring or export bug before we trust a comparison.
5. On the Windows reference laptop, record RTF, peak private bytes and model load time per model,
   with the same interleaved timing as `race`.

### Candidate models (all int8 sherpa-onnx exports, from the `asr-models` release)

v2t grid numbers are on the leaderboard scorer: ami-ihm / earnings22-aa / ami-sdm where measured
(Mac, bf16 or fp16; 150 clips). Lower is better.

| Model | sherpa-onnx export | v2t grid | Why test it | Watch for |
|---|---|---|---|---|
| Parakeet TDT v2 (**current**) | `…-v2-int8` 482 MB | not in grid (EN-only) | Baseline | |
| Parakeet TDT v3 | `…-v3-int8` 487 MB | 8.92 / 6.07 / 34.9 | v2t's default; multilingual | Small English cost against v2 (6.3% vs 6.0% leaderboard average) |
| **Parakeet Ultra** | `…-0.6b-ultra-int8` 487 MB (2026-11-07) | **7.53** / 5.97 / – | Same architecture as v3, so it's a pure model-pack swap with native word timestamps | v2t ran a third-party MLX conversion; LS-other slightly worse than the card |
| **Parakeet Unified EN** | `…-unified-en-0.6b-int8-non-streaming` 501 MB, plus 240/560/1120 ms streaming | 7.80 / 6.66 / – | Strong on LS-other; one checkpoint also streams, which is relevant to S7 live text | Licence is "other": read it |
| **Qwen3-ASR 0.6B** | `qwen3-asr-0.6B-int8` 878 MB | 1.7B only: 7.60 / 5.77 / **29.3** | 1.7B was the best open model on the **room mic** in v2t's grid. The crate's config has a `hotwords` field | v2t never tested 0.6B. Autoregressive decoder, so check CPU RTF. Word timestamps probably absent (karaoke would fall back to the uniform spread used for Whisper) |
| **Cohere Transcribe** | `cohere-transcribe-14-lang-int8` 1.7 GB | **6.68** / 6.82 / – | Best AMI headset score in the grid | 2B params, so CPU speed and RAM are the question. Invents text on silence; Folio's Silero VAD in front should cover that, but test it. Timestamps as for Qwen |
| Parakeet Redux | `…-redux-int8` 327 MB | 8.53 / 7.84 / – | Fallback for 8 GB laptops | Slightly worse everywhere |
| Nemotron 3.5 streaming | `…-0.6b-{80…1120}ms-int8` 475 MB | 13.75 / 13.63 / – | Only if S7 needs native streaming | Clearly worse in v2t's grid |

Skip Whisper turbo (13.09 ami-ihm and slow), Bark 3.0 and Phonon-2 (no sherpa export).

**Nemotron, in three forms:**

- **Nemotron 3.5 ASR streaming** (row above): cache-aware, multilingual, and the weakest in the grid.
- **Nemotron-Labs Unified ASR 0.6B**: v2t **ships** it as the live preview (`transcription.preview =
  "nemotron"`, `v2t/nemotron.py`). It runs as a stream beside Parakeet and shows only the live words.
  The saved text still comes from Parakeet's offline pieces. Live text updated every 0.63 s, against
  1.13 s for Parakeet's own previews, and the saved text matched a direct decode 5 times out of 5. It
  runs on Core ML through FluidAudio, and **there's no sherpa-onnx export**. Its English counterpart in
  sherpa is Parakeet Unified EN streaming (240/560/1120 ms).
- **Nemotron Speech Streaming EN 0.6B**: our planned S7 fallback. v2t couldn't run it faithfully on MLX:
  `nemotron-asr-mlx`'s streaming path lacked the pre-encode cache, and the streamed WER was 45% against
  20% whole-file. That was a runtime bug, not a measure of the model. sherpa-onnx has int8 exports at
  80, 160, 560 and 1120 ms (April 2026).

The pattern to borrow for S7 is **streaming model for the live words, offline model for the record**.
It drops S1's "WER of live vs human" condition, because the live text is never the record.

The **Folio-specific constraint v2t doesn't have** is word timestamps for the karaoke cursor and
diarization mapping. The Parakeet family keeps them natively. For Qwen3-ASR and Cohere, check what
`OfflineRecognizerResult` returns before investing; they may only be worth it as a slow, more accurate
batch pass. Checked on 2026-11-23: neither returns word timestamps. See "Word timings constrain the model"
in section 1b.

## 1b. Decoding: how v2t does it, how Folio does it, what to evaluate

### v2t

- **No VAD gating on the Parakeet path.** A recording under 60 s is decoded whole
  (`WHOLE_FILE_S`). Longer ones are cut into **~30 s pieces at the quietest 100 ms within ±5 s of each
  mark** (`PieceDecoder`, `backends.py:102-196`), the splitter mlx-audio uses for Qwen3-ASR. Every
  sample is decoded; nothing is dropped as "not speech". On Core ML the pieces are 12 s ±3 s, because
  the encoder window is fixed at 15 s.
- **Pieces are decoded while recording**, so on stop only the last piece is left. Wait after release:
  p50 0.28 s.
- **They measured the choice.** On ls-long (40 labelled clips, 65–286 s), Parakeet v3 in pieces scored
  **2.3% WER against 5.3%** for the 5 s stream it replaced, and was faster on release.
- **Greedy TDT** everywhere (`coreml.py:tdt_greedy`, max 10 symbols per frame). No beam search and no
  biasing.
- **VAD only where a model needs it.** The `cohere-transcribe-vad` row decodes speech segments only,
  because Cohere invents text over silence.
- **Lead silence:** Bark gets 0.25 s of silence prepended, its card's evaluated setting.

### Folio

- **Silero VAD gates everything.** Only audio the VAD calls speech is decoded (`sherpa.rs:34`). For
  files the segments close after 0.8 s of silence, up to 120 s. Live capture uses 0.8 s / 15 s.
  `min_speech` is 0.25 s and the threshold is 0.5. We don't pad segment edges ourselves; check whether
  sherpa's VAD does.
- **Greedy search** (`sherpa.rs:195`).
- **The live utterances become the record.** On stop, the 0.8 / 15 s live utterances are the saved
  transcript (`recorder.rs:6-8`), with no re-decode.

The risk v2t avoids and we don't: **on a room microphone, quiet or distant speakers can fall under
the VAD threshold and disappear**. That shows up as deletions, which a reviewer won't notice because
there's no text to check. Some of the 35% room-mic WER could be ours rather than the model's.

### Decoding eval (on the bench from section 1, Windows laptop included)

Score every cell as WER **split into substitutions, deletions and insertions**, plus RTF and peak
memory. The sets are `ami-sdm`, `ami-ihm`, `earnings22` and long-form audio.

| # | Question | Arms | Decides |
|---|---|---|---|
| D1 | Does VAD gating lose speech? | Current VAD (0.8 / 120) · VAD with 0.2–0.3 s edge padding · v2t's energy cuts (30 s ±5 s, no gating) · energy cuts with VAD only to skip long silence | File-path segmentation. Watch deletions on `ami-sdm` |
| D2 | How long should pieces be? | 15 / 30 / 60 / 120 s caps | Accuracy against peak memory on 8 GB Windows laptops |
| D3 | Should the record come from a re-decode? | Keep the live utterances · re-decode the WAV on stop with the file profile | Record quality against time from stop to editor, on 30, 60 and 90-min recordings |
| D4 | Greedy or beam? | `greedy_search` · `modified_beam_search` (beam 4 and 8) | Whether beam pays for itself. It's required for hotwords |
| D5 | Do hotwords help? | D4's best · plus hotwords at 2–3 scores, on the labelled vocabulary set | Name recall against WER on ordinary words, and speed |
| D6 | Is a streaming preview better live? (S7) | Current live VAD · Parakeet Unified EN streaming · Nemotron Speech Streaming EN · Nemotron 3.5, each via `OnlineRecognizer`, preview only | Live update rate and CPU. Licence review for the NVIDIA models first |

### First results (Apple M5 Pro, 2026-11-22, Parakeet v2, `tools/asr-bench`)

**sherpa-onnx 1.13.2 returns empty text for some speech.** On some recordings, Parakeet returns
nothing for clear English once a segment passes about 12 s. Two of six earnings calls and one of
three meetings lost large stretches. Changing the volume flips the result, and v2, v3 and Ultra all
fail on the same audio. The cause is the front end's log floor: upstream
[#3997](https://github.com/k2-fsa/sherpa-onnx/issues/3997) (fixed in 2.0.0; also
[#2258](https://github.com/k2-fsa/sherpa-onnx/issues/2258)). The app's 120 s VAD segments hit it.
sherpa-onnx 2.0.1 decodes the same windows correctly. Today's file path, before and after the upgrade:

| Set | 1.13.2 | 2.0.1 |
|---|--:|--:|
| earnings22-long (6 calls) | 25.2% | 8.6% |
| ami-sdm-long (3 meetings, room mic) | 48.9% | 38.8% |
| ami-sdm (150 short clips) | 45.8% | 45.6% (clips are mostly under 12 s) |

**On 2.0.1, energy cuts beat VAD gating (D1)**, with the biggest gain on the room microphone:

| Segmenter | ami-sdm-long WER | deleted | earnings22-long WER | peak MB (AMI) | RTF (AMI) |
|---|--:|--:|--:|--:|--:|
| `vad:0.8/120` (today) | 38.8 | 34.1% | 8.6 | 3,360 | 0.040 |
| `vad-pad:0.8/120/300` | 37.1 | 31.8% | 7.8 | 3,589 | 0.040 |
| `vad:0.8/15` (live profile) | 38.7 | 33.7% | 8.9 | 2,200 | 0.037 |
| `energy:30/5` | **25.3** | 17.7% | **7.3** | **1,864** | 0.067 |
| `energy:15/5` | 24.9 | 16.3% | 7.7 | 1,455 | 0.070 |

- Energy cuts are better or level on **every one** of the 9 recordings. Per meeting: TS3003a 54.4 → 25.7,
  ES2004a 34.6 → 25.7, IS1009a 24.5 → 24.2. Per call: 4.1 → 1.7 down to 8.4 → 8.2.
- There are only 3 and 6 recordings, so there's no interval. Per-recording consistency is the evidence.
- Cost: energy cuts decode silence too, so RTF is higher (0.067 against 0.040 on the meetings). Peak
  memory is lower, about 1.9 GB against 3.4 GB, because no segment exceeds 35 s. That matters on 8 GB laptops.
- **D3:** the live profile (`vad:0.8/15`) scores the same as the file profile on 2.0.1. Re-decoding on
  stop is only worth it if the record moves to energy cuts.
- **Short clips:** `whole` decoding scores 30.6% against 45.6% for today's VAD on `ami-sdm`. The VAD
  returns nothing for 23 of 150 human-cut clips.

**Decided (2026-11-22):**
- Upgrade sherpa-onnx to 2.0.1 ([PR #23](https://github.com/KindTechUK/folio/pull/23)).
- Move the file path to energy cuts once the floor laptop's speed numbers are in, as a change of its
  own. Live capture keeps the VAD.

**Decided (2026-11-24), superseding the last point.** The room-mic research
(`room-mic-capture-2026-11.md`) showed that VAD gating is what loses speech when one laptop sits in
the middle of a room, so neither path gates the record any more:
- **Imports:** Parakeet decodes energy cuts (15 s pieces, cut at the quietest 100 ms within ±5 s,
  so 10–20 s long), without waiting for the floor laptop's numbers. Whisper, Qwen3 and Cohere keep
  a VAD: pieces include silence, which Whisper and Cohere fill with invented text, and Qwen3 has a
  short output budget.
- **Recordings:** with Parakeet, a recording's saved transcript is built from the same pieces, cut
  from the live stream and decoded in the background. The live captions stay on the VAD and are
  only a preview. With the other models the record stays the VAD utterances.
- **Why 15 s, not 30 s:** on a room recording made through Folio, Parakeet v2 returned only the
  last 2 s of a 30.5 s piece, at almost any gain, while 20 s and 25 s pieces of the same audio
  came back whole (`engine/tests/long_pieces.rs`). `energy:15/5` scores the same as `energy:30/5`
  on the bench above.
- **Every VAD** (live captions, and imports for the other models) hears a levelled copy of the
  audio (up to +30 dB towards −23 dBFS over 3 s), while the decoder gets the original. On Parakeet v2
  this takes the live profile from 45.8% to 36.5% WER on `ami-sdm` (−9.3 points, 95% CI
  [−13.7, −5.1]; empty clips 23 → 9) and from 38.7% to 27.4% on `ami-sdm-long`, with
  `earnings22-long` level (8.9% → 8.7%).

D1–D3 change no model and need no new crate. D4 and D5 are config changes plus a check that
`nemo_transducer` supports beam search and hotwords. D6 is the only one that needs a new code path.

### Full Mac grid (Apple M5 Pro, 2026-11-23, sherpa-onnx 2.0.1)

Same pack and scorer as above. The short sets have 150 clips each, with 95% bootstrap intervals. The long
sets are 3 meetings and 6 calls, too few for an interval. Peak memory moves by a few hundred MB between
runs, so compare it within one table only. Raw results are in `~/folio-bench-results` on the bench Mac.

**D2: piece length barely changes accuracy, but memory grows with it.** Parakeet v2, energy cuts with
a ±5 s search:

| Segmenter | ami-sdm-long WER | deleted | earnings22-long WER | peak MB (AMI) |
|---|--:|--:|--:|--:|
| `vad:0.8/120` (today) | 38.8 | 34.1% | 8.6 | 3,092 |
| `vad-pad:0.8/120/200` | 38.3 | 33.2% | 8.2 | 2,890 |
| `energy:15/5` | 24.9 | 16.3% | 7.7 | 1,455 |
| `energy:30/5` | 25.3 | 17.7% | 7.3 | 1,554 |
| `energy:60/5` | 24.9 | 17.8% | 7.1 | 1,947 |

- v2t's 30 s is a sound default.
- 15 s saves about 100 MB on the AMI meetings, if the 8 GB floor laptop is tight.
- 200 ms of VAD padding helps no more than the 300 ms above.

**D4: beam search gains nothing.** Parakeet v2 with today's VAD, `modified_beam_search` with beam 4
against greedy:

| Set | greedy | beam 4 | Δ [95%] |
|---|--:|--:|--:|
| ami-sdm | 45.6 | 46.9 | +1.3 [−1.3, +4.1] |
| ami-ihm | 16.9 | 16.1 | −0.8 [−2.2, +0.6] |
| earnings22 | 11.7 | 11.7 | 0.0 [−0.5, +0.5] |

- No difference on any set.
- It also cost nothing in speed on this Mac: RTF was the same or lower.
- So D5 can test hotwords on beam without a speed penalty to weigh. The Windows laptop still has to
  confirm the speed.
- Beam 8 wasn't run.

**Models, with every short clip decoded whole.** Each clip is decoded in one piece with no VAD, so this
table compares models rather than gating. The table gives WER, with Δ against v2 and its 95% interval.

| Model | ami-sdm | ami-ihm | earnings22 | RTF | peak MB | word timings |
|---|--:|--:|--:|--:|--:|--:|
| Cohere Transcribe | **24.4** (−6.2 [−10.4, −2.4]) | **7.1** (−2.7 [−5.4, −0.4]) | 8.8 (−0.9 [−1.7, −0.0]) | 0.14–0.17 | 3,600–4,400 | none |
| Parakeet Unified EN | 28.1 (−2.4 [−4.9, +0.1]) | 7.3 (−2.5 [−5.1, −0.3]) | **8.7** (−1.0 [−1.9, −0.2]) | 0.08 | ~1,400 | 93–98% |
| Parakeet v2 (current) | 30.6 | 9.9 | 9.7 | 0.08 | ~1,400 | 95–98% |
| Parakeet Ultra | 32.3 (+1.8 [−1.1, +4.6]) | 8.5 (−1.4 [−3.7, +0.8]) | 9.8 (+0.1 [−0.9, +1.1]) | 0.08 | ~1,400 | 95–98% |
| Parakeet v3 | 31.9 (+1.4 [−1.5, +4.2]) | 11.4 (+1.6 [−1.3, +4.3]) | 10.6 (+0.9 [−0.4, +2.1]) | 0.08 | ~1,400 | 97–98% |
| Qwen3-ASR 0.6B | 35.0 (+4.4 [+1.2, +7.5]) | 11.8 (+1.9 [−1.6, +5.8]) | 9.3 (−0.4 [−1.4, +0.5]) | 0.19–0.25 | 1,800–2,900 | none |
| Parakeet Redux | 40.6 (+10.0 [+5.6, +14.3]) | 9.4 (−0.5 [−2.8, +1.6]) | 11.2 (+1.5 [+0.2, +2.8]) | 0.08 | ~1,400 | 84–97% |

- **VAD gating costs accuracy on short clips too (D1).**
  - Parakeet v2 on the same clips, VAD against whole: 45.6 → 30.6 on `ami-sdm`, 16.9 → 9.9 on
    `ami-ihm`, and 11.7 → 9.7 on `earnings22`.
  - Empty room-mic clips fall from 23 to 7.
  - With VAD in front, v3 is clearly worse than v2 on all three sets (+4.4, +8.1 and +2.8), and Ultra
    is level. Decoded whole, both are level with v2. The gating makes v3's deletions worse.
- **Cohere is the most accurate model, but it can't be the record as things stand.**
  - It returns no word timestamps, and the karaoke cursor and speaker mapping need them.
  - It uses about 3× Parakeet's memory, takes 3–6 s to load, and is 2.7 GB on disk.
  - The ways round that are under "Word timings constrain the model" below.
- **Qwen3-ASR has a language problem.** Some short English clips come back in Chinese: 8 of 150 on
  `ami-sdm` and 10 on `ami-ihm`, for example "我懂。". sherpa-onnx 2.0.1 has no language setting for
  it. It also has no word timestamps and is 2–3× slower than Parakeet.
- **Redux isn't a useful 8 GB fallback.** v2 already peaks at about 1.4 GB, and Redux returns nothing
  for 24 of 150 room-mic clips.

**Unified EN on long recordings isn't a clear win.** With `energy:30/5`:

| Set | Parakeet v2 | Unified EN |
|---|--:|--:|
| ami-sdm-long | 25.3 (17.7% deleted) | 28.4 (23.4% deleted) |
| earnings22-long | 7.3 | 6.8 |

It also returned word timings for 88% of the AMI segments, against 95% for v2. Three meetings can't
settle this, but they point the other way from the short clips.

**Word timings constrain the model.** The review editor is built on per-word times, and three
things depend on them:

- **The karaoke cursor and click-to-seek.** Native times (±200–500 ms) are enough for these.
- **Speaker labels.** `engine/src/merge.rs` gives each word the speaker whose diarized span it overlaps
  most. A word placed at the wrong time can get the wrong speaker, and speaker labels are part of the
  record.
- **Paragraphs.** Without speaker labels, the editor and the Word export break paragraphs at pauses
  between words.

A model that returns no word times still runs: `group_tokens_to_words` (`engine/src/sherpa.rs`) spreads
the words evenly across the segment. The error grows with segment length. In a 30 s energy-cut piece a
word can land many seconds from where it was said. A speaker change inside the piece becomes a guess,
and the pauses that make paragraphs disappear.

So the default model must return native word timestamps. That leaves the Parakeet family: v2 and
Unified EN. The cost is Cohere's accuracy, 24.4% against 30.6% WER on `ami-sdm`, though only 1–3 points
on `ami-ihm` and `earnings22`, where Unified EN closes most of the gap.

If Cohere's accuracy is ever wanted, there are three ways round the missing timestamps:

1. **Short pieces with the even spread.** About 5–8 s pieces bound the timing error to a few seconds.
   Speaker changes and paragraphs are still guesses, and it's untested whether short pieces cost Cohere
   accuracy.
2. **Two passes.** Parakeet provides times, Cohere provides words, and a word-level alignment (edit
   distance) moves Parakeet's times onto Cohere's words. Inserted words take times from their
   neighbours. The editor keeps working fully. It costs both decodes (RTF about 0.08 + 0.15) and both
   models in memory, about 5 GB, so it's only realistic on Standard or Copilot+ machines, as an
   optional accuracy pass.
3. **A separate forced-alignment model.** Another download and more memory, so it's worth trying only
   if option 2 falls short.

Option 2 is the one to try first. It's worth building only if the Windows runs show the room-mic gap
matters in practice.

**Recommendation for the Windows reference laptop:**
- **File path:** energy cuts at 30 s, or 15 s if memory is tight on the 8 GB floor. That keeps the
  2026-11-22 decision.
- **Decoding:** keep greedy.
- **Models:** run v2 and Unified EN with `energy:30/5`, on the short and the long sets. Keep v2 as the
  default unless Unified EN also wins on the long sets.
- **Cohere:** one run on a Standard or Copilot+ machine, for speed and memory only. That's enough to
  judge the two-pass option above.
- **Dropped:** Ultra, v3, Redux and Qwen3-ASR.

## 2. How v2t's vocabulary works

One file, `~/.v2t/dictionary.txt` (mode 0600), shared by the Mac app, CLI and iOS app (App Group).
Hash lines are comments. It is reloaded when its mtime changes, with no restart.

| Line | What it does | When |
|---|---|---|
| `heard => written` | Whole-word, case-insensitive, literal substitution, in file order | After transcription, with or without cleanup |
| `Term` | Appended to the cleanup LLM's system prompt: "when the transcription has a similar-sounding word, write the term exactly as spelled here" | LLM cleanup only |

Supporting details worth copying:

- **Spelled acronyms are joined before replacements run** (`join_spelled_acronyms`: "C L I" becomes
  "CLI"). Speech models write them in about 1 in 16 dictations. A `heard` side written spaced still
  matches the joined form.
- **The word guard** (`keep_spoken_words`, `backends.py`) aligns the cleaned text to the raw text. It
  lets the LLM write a dictionary word only in place of a word that **sounds alike** (first letter plus
  consonant skeleton, difflib ratio ≥ 0.5: "clock" → "Claude" passes, "the" → "Claude" fails). This
  stops a model from reciting the dictionary into the text.
- They measured that **LLM cleanup without the guard made transcripts worse on every set** (262 word
  errors added, 107 fixed, over 1,800 labelled clips). For Folio, where the transcript is the record,
  the LLM should not rewrite the transcript, and any vocabulary step has to be deterministic or guarded.
- History stores the raw text, the final text and how many replacements fired, so every change can be
  audited. `v2t dictionary apply <file>` prints which entries fired.
- Rules for choosing a replacement or a term: a heard form nobody says on purpose ("Navio Stokes") gets
  a replacement. A real word ("Lee", "purchase") gets a term only, because a replacement would fire on
  genuine uses. A single common word is avoided as a heard side. When the right spelling is unknown,
  ask, because a wrong `written` side corrupts every later transcript.

**v2t has no ASR-level biasing.** MLX Parakeet doesn't offer it, so terms only reach the LLM.
**Folio can bias the decoder itself:**

- The `sherpa-onnx` 1.13.8 crate exposes `hotwords_file`, `hotwords_score` and `decoding_method` on
  `OfflineRecognizerConfig`. Hotwords need `modified_beam_search`, but our engine sets
  `greedy_search` (`engine/src/sherpa.rs:195`). Whether hotwords work with the `nemo_transducer`
  (TDT) model type is **unverified**: that's a spike, measuring the cost to speed and to WER on
  ordinary words.
- `OfflineQwen3ASRModelConfig.hotwords` is a prompt-based bias, if Qwen3-ASR earns a place.
- Deterministic `heard => written` replacements can sit after the ASR either way. In the editor they
  should show as changed words, so the reviewer sees what the dictionary did. That matters more for a
  legal record than for dictation.

## 3. Vocabulary without the user typing it

v2t has three routes, none of which asks the user to open the dictionary:

1. **Learn from a rejected correction** (iOS keyboard, `TypingEngine.learn` →
   `KeyboardViewController` `onNewWord`). When the user undoes an autocorrect, the word is learned. If it
   has two or more letters and no word list knows it ("marimo"), it is **added as a term
   automatically**, and the suggestion bar shows a brief "Added …" notice. Dictation then spells it
   right too. Real words kept as typed are not added.
2. **Agent turns corrections into entries** (`.claude/skills/v2t-dictionary/SKILL.md`). It puts the
   raw and corrected text side by side and keeps only spellings of things (names, products, acronyms,
   not grammar). It classifies each pair with the rules table above and checks that existing entries
   don't already cover it or rewrite it first. It adds the entries and then **proves each replacement
   fires** by running `apply` on the raw text. It reports what it skipped and why.
3. **Import** from the tool the user came from (`v2t dictionary import-wispr` reads Wispr Flow's
   local SQLite).

### How this maps to Folio

Folio already collects better signals than a keyboard. Every reviewed transcript is a (machine draft,
human-corrected) pair:

| Signal in Folio | Proposal |
|---|---|
| Reviewer edits a sentence | Align the draft and the edit by word (as v2t's guard does). For each substituted span whose new form is capitalised, an acronym, or unknown to a word list, propose `heard => written` if the heard form isn't a common word, or a term otherwise. |
| **Find & replace → Replace all** | The strongest signal: the reviewer has stated a `heard => written` pair. Offer "Fix this in future transcripts too?" |
| Speaker rename ("Speaker 2" → "Ms Okafor") | Propose the name as a hotword or term. |
| Same correction across several transcripts | Raise confidence, and show it first in the suggestion list. |
| Notes LLM (S2) | When it is present, it can propose terms from the transcript and the matter metadata, but only as suggestions, never applied silently. |
| Org vocabulary | Import or export a plain-text file (v2t's format works) that IT can push with the installer. There's no network, so a shared file is the only sync. |

Differences from v2t that suit Folio's setting: **propose, don't auto-add**. A wrong entry silently
rewrites later legal records, which v2t's own skill warns against. Show proposals in the editor or at
export, one tap to accept. Keep each entry's origin (which transcript and edit it came from) and allow
removal. Apply accepted entries visibly in the draft.

### Suggestion UX: options to try

v2t's UX is minimal: the keyboard auto-adds the word and shows "Added …" in the suggestion bar, and the
agent reports what it added and skipped. Folio needs consent and an audit trail, so here are some
options to prototype:

| Option | When it appears | Fits | Risk |
|---|---|---|---|
| **A. Inline chip** | Right after a reviewer fixes a likely name: "Use *Okafor* in future transcripts?" under the sentence | Instant, with full context | Interrupts review. Chips pile up on a heavily edited transcript |
| **B. End-of-review batch** | At "mark reviewed" or export: "You fixed 5 spellings. Use them next time?" A list of `heard → written` with counts and checkboxes | One decision at a natural pause; easy to skip | Context is gone, so each row needs a snippet and play button |
| **C. Replace-all hook** | A checkbox in the find & replace row: "Also fix in future transcripts" | The reviewer has already stated the pair | Only catches replace-all users |
| **D. Visible on next use** | In later drafts, words the vocabulary changed or biased get a subtle underline. Hover shows the rule, its source transcript, and Undo or Remove rule | Builds trust, makes rules auditable, and lets bad rules be removed where they misfire | Visual noise if overused |
| **E. Vocabulary page** | A list in Settings or Models with each entry's origin, hit count and last use, plus import and export for IT | Org-wide lists, bulk tidy-up | Nobody visits it unprompted |

**Decided (2026-11-20): B + C to collect, D to keep it auditable, E as the fallback.** A waits until
after user testing. A clickable prototype of B–E exists for that testing.

## Suggested next steps

1. ~~Port the `oasr` scorer and add `ami-sdm`, `ami-ihm` and `earnings22` at 150 clips, with bootstrap
   intervals, to the eval harness.~~ Done: `tools/asr-bench` ([PR #24](https://github.com/KindTechUK/folio/pull/24)).
2. Decoding cells D1–D4 (section 1b): done on Mac (2026-11-23, section 1b). Still to run on the
   Windows reference laptop.
3. On the Windows reference laptop: Parakeet v2 (baseline) and Unified EN, both with `energy:30/5`,
   plus one Cohere run on a Standard or Copilot+ machine. The Mac grid dropped Ultra, v3, Redux and
   Qwen3-ASR. Record WER, RTF, peak memory, load time, and whether word timestamps come back.
4. Vocabulary: adopt v2t's file format (`Term` / `heard => written`, plus acronym joining) and add
   deterministic replacements after ASR. Both are in
   [PR #26](https://github.com/KindTechUK/folio/pull/26), with acronyms joined only when an entry asks
   for it. Next comes the D5 hotwords spike.
5. Prototype the suggestion UX (B + C + D) against the review editor.
6. Device tiers, modelled on v2t's `Recommendation`: see
   [`windows-device-tiers-2026-11.md`](windows-device-tiers-2026-11.md). Run the bench on its
   reference machines, starting with the ~£110 refurbished i5-8265U with 8 GB.
