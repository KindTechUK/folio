# Live capture in open-source transcription apps (Nov 2026)

> Recorded 2026-11-10. Read from each project's source and issue tracker on GitHub.
> Purpose: check Folio's in-app recording design (P1) against how comparable apps do it,
> especially on Windows. Claims cite the project's own files, issues or PRs; "not found"
> means we looked and didn't find it.

Projects: Meetily (`Zackriya-Solutions/meetily` @ 07c4dfd), Hyprnote, now Anarlog
(`fastrepl/anarlog` @ 0a97fcd; issues disabled, so evidence is from PRs), OpenWhispr
(`OpenWhispr/openwhispr` @ 4cb0866), Handy (`cjpais/Handy` @ 73cc857, capture in
`handy-computer/handy-recorder` @ 55748ee), Buzz (`chidiwilliams/buzz`), Vibe
(`thewh1teagle/vibe`), and sherpa-onnx's own examples (`k2-fsa/sherpa-onnx`).

## Summary

Folio's pipeline is the common one:

1. capture the microphone with `cpal`;
2. resample to 16 kHz mono;
3. split into utterances with Silero VAD;
4. decode each utterance offline with Parakeet on a worker thread;
5. diarize after stop.

Meetily is the closest match, and sherpa-onnx's own Parakeet + cpal microphone example has the same structure. Folio's crash-safe recording is stronger than every app reviewed.

The gaps were all in Windows device robustness and backlog handling. They are addressed in the recording-robustness change.

## Comparison

| | Folio | Meetily | Anarlog | OpenWhispr | Handy |
|---|---|---|---|---|---|
| Mic capture | cpal, default input | cpal (patched 0.15) | cpal 0.17 | Chromium `getUserMedia` | cpal fork (handy-recorder) |
| Internal rate | 16 kHz (rubato FFT) | 48 kHz; VAD at 16 kHz | 16 kHz (rubato async) | 16/24 kHz (Chromium) | 16 kHz (rubato FFT) |
| Segmentation | Silero, 0.8 s silence / 15 s max | Silero, 0.5 s silence, no max (#756) | Silero v6.2, 0.6 s, 20 s target / 25 s max | fixed 5 s chunks (#2998: hurts accuracy) | pause cuts, ~30 s target / 40 s max |
| ASR on Windows | Parakeet TDT v2 (sherpa-onnx), in process | Parakeet v2/v3 (own ORT code) or whisper.cpp | none local (cloud or local server) | whisper.cpp / sherpa-onnx sidecars | transcribe-cpp in a supervised worker process |
| Decode queue | bounded memory: spans read from WAV past 120 s | unbounded, one worker | bounded, with backpressure | unbounded (#2976: 37-min stall) | unbounded |
| After stop | live text kept; optional diarization over the WAV | live text kept; re-transcribe on request; no diarization | batch re-transcription recommended; pyannote + WeSpeaker | live text kept; pyannote 3.0 + CAM++ via sherpa-onnx | n/a (dictation) |
| Recording storage | WAV, header rewritten and fsynced every 5 s | AAC checkpoints every 30 s | stereo MP3 + fsynced 60 s chunks | meeting audio not kept (#2976) | WAV written after stop; none on crash |
| Device loss | reopen default, pad gap with silence | fall back to default (3 tries) | retries 1/2/4 s | fall back to default | reopen, keep audio so far |
| Silent-mic detection | 3 s of digital silence | not found | zero-run monitor (#7284) | loopback watchdog only | no-audio watchdog |
| Bluetooth headset | warn when input is ≤ 16 kHz or Bluetooth | name heuristics | prefers a wired mic (#7284) | needs exclusive mode off (#1138) | README note |
| System audio | not yet (P4) | endpoint loopback, mixed with mic, no AEC | endpoint loopback following the active endpoint; AEC skipped on headphones | process loopback helper + WebRTC AEC3 + text dedup | none (out of scope) |

Buzz and Vibe are less relevant:
- **Buzz:** fixed 5 s batches with no VAD; drops audio when 3 batches behind; keeps no audio.
- **Vibe:** records first and transcribes after stop; its WAV is only valid after `finalize()`.

## Findings that changed Folio

1. **Device robustness.** Windows microphones fail mid-recording, or deliver zeros instead of an error:
   - mic privacy denial;
   - Realtek "Voice Clarity" (Handy #2141);
   - Bluetooth HFP gating (Anarlog measured 26–52% zero samples, #7284).

   Folio now reopens the default microphone after an error or a 3-second stall, with retries of 1, 2, 4, 8 and 8 s. It records silence for the gap, warns after 3 s of digital silence, and warns when the input looks like a headset (Bluetooth, or 16 kHz or less): headsets are tuned to the wearer, so other people in the room may be missed.
2. **Backlog.** OpenWhispr's unbounded queue stalled an hour-long meeting (#2976). Folio keeps decoding every utterance, since the WAV is the record. Past 120 s of backlog the queue holds only positions, and the decoder reads those samples back from the WAV. The UI says when live text is catching up, and Stop shows progress through the remaining backlog.
3. **Power loss.** Anarlog fsyncs its recording chunks. Folio now fsyncs the WAV each time it rewrites the header.
4. **Call capture (P4).** Anarlog removed Windows process loopback because new Teams produced zeros on it (PRs #7283 → #7590). The PRD now specifies endpoint loopback following the active endpoint, separate mic and call channels, padded loopback gaps, and AEC only without headphones.

## Kept as is, with reasons

- **0.8 s silence / 15 s max speech.** Others use 0.5–0.6 s and 20–25 s. Spike S1 measured 0.8 s / 15 s as the most accurate profile with Parakeet on two AMI meetings; 0.5 s did worse on one. Very short segments are known to hurt accuracy (OpenWhispr #2998; Meetily dropped 400 ms because Whisper hallucinated on short clips). sherpa-onnx's maximum is soft: past it the VAD threshold rises to 0.9 until the segment ends.
- **Recognizer rebuilt every 30 minutes.** None of the apps do this. Folio's own 2-hour soak in S1 showed slow memory growth, and sherpa-onnx has related open reports (#3979, #1939). It needs a Windows check.
- **Statically linked sherpa-onnx.** This avoids OpenWhispr #2054, where Windows' own older `System32\onnxruntime.dll` was loaded in place of the bundled one.
- **No interim words inside an open utterance.** sherpa-onnx's examples re-decode the open segment about every 0.2 s for partial text. That multiplies decode work on the machines with the least of it, so it waits for its own measurement.

## Still to check on Windows hardware

- Bluetooth headsets: HFP gating, and the narrowband warning.
- Mic privacy denial producing zeros.
- Realtek effects.
- RDP "Remote Audio" buffer sizes (Handy #2291).
- CPUs without AVX2 (OpenWhispr #1418, Meetily #465).
- Memory over a 2-hour session with recognizer rebuilds.
