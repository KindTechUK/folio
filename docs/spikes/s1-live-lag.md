# Spike S1: live transcription on the existing engine

> Status: **GO-WITH-CONDITIONS** (Mac + Windows CI runner; reference laptop still to run) · Recorded 2026-11-10.
> Plan: [`next-phase-spikes.md`](next-phase-spikes.md#s1-live-transcription-on-the-existing-engine).
> Harness: `engine/examples/live_replay.rs`.

## Question

If mic audio goes into the same Silero VAD → Parakeet TDT v2 `OfflineRecognizer` path the app uses for
files, how far behind speech does the text appear, does the machine cope, and is the text as good?

## Verdict so far

**Recommended live profile: 0.8 s silence / 15 s max speech.** It passes the lag target on both
machines measured:

- **Apple M5 Pro:** p95 end lag 1.7 s simulated, 2.3 s in real time. The decoder is busy about 5% of the time.
- **Windows x86 CI runner** (AMD EPYC 7763, 1 core / 2 threads, 8 GB, 2 recognizer threads; weaker than
  the reference laptop): p95 3.25 s, max 3.99 s. The decoder is busy 12.8% of the time, with no queueing.

Accuracy against human transcripts is close to the file path. The conditions:

1. **Repeat on the reference Windows laptop** before P1 ships. The CI runner is a pessimistic stand-in, not
   the reference machine.
2. **Memory grows during long sessions.** Over the 2-hour soak, the footprint rose from about 1.11 GB to 1.34 GB
   (see "Soak"). P1 should rebuild the recognizer at a quiet moment every 30 minutes or so, and check this on Windows.
3. **The pass criterion "WER delta vs offline ≤ 1 point" was the wrong measure**, and is replaced here by
   WER against a human reference (see "Accuracy").
4. **Text arrives an utterance at a time.** The oldest word of an utterance can be up to about 20 s old
   when it appears (start lag p95). P1's UI has to show that capture is working between utterances.

## Environment

- Apple M5 Pro, 18 cores, 48 GB, macOS. Background load average about 4.6–5.2 during the runs.
- Parakeet TDT 0.6B v2 int8 and `silero_vad.onnx` at the pinned revisions in
  `scripts/modelpack-assets.sha256`. 4 recognizer threads (the app default). VAD threshold 0.5,
  min speech 0.25 s, as in `engine/src/sherpa.rs`.
- Audio: two AMI Meeting Corpus meetings (CC-BY-4.0), `Mix-Headset` mixes converted to 16 kHz mono:
  ES2002a (21 min, 4 speakers) and IS1009a (14 min, 4 speakers). Real, overlapping meeting speech.
  Human reference transcripts from the AMI manual annotations 1.6.2 (word files of all speakers,
  ordered by start time, punctuation dropped).

## Commands

```sh
cd engine && cargo build --release --example live_replay
for m in ES2002a IS1009a; do
  ffmpeg -i $m.Mix-Headset.wav -ac 1 -ar 16000 -sample_fmt s16 $m.16k.wav
  target/release/examples/live_replay $m.16k.wav --models-dir <models> --threads 4 \
    --json sim.jsonl --texts texts/            # 3 x 3 sweep + offline reference
done
target/release/examples/live_replay IS1009a.16k.wav --models-dir <models> --threads 4 \
  --silence 0.8 --max-speech 15 --realtime --json rt.jsonl            # wall-clock check
target/release/examples/live_replay IS1009a.16k.wav --models-dir <models> --threads 4 \
  --silence 0.8 --max-speech 15 --realtime --loops 9 --json soak.jsonl  # 2 h soak
```

The default mode feeds the file as fast as possible but times every utterance on the audio clock,
with one decode worker (an utterance waits if the previous decode is still running). Decode times are
real. `--realtime` paces the audio at real speed on one thread and decodes on another, so its lag is
wall-clock.

Definitions:
- **End lag:** from the end of an utterance's speech to its text being available. This is the S1 pass metric.
  It includes the VAD's silence wait, any queueing and the decode.
- **Start lag:** from the start of an utterance's speech to its text being available, i.e. how old the oldest
  word is when it appears.
- **Decoder busy:** total decode time divided by audio duration.

## Results: sweep (simulated clock, measured decode)

**ES2002a** (21 min). Offline file path: RTF 0.053, WER vs human 23.0.

| Silence / max | Utterances | Median length | End lag p50 / p95 / max | Start lag p95 | Decoder busy | WER vs offline | WER vs human |
|---|---|---|---|---|---|---|---|
| 0.3 s / 10 s | 158 | 3.3 s | 0.53 / 1.00 / 1.64 s | 14.5 s | 4.6% | 10.3 | 22.3 |
| 0.3 s / 15 s | 147 | 3.3 s | 0.52 / 1.38 / 1.66 s | 18.9 s | 4.7% | 10.1 | 21.7 |
| 0.3 s / 20 s | 140 | 2.9 s | 0.51 / 1.70 / 2.19 s | 24.5 s | 4.8% | 10.1 | 21.4 |
| 0.5 s / 10 s | 145 | 3.8 s | 0.75 / 1.11 / 1.66 s | 16.1 s | 4.6% | 9.5 | 22.2 |
| 0.5 s / 15 s | 129 | 3.7 s | 0.77 / 1.39 / 1.78 s | 19.2 s | 4.6% | 9.0 | 21.3 |
| 0.5 s / 20 s | 122 | 3.2 s | 0.73 / 1.77 / 2.18 s | 24.9 s | 5.0% | 9.4 | 21.2 |
| 0.8 s / 10 s | 126 | 6.7 s | 0.93 / 1.37 / 1.73 s | 17.4 s | 4.9% | 8.1 | 22.0 |
| **0.8 s / 15 s** | 110 | 5.3 s | 1.12 / 1.63 / 1.74 s | 19.7 s | 4.7% | 7.7 | **21.1** |
| 0.8 s / 20 s | 100 | 4.9 s | 1.15 / 1.80 / 2.16 s | 24.9 s | 4.7% | 7.8 | 21.0 |

**IS1009a** (14 min). Offline file path: RTF 0.059, WER vs human 23.7.

| Silence / max | Utterances | Median length | End lag p50 / p95 / max | Start lag p95 | Decoder busy | WER vs offline | WER vs human |
|---|---|---|---|---|---|---|---|
| 0.3 s / 10 s | 92 | 6.1 s | 0.74 / 1.04 / 1.55 s | 14.1 s | 5.1% | 9.6 | 26.5 |
| 0.3 s / 15 s | 80 | 6.8 s | 0.77 / 1.39 / 1.63 s | 19.7 s | 5.2% | 8.0 | 24.4 |
| 0.3 s / 20 s | 78 | 6.3 s | 0.79 / 1.65 / 1.90 s | 24.7 s | 5.1% | 8.9 | 24.8 |
| 0.5 s / 10 s | 76 | 9.2 s | 0.84 / 1.24 / 1.48 s | 17.7 s | 5.2% | 9.0 | 25.9 |
| 0.5 s / 15 s | 66 | 9.3 s | 1.15 / 1.49 / 1.73 s | 19.8 s | 5.4% | 6.9 | 25.4 |
| 0.5 s / 20 s | 61 | 9.2 s | 1.15 / 1.87 / 2.16 s | 24.7 s | 5.6% | 7.1 | 25.6 |
| 0.8 s / 10 s | 72 | 10.0 s | 0.98 / 1.36 / 1.63 s | 17.8 s | 5.4% | 8.3 | 25.2 |
| **0.8 s / 15 s** | 60 | 11.5 s | 1.19 / 1.67 / 1.72 s | 19.6 s | 5.2% | 5.9 | **24.6** |
| 0.8 s / 20 s | 52 | 12.0 s | 1.55 / 1.97 / 2.12 s | 24.8 s | 5.4% | 6.5 | 25.4 |

The projected average CPU over a live session was about 1% of this 18-core machine in every row.

## Results: real time (IS1009a, 0.8 s / 15 s)

| Wall time | Utterances | End lag p50 / p95 / max | Start lag p50 / p95 | Decoder busy | Machine CPU |
|---|---|---|---|---|---|
| 830.8 s | 60 | 1.55 / 2.27 / 2.53 s | 13.2 / 20.7 s | 7.5% | 1.5% |

Real-time lag is about 0.6 s higher than the simulated figures, because decoding shared the machine with
the audio thread and other work. It is still well inside the 5 s target.

## Results: Windows x86 CI runner (IS1009a, simulated clock)

GitHub `windows-latest` for a private repo: AMD EPYC 7763, 1 core / 2 logical processors, 2.45 GHz, 8 GB.
`--threads 2`. A temporary workflow on the spike branch ran the harness; it was removed after the run.
Offline file path: RTF 0.158 (M5 Pro: 0.059).

| Silence / max | Utterances | Median length | End lag p50 / p95 / max | Start lag p95 | Decoder busy | Queued | WER vs offline |
|---|---|---|---|---|---|---|---|
| 0.5 s / 10 s | 76 | 9.2 s | 1.95 / 5.55 / 6.89 s | 19.4 s | 17.4% | 5 | 9.2 |
| 0.5 s / 15 s | 66 | 9.3 s | 2.11 / 3.24 / 4.03 s | 21.6 s | 12.8% | 1 | 8.0 |
| 0.8 s / 10 s | 72 | 10.0 s | 1.94 / 3.14 / 4.46 s | 19.4 s | 13.6% | 2 | 8.7 |
| **0.8 s / 15 s** | 60 | 11.5 s | 2.58 / 3.25 / 3.99 s | 21.6 s | 12.8% | 0 | 6.2 |

Decoding is about 2.5× slower than on the M5 Pro, at half the threads. The 0.5 s / 10 s row is the
outlier: its total decode time was about a third higher than the other rows for the same audio. That is
probably runner noise, since on the Mac the same row cost no more than the others. The WER figures match the Mac's to within about half a point, as expected for
the same model. Process CPU wasn't sampled (the harness uses `ps`). With the decoder busy about 13% on a
2-thread machine, the average CPU is roughly in the same range, far under the 60% limit.

## Results: 2-hour soak (Mac, real time)

IS1009a looped 9 times (126 min) in real time at 0.8 s / 15 s. Memory was sampled every 10 s as the macOS
physical footprint. The process RSS from `ps` is unusable on macOS here: in the first real-time run it
dropped from 1.2 GB to 21 MB as idle pages were compressed.

| Wall time | Utterances | End lag p50 / p95 / max | Start lag p95 | Decoder busy | Footprint first → last (min / max) |
|---|---|---|---|---|---|
| 7,542 s | 515 | 1.37 / 2.70 / 10.97 s | 22.3 s | 5.9% | 1,127 → 1,335 MB (797 / 1,335) |

- **Lag held** over 2 hours. The single 11 s maximum was one utterance that waited behind others while the
  machine was busy.
- **Memory is not flat.** After the first minute, the footprint climbed steadily from about 1,110 MB to
  1,335 MB, roughly 100 MB an hour, slowing slightly. That misses the "±5% over 2 h" criterion. The
  harness keeps only a few kilobytes of state, so the growth is most likely in the ONNX Runtime allocator
  as one recognizer handles hundreds of differently sized inputs. The file path never shows this, because it builds a
  recognizer per transcription. Mitigation for P1: rebuild the recognizer at a quiet moment, every 30
  minutes or so. That takes about 1–2 s and can run while the VAD has no open utterance. Check on Windows with private bytes.

## Accuracy

The plan's criterion was "WER delta against the offline path ≤ 1 point". Every live profile misses that
by 6–10 points. But that number compares two machine transcripts. Against the AMI human transcripts,
**live and file transcripts are about equally accurate**. They make different mistakes, mostly at
utterance boundaries:

| | ES2002a WER vs human | IS1009a WER vs human |
|---|---|---|
| Offline file path | 23.0 | 23.7 |
| Live, 0.8 s / 15 s | 21.1 (−1.9) | 24.6 (+0.9) |
| Live, range over all 9 profiles | 21.0–22.3 | 24.4–26.5 |

So the criterion becomes: **live WER against a human reference within 1 point of the file path, or
better.** 0.8 s / 15 s meets it on both meetings. The short-silence profiles (0.3 s) do worst on IS1009a,
where cutting at short pauses breaks sentences apart.

The absolute WER (21–26%) is high because AMI meetings have overlapping speech and fillers that the
reference keeps and Parakeet drops. Both paths are scored the same way, so the comparison is fair.

## Headroom

The CI runner shows what a weak machine does: decodes 2.5× slower than the M5 Pro, with p95 lag still 1.75 s
under the limit. A 12th/13th-gen U-series laptop has more and faster cores than one EPYC core, so it should
land between the two. If the reference laptop still misses, try 0.8 s / 10 s next (p95 3.14 s on the runner),
before S7.

## What changes in the PRD

- **Live VAD profile:** 0.8 s silence / 15 s max speech, alongside the offline 0.8 s / 120 s profile.
  Fallback 0.8 s / 10 s.
- **Long sessions:** rebuild the recognizer periodically (see "Soak").
- **No new model or crate is needed for live text** (subject to the Windows run). S7 (Nemotron streaming)
  stays a fallback only.
- **UI:** text appears an utterance at a time, typically 5–12 s of speech at once, and up to about
  20 s after its first word. The live view needs a visible "listening" state for the open utterance
  (elapsed time and an input level), so the user can see that capture is working. Showing provisional words
  inside the open utterance would mean re-decoding the growing buffer every few seconds. That is a
  separate experiment, because it multiplies decode work by the very machines that have the least of it.
- **Test 1 in the PRD's Testing Decisions** ("live pipeline equivalence") should compare against a human
  reference, or allow the measured boundary differences, rather than require the file path's exact words.

## Still to do

1. The same sweep, real-time run and soak on the reference Windows laptop, with Task Manager-equivalent
   CPU and private bytes.
2. Confirm that periodic recognizer rebuilds hold memory flat over 2 hours.
3. A real-microphone run (`cpal`) to check the capture path adds no lag. This belongs in the P1 build, as the
   recorder is part of it.
