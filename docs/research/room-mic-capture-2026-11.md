# One laptop in the meeting room: far-field capture (Nov 2026)

> Recorded 2026-11-23. Read from source: OpenWhispr (`OpenWhispr/openwhispr` @ 7970fda, release
> 1.12.0), Meetily (`Zackriya-Solutions/meetily` @ 2166bc6) and Anarlog, formerly Hyprnote and briefly
> Char (`fastrepl/anarlog` @ d63676c). Granola, Otter and Jamie from their help centres, security
> pages and blogs, plus reviews. Most reviews are by competitors and are marked as such. Papers:
> NOTSOFAR-1 and its summary, CHiME-8 system papers, and work on speech enhancement before ASR. Model
> cards for Parakeet TDT v2 and pyannote. Microsoft and Apple documentation on OS audio processing.
> **New measurements:** `tools/asr-bench` runs on the M5 Pro (sherpa-onnx 2.0.1, Parakeet v2, greedy)
> on copies of the bench pack's room-mic and headset audio after ffmpeg front-end processing
> (section 3.1). The processed packs were scratch copies and weren't kept; the arms below are enough
> to rebuild them.
> Purpose: the product owner believes OpenWhispr and Granola "do it alright" with one laptop in the
> middle of a meeting room. Find out what they actually do, what Folio can copy under zero egress,
> CPU only and 8 GB, and what to bench next. Builds on `voice2text-lessons-2026-11.md` (sections 1
> and 1b), which found that VAD gating costs 13 points of WER on room-mic meetings.

## Summary

- **Granola's in-room quality isn't a technique Folio can copy.** It streams audio to cloud ASR
  (Deepgram and AssemblyAI, both US-hosted) and needs a live connection. On desktop it labels
  everyone in the room "Me", and people read its summary rather than the transcript. Its in-room
  advice is all placement: face the mic at the conversation, sit near the main speaker, avoid Low
  Power Mode.
- **OpenWhispr isn't built for rooms.** It's a dictation app with meeting notes added, and in-person
  speaker labels came in 2026. Its local meeting path decodes fixed 5 s chunks, and its own issue
  #2998 measured VAD doubling the errors. Two things are worth copying:
  - It records the mic raw, with browser AGC, noise suppression and echo cancellation all off.
  - It has a measured diarization policy on the same sherpa-onnx models Folio uses.
- **Enhancement before ASR hurts room audio, on Folio's own bench too.** RNNoise adds 4.3 points
  [+2.0, +6.7] on `ami-sdm` and 4.3 on the long meetings; spectral NS adds 1.8–4.1. RNNoise is level
  on headset audio, so the damage falls on distant speech. This matches the literature (Iwamoto
2022; "When De-noising Hurts" 2025, which
  includes Parakeet). No app we read enables a denoiser by default.
- **AGC is neutral for the decoder but rescues the VAD.** Parakeet normalises each mel band per
  segment, so every gain and AGC arm lands within 0.8 points of the original. But levelling the
audio before Silero
  takes the live profile from 38.7% to 27.9% on `ami-sdm-long`. Most VAD deletions on room audio are
  about input level, not speech detection.
- **Not gating is still the biggest lever, and the rest of the gap is mostly physics.**
  - Energy cuts give about 25%, against about 10% on headsets. Half of the remaining deletions are
    backchannels and short words, consistent with overlap that no single-channel front end undoes.
  - NOTSOFAR-1's best single-channel system still scored 22% tcpWER, with GPU-scale fine-tuned
    Whisper.
  - Cohere, best on short clips, fell to 43% on the long meetings with repetition loops.
- **OS processing is a risk to manage, not a tool.** MacBook mics are always beamformed by macOS,
  for a user who most likely sits at the keyboard. Windows OEM modes can focus on "the person facing
  it". Windows allows raw capture where the driver supports it. Laptop beams aren't aimed at a
  table, so prefer raw and measure.
- **Distance costs diarization less than ASR.** pyannote's AMI DER rises only 3 points from headset
  to room mic, so speaker counting is the problem. Asking for the head count and copying OpenWhispr's
  clustering rules are cheap. A £60–90 USB speakerphone in the middle of the table is the
  hardware advice everyone gives.

## 1. What Granola and OpenWhispr actually do

| | Granola desktop (Mac, Windows) | Granola mobile (iPhone, Android) | OpenWhispr, local meeting mode |
|---|---|---|---|
| Audio captured | Microphone plus the combined system-audio stream. No audio kept. | Phone microphone; audio cached during the meeting, deleted after transcription. | Microphone plus a system-audio tap. Meeting audio not kept. |
| Where ASR runs | **Cloud, streamed in real time.** Security page: "best-in-class transcription providers (like Deepgram and Assembly)". Help centre: "Transcription needs a live connection to our transcription providers." Sub-processor lists show Deepgram and AssemblyAI, both US-hosted. | Cloud, after the meeting, from the cached audio. | On the laptop: whisper.cpp, or Parakeet, Orukeet (a multilingual Parakeet fine-tune) or Cohere Transcribe through sherpa-onnx. Cloud providers (OpenAI, Deepgram, AssemblyAI and others) are a separate mode. |
| Pre-processing | Filters echo of the system audio out of the mic ("Granola normally filters this echo out"). Nothing documented on noise suppression or gain. | Not documented. | **None on the mic by default.** `echoCancellation`, `noiseSuppression` and `autoGainControl` are all `false` (`microphoneSelection.js`, `meetingRecordingStore.ts`). The code comment: AGC is off because "Chromium's AGC on Windows mutates the system mic volume via WASAPI (#476)", and echo cancellation and noise suppression are off "to avoid latency and speech distortion". An optional WebRTC AEC3 helper adds moderate noise suppression, but only when the echo-cancellation toggle is on, and it defaults to off. |
| Segmentation | The provider's streaming endpointing. | Not documented. | **Fixed 5 s chunks** (`LOCAL_MEETING_CHUNK_INTERVAL_MS = 5000`), each decoded on its own. Chunks with RMS < 0.0015 and peak < 0.05 are skipped. whisper.cpp adds Silero VAD by default. |
| People in the room | **All labelled "Me".** The mic is one bucket and system audio is "Them". Names come only from Meet, Zoom or Teams display names, so a room has none. | "The Granola mobile app (iPhone and Android) can also distinguish speakers during in-person meetings." Method not documented. | Diarizes the mic track when no call audio is present, since #1726. Same models as Folio (pyannote segmentation-3.0 and CAM++ through sherpa-onnx), plus a tuned clustering policy (section 3.6). |
| Built for | Video calls. In-person use gets placement advice. | In-person meetings. | Dictation first, then calls. The changelog added in-person speaker labels during 2026 (#1726, #2719, #2782). |

**Granola.** Its in-room quality comes from three things. Folio can copy none of them.

- **A commercial cloud ASR.** Deepgram markets Nova-3 for "distant, noisy, and multi-speaker"
  audio. Training data and far-field figures are not published; all the figures are the vendor's own.
- **The summary hides transcript errors.** People read Granola's notes, not its transcript. This is
  inference from how the product is used, but HappyScribe (a competitor) reports transcripts that
  miss numbers and codes, and Rimo (also a competitor) scored its raw accuracy at 88%.
- **No speaker labels in the room on desktop,** so there are no speaker errors to see.

Granola's own advice for rooms is physical: face the laptop's microphone toward the conversation,
stay "roughly 6 to 12 inches from the primary speaker", put it near the main speaker rather than at
the far end of a long table, and avoid Low Power Mode, which "can reduce microphone sensitivity". It
says placement beats software noise reduction afterwards. Circleback, a competitor, suggests a USB
conference microphone, or everyone joining one call from their own laptop.

**OpenWhispr** doesn't do in-room capture well. It is a dictation app that grew meeting notes.
- **The local path is weak on rooms.** Its 5 s chunks are decoded in isolation, with no context.
  Its own issue #2998 found the meeting path clearly worse than dictation on the same mic. Silero
  VAD for meetings doubled the errors (about 14 against 7 on the same reading) by splitting chunks
  into sub-second pieces.
- **What it gets right:**
  - It records the mic raw.
  - It never touches the OS mic level.
  - It ships a measured diarization policy on the same sherpa-onnx models Folio uses (section 3.6).

## 2. Other tools, where they show a technique

- **Otter** (cloud) gives the clearest user guidance. Put the microphone "in the center of all
  speakers", use an omnidirectional external mic, keep it within three feet of whoever is speaking,
  and don't touch the device. It also warns that noise-cancelling mics "can clip or distort voices",
  and that one mic in a large room hurts speaker identification.
- **Jamie** (cloud, EU) records in-person meetings from the laptop's internal mic. It labels
  speakers with voice profiles remembered across meetings. Audio streams to a European transcription
  provider.
- **Meetily** (local) puts an 80 Hz high-pass filter and an EBU R128 loudness normaliser (−23 LUFS,
  a gain from cumulative loudness, so it changes slowly) on the mic. It ships RNNoise but **disables
  it by default** (`RNNOISE_APPLY_ENABLED = false`, "Whisper handles noise well").
- **Anarlog** (local on Apple Silicon) runs Apple's voice processing (AEC, plus Apple's AGC and noise
  suppression) on the mic only when the call plays through the laptop speakers. With headphones it
  records the mic plainly. It also measured that voice processing ducks other apps' audio by 8 dB.
  The voice processing is there for echo, not for rooms.
- **NOTSOFAR-1's commercial room devices** are the professional version of "one device in the
  room". The single-channel streams from Yealink, EPOS, Sennheiser, Logitech MeetUp, Poly Studio and
  Jabra PanaCast pass through "echo cancellation, de-reverberation, beamforming, and noise
  suppression" in the device. Even so, the best single-channel system scored 22.2% tcpWER, against
  10.8% from the raw 7-mic arrays (section 3.5).

None of these tools runs a speech-enhancement model before ASR by default.

## 3. Techniques, and whether they help ASR

### 3.1 New bench runs: gain, AGC and noise suppression on AMI room-mic audio

**Method.** Each arm is the bench pack's audio passed through one ffmpeg filter, then written back as
16 kHz mono 16-bit, with the references unchanged. The decoder is Parakeet v2, greedy, as shipped.
The arms:

- `gain−12`: static −12 dB.
- `gain+20`: static +20 dB with a peak limiter (0.89).
- `agc-3s`: `dynaudnorm`, 200 ms frames over a ~3 s window, up to 30× gain. A fast digital AGC,
  close in spirit to WebRTC AGC2's adaptive digital gain.
- `agc-gentle`: the same with up to 4× gain.
- `agc-15s`: `dynaudnorm`, ~15 s window, up to 10×.
- `fft-ns`: `afftdn`, 12 dB of classical spectral noise reduction with noise tracking, like WebRTC
  NS.
- `rnnoise`: ffmpeg `arnndn` with the general-purpose `sh` model, RNNoise as Meetily ships it.
- `rnnoise-mix`: 70% RNNoise plus 30% original, Iwamoto's "observation adding".

**Long meetings** (`ami-sdm-long`, 3 meetings, 55 min, no interval possible):

| Arm | `energy:30/5` WER | deleted | ES2004a / IS1009a / TS3003a | `vad:0.8/120` WER | deleted | `vad:0.8/15` WER |
|---|--:|--:|---|--:|--:|--:|
| original (from the Full Mac grid) | 25.3 | 17.7% | 25.7 / 24.2 / 25.7 | 38.8 | 34.1% | 38.7 |
| `gain−12` | 25.2 | 17.5% | 24.9 / 23.5 / 26.9 | | | |
| `gain+20` | 24.8 | 17.2% | 24.9 / 23.4 / 26.0 | **29.5** | 22.1% | **30.1** |
| `agc-3s` | 24.7 | 16.5% | 24.8 / 24.5 / 24.7 | **27.3** | 19.3% | **27.9** |
| `agc-gentle` | 24.7 | 16.9% | 24.6 / 23.1 / 26.1 | | | |
| `agc-15s` | 24.5 | 16.6% | 24.2 / 24.3 / 25.0 | 29.4 | 22.1% | |
| `fft-ns` | 27.1 | 19.2% | 27.0 / 24.4 / 29.4 | | | |
| `rnnoise` | **29.6** | 21.6% | 28.5 / 25.6 / 33.8 | | | |
| `rnnoise-mix` | 29.0 | 20.3% | 28.4 / 24.6 / 32.9 | | | |

**Short clips** (150 each, every clip decoded whole, Δ against unprocessed with its paired 95%
interval):

| Arm | `ami-sdm` WER | Δ [95%] | `ami-ihm` WER | Δ [95%] |
|---|--:|--:|--:|--:|
| original | 30.6 | – | 9.9 | – |
| `agc-3s` | 31.5 | +1.0 [−0.9, +2.9] | 9.4 | −0.5 [−2.0, +0.7] |
| `fft-ns` | 34.6 | **+4.1 [+1.8, +6.5]** | | |
| `rnnoise` | 34.8 | **+4.3 [+2.0, +6.7]** | 9.3 | −0.6 [−2.8, +1.1] |

**What the runs say:**

- **The noise floor of a long run is about ±1 point per meeting.** A static −12 dB moved single
  meetings by −0.8, −0.7 and +1.2 and left the total unchanged. Treat any long-set difference under
  about 1 point as no difference.
- **Noise suppression hurts room audio and does nothing for headset audio.**
  - **RNNoise:** +4.3 points on the long meetings, worse on all three, and +4.3 [+2.0, +6.7] on the
    short clips.
  - **Spectral NS:** +1.8 on the long meetings and +4.1 [+1.8, +6.5] on the short clips.
  - **On the long meetings the extra errors are mostly deletions:** the denoiser removes quiet
    distant speech along with the noise. On short clips they're mostly substitutions, from
    distorted words.
  - **Observation adding** (`rnnoise-mix`) recovers only 0.6 points. On headset audio RNNoise is
    level (−0.6, interval spans zero). Denoising has nothing to fix there, and it breaks distant
    speech.
- **AGC doesn't help the decoder.** All five gain and AGC arms sit at 24.5–25.2 on energy cuts,
  within the ±1-point noise. On short clips `agc-3s` is +1.0 with an interval spanning zero.
  Parakeet's per-band normalisation already absorbs level (3.2).
- **AGC rescues the VAD.** With the audio levelled first, the VAD-gated segmenters recover most of
  what gating loses:
  - `vad:0.8/120` goes from 38.8 to 27.3, and `vad:0.8/15`, the live profile, from 38.7 to 27.9.
  - Deletions fall from 34% to 19%.
  - Even a static +20 dB gets to 29.5–30.1, so most of the VAD's losses are about the input level.
  - Energy cuts are still about 3 points better.
- **Caveats:**
  - These are ffmpeg filters, not WebRTC's or the OS's implementations.
  - There are only three long meetings.
  - AMI's table mic is quieter (about −49 dBFS RMS) than a laptop with OS input gain may be.

### 3.2 Gain normalisation and AGC

**For Parakeet, the level of the decoded audio barely matters.** Its front end normalises each mel
band over the segment being decoded: the int8 encoder's metadata reads `normalize_type =
per_feature`.
- **Fixed gain is a no-op.** A fixed gain disappears in that normalisation. A fast AGC only changes
  how loud one talker is relative to another within a piece, and that measured as noise (3.1).
- **Overall level:** from −12 dB to +20 dB static gain, every arm landed within 0.6 points of the
  baseline.
- **The 2026-11 volume-dependent failures were a bug,** sherpa-onnx's log floor (#3997, fixed in
  2.0.0), not a property of the model.

**For the VAD, level matters a great deal.** AMI's table microphone records at about −49 dBFS RMS,
and much of its quiet speech falls under Silero's 0.5 threshold. The same `vad:0.8/120` segmenter
went from 38.8% to 27.3% when the audio passed through a 3-second AGC first. Deletions fell
from 34.1% to 19.3%. A static +20 dB alone reached 29.5%. The live profile `vad:0.8/15` went from
38.7% to 27.9%. Energy cuts, which have no VAD, stay best at about 25%.
- **What this means for Folio:** wherever a VAD stays, as the live preview or as a fallback, it
  should judge a level-normalised copy of the audio, while the decoder can take either.
- **A caution:** laptop capture may already arrive louder than AMI's (OS mic gain), so the effect
  on real laptops may be smaller. That's inference, and rec. 6 measures it.

**Other evidence:**
- Older studies found AGC-style energy normalisation helps when the model is *trained* with it.
  The clearest case is PCEN, a per-band AGC front end, for far-field keyword spotting (Wang et al.,
  2017).
- An inappropriate gain strategy can raise WER (Sensors 2022).
- Nothing current shows that inference-time AGC helps a log-mel model like Parakeet, and our runs
  agree.
- **Never use analogue AGC** that moves the OS input volume. OpenWhispr turned it off because
  Chromium's AGC changed the system mic volume on Windows (#476).

### 3.3 Noise suppression and speech enhancement

**The known result holds for current models.** Single-channel enhancement in front of a modern ASR
model usually hurts:

- **The cause is artefacts, not leftover noise.** Iwamoto et al. (Interspeech 2022) split
  enhancement errors into leftover noise and artefacts, and found that **artefacts** do the damage.
  Adding back a scaled copy of the original signal ("observation adding") reliably reduces it.
- **It includes Parakeet.** "When De-noising Hurts" (Dec 2025) ran MetricGAN+ denoising in front of
  Whisper, **Parakeet**, Gemini Flash 2.0 and a medical model, on 500 recordings under nine noise
  conditions. The noisy original beat the enhanced audio in all 40 configurations, with semantic WER
  up by 1.1 to 46.6 points.
- **Whisper is U-shaped.** Huo et al. (2026) found that Whisper improves with *weak* enhancement
  (6.44 → 5.57 WER at a quarter of FRCRN's mask strength) and returns to the noisy baseline at full
  strength. A noise-robust recogniser is hurt more by distortion than by the noise.
- **Gains come only from enhancement trained for an ASR model.** Whisper-Fest (Samsung, ICASSP
  2026) gets 30% relative on AMI for Whisper small.en, but it is a mel-to-mel front end trained
  against a frozen Whisper encoder. It's specific to that model, not a drop-in denoiser.
- **Vendors agree.** Otter warns that noise-cancelling mics "can clip or distort voices". OpenWhispr
  turns noise suppression off to avoid "speech distortion". Meetily ships RNNoise disabled.

**Folio's own runs (3.1) agree.** RNNoise costs 4.3 points on room audio, on the long meetings and
on the short clips alike. Classical spectral NS costs 1.8 to 4.1. RNNoise is level on headset audio,
so the damage is specific to distant speech. Mixing 30% of the original back in recovers only 0.6
points.

**OS voice isolation** is the same kind of processing. That covers macOS Voice Isolation, Windows
Voice Focus and Studio Effects, Voice Clarity, and OEM AI noise cancelling. Some of these also steer
toward one talker, which is the opposite of what a table needs (3.4). The bench can't test them
directly, because they run only on live capture. Rec. 6 covers them.

### 3.4 Laptop mic arrays and OS voice processing

**Windows.** Capture streams go through the driver's signal processing mode.
- **Modes.** Microsoft defines Raw, Default, Speech and Communications for capture. "Raw capture
  streams must not include any time varying or adaptive processing, such as echo control, automatic
  gain control, or noise suppression." Default mode carries whatever APOs the OEM installed, and
  beamforming, NS, deep NS, AGC and dynamic range compression are all standard APO types.
- **Requesting raw.** An app asks for raw through `IAudioClient2::SetClientProperties` with
  `AUDCLNT_STREAMOPTIONS_RAW`. It can only do so when the endpoint's `RawProcessingSupported` property
  is true. Microsoft advises against raw "unless the implications are understood", because it skips
  the OEM's tuning. A Microsoft Q&A answer says Teams, Discord and Zoom often choose raw for this
  reason. One small project found the flag can return S_OK on an endpoint that ignores it, so read
  the effects state back.
- **Voice Clarity** (Windows 11 24H2) is on by default for apps in Communications mode. It applies
  when the OEM provides no Communications processing of its own. Folio's cpal stream doesn't ask for
  Communications, so it shouldn't get it. That's inference from cpal's defaults; we haven't checked
  it on hardware. The live-capture doc already lists Realtek "Voice Clarity" delivering zeros
  (Handy #2141). On 2026-11-22 another project found Voice Clarity and Voice Focus processing a USB
  input until it switched to raw.
- **OEM utilities.** These change the beam on the built-in array. Lenovo's Smart Noise Cancelling,
  for example, has these modes:
  - "Normal": "focuses on the voice of the person facing it".
  - "Multiple Voices": "captures multiple voices from an expanded range in front of the computer".
  - "Voice Recognition": keeps voices in their spatial positions.
  - "Only My Voice": filters out everyone but the enrolled user.

  **For a laptop in the middle of a table, the wrong mode can remove most of the room.** We found no
  measurement of how much, so that is inference.

**macOS.** The built-in mic on Apple Silicon MacBooks is always beamformed.
- **No raw path.** Apple lists a "three-mic array with … directional beamforming". Asahi Linux's
  docs show that the three PDM mics are omnidirectional and raw, and must be beamformed in software.
  On macOS that happens before any app sees the audio, with gain and a high-pass filter, and there's
  no public way to bypass it (Apple developer forums thread 704661).
- **Beam direction unknown.** Apple doesn't publish it. Since the processing is tuned for calls, it
  most likely favours the person at the keyboard. That's inference, and worth measuring (rec. 6).
- **Mic Modes.** Voice Isolation and Wide Spectrum apply only to apps that adopt them, mainly
  through Apple's voice-processing path. Plain Core Audio recorders such as Audacity and OBS don't
  get them. Folio records through cpal's plain Core Audio unit, so it should get Standard. That's
  inference from the same reports.

**Should an app turn OS voice processing off?** Every open-source transcriber we read does (OpenWhispr,
Meetily, Anarlog with headphones), and the enhancement evidence in 3.3 agrees. The exception is
**beamforming toward the room**, which is the one thing a laptop array can do that software after
it can't. NOTSOFAR's multi-channel systems halved single-channel error. But a laptop beam is aimed
by the OEM at one user, so for a laptop in the middle of the table, raw or a wide mode is the safer
default. We have no measurement of this, so rec. 6 measures it.

### 3.5 VAD thresholds, and models trained on far-field audio

**VAD.** The bench already settled the main question. Not gating beats every VAD setting we tried
on `ami-sdm-long`: 25.3% against 37.1–38.8%, and 200–300 ms of padding barely helps. OpenWhispr's
#2998 is an independent replication on Whisper: turning Silero off for meetings halved the errors.
The bench has no option for the Silero threshold (fixed at 0.5 in `engine/src/sherpa.rs`), so a
lower threshold is untested. But the runs in 3.1 show that the VAD's losses on AMI are mostly about
input level. Levelling the audio before the VAD recovers about 11 of the 13 points, which a
threshold change might partly copy. A levelled input is the more general fix, because it doesn't
depend on how loud a given laptop records.

**Where the remaining deletions are.** With energy cuts, 17.8% of reference words are still deleted
on `ami-sdm-long`: 1,265 of 7,109, from the existing `energy:30/5` results and the bench's own
normaliser.
- **Short words make up half.** 47% of the deleted words are one of 30 short backchannel or function
  words. The top two are "yeah" (96) and "okay" (66).
- **Inference: much of this is overlap.** It looks like overlapped backchannels and quiet
  interjections that a single-stream ASR drops. NOTSOFAR found "debate-style overlapping speech"
  hardest for single-channel systems.
- **So front-end processing can't recover most of the gap** to the headset's ~10%. Part of that gap
  is crosstalk, which no single-channel front end can undo on a CPU budget.

**NOTSOFAR-1 (CHiME-8) on single-channel far-field:**

| Eval tcpWER | single-channel | multi-channel |
|---|--:|--:|
| USTC-NERCSLIP (winner) | 22.2 | 10.8 |
| NPU-TEA | 30.0 | 18.7 |
| NAIST | 36.6 | 23.0 |
| Baseline (CSS + Whisper large-v2) | 41.4 | 28.3 |

- **Every top single-channel system used fine-tuned Whisper,** with diarization-conditioned
  separation. All are GPU-scale.
- **Fine-tuning on in-domain far-field data was the big lever.** USTC's dev tcpWER went from 16.57%
  to 9.87% after adding in-domain and simulated data.
- **The hardest conditions** were transient noise and laughter, then overlap. Lower DER didn't
  predict lower tcpWER.
- **Separation helped the baseline, but enhancement-style front ends didn't always help.** USTC's
  separation without GSS was "consistently worse", and STCON's target-speaker extraction "didn't
  yield better ASR accuracy" despite good-sounding output.

**Models in Folio's reach.** None of the open models with sherpa-onnx exports is trained
specifically for far-field.
- **Parakeet v2** has AMI in its training mix (model card; which microphones isn't stated). Its noise
  table holds up to SNR 5 dB: 8.2% against 6.1% clean.
- **icefall's AMI Zipformer,** trained on headset, SDM and GSS audio, scores 32.2% on AMI SDM test
  with its own scorer. It's no better.
- **Cohere Transcribe** is the most accurate on short `ami-sdm` clips in our grid (24.4%), but has
  no word timestamps. On the long meetings it scored 43.0% with `energy:30/5`, with repetition loops
  (section 5).
- **The far-field-specific route is fine-tuning Parakeet.** It would use NOTSOFAR-1's training
  meetings (CC-BY-4.0, about 55 h of single-channel device audio) plus simulated room audio, then an
  int8 export. That's GPU work once, and zero egress at run time. Heidi fine-tuned the same Parakeet
  v2 on 1,500 h of its own audio and cut WER from 11.5% to 9.4%. Keep AMI out of the training data,
  or the bench stops measuring anything.

### 3.6 Far-field diarization

**Distance costs diarization less than ASR.** Rows from pyannote's published benchmarks (no collar,
overlap scored):

| Pipeline | AMI headset mix DER | AMI SDM DER | of which confusion (headset → SDM) |
|---|--:|--:|--:|
| pyannote 3.1 | 18.8 | 22.4 | 5.7 → 7.5 |
| pyannote community-1 | 17.0 | 19.9 | – |

- **Three DER points against twenty WER points.** ASR goes from about 10% to 25–30% WER on the same
  meetings, but DER rises only three points.
- **Speaker counting and overlap are the bigger problems.** NOTSOFAR-1 found that "lower DER does
  not necessarily translate to lower tcpWER". Its winner self-reported a higher DER
  than the other top teams and won on ASR.
- **Folio's stack is simpler than pyannote's.** It uses pyannote segmentation-3.0 with the English
  VoxCeleb CAM++ and sherpa-onnx's threshold clustering (0.5), without pyannote's VBx-style
  clustering, so expect worse than the table (inference). Diarization is off by default in the
  engine because "measured speaker quality on real audio was poor enough that labels hurt"
  (`engine/src/lib.rs`).

**OpenWhispr has done Folio's homework on the same sherpa-onnx models** (`diarizationPolicy.js`,
`constants/speakerDetection.json`):

- **Embedding model:** the zh-en "advanced" CAM++ (`3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced`,
  #2452) instead of the English VoxCeleb one Folio ships.
- **Threshold that grows with length:** cosine-distance threshold 0.6 for short audio, rising to 0.7
  between 15 and 60 minutes, because one voice's embeddings drift over a long recording. With the
  old model, a 73-minute memo produced 46 "speakers".
- **Pruning phantoms:** clusters with under 1 s of speech are dropped. "Phantom" clusters, under 10%
  of the speech and averaging turns under 3 s, are folded in, but never the two largest clusters.
- **Collapse check:** a run where one cluster holds over 93% of the speech, with every other cluster
  made of short turns, is treated as two real speakers merged into one.
- **Merging by voice:** voices are merged by speech-weighted centroid similarity of at least 0.5.
  Measured on AMI meetings (#2641), that gave the right speaker count on 12 of 15 recordings of 1–4
  voices, where 0.6 got 7.
- **Asking the user:** an expected-speaker-count stepper (default 2, max 8) seeds the clustering.

Folio's engine already accepts `num_speakers`, so asking "how many people are speaking?" when
recording starts costs one UI field.

## 4. What to tell users

**How others phrase it:**
- **Granola:** "Keep the laptop microphone facing the conversation", "roughly 6 to 12 inches from
  the primary speaker".
- **Otter:** "Place the microphone in the center of all speakers", an omnidirectional external mic,
  "within three feet".
- **Circleback:** a USB conference mic for regular in-person meetings.

All of them lead with placement and hardware, not software.

**Hardware worth naming** (UK prices checked 2026-11-23):

| Device | Mics, pickup | Price |
|---|---|---|
| Anker PowerConf S330 | 4 mics, 360°, "within 3m" | about £70–75 new, £59 renewed |
| Jabra Speak2 40 | 4 MEMS mics, up to 2.3 m; ideal room about 4.5 × 4.5 m | about £85–90 inc VAT, RRP £127 |

Both are plain USB audio devices, so they need no driver and nothing leaves the laptop. Both run
their own echo and noise processing. Whether that helps or hurts Parakeet is untested (rec. 6).
**Avoid Bluetooth headsets for rooms.** They're tuned to the wearer, often narrowband, and Folio
already warns about them.

**Draft copy for Folio's "Recording a meeting in a room" tip.** It uses Folio's voice: short
sentences, with no promise we haven't measured.

> **Recording people in the same room**
> - Put the laptop in the middle of the table, lid open, with nothing between it and the people speaking.
> - If one person speaks quietly, put the laptop nearer to them.
> - Keep it away from fans, projectors and walls. Don't type on it while it records.
> - Folio hears one voice at a time best. When people talk over each other, some words will be missing.
> - Rooms with more than about six people, or longer than about 3 metres: use a USB conference
>   speakerphone (from about £60) in the middle of the table and choose it in Folio.
> - On Windows laptops with noise-cancelling settings (for example Lenovo Vantage), choose the mode
>   for several voices, not "only my voice".
> - Tell Folio how many people are speaking: it helps it tell them apart.

The six-person and 3-metre figures come from the speakerphones' rated pickup (2.3–3 m) and
Granola's and Otter's advice. Neither has been measured for a bare laptop. Rec. 6 measures it.

## 5. Recommendations for Folio, ranked

Every recommendation keeps all audio on the laptop, runs on the CPU, and adds no model that won't
fit beside Parakeet in 8 GB. They are ranked by expected accuracy gain for the effort.

**1. Make the room-mic record ungated, for recordings as well as files.** This is the largest gain
available, and it's already measured. Recordings keep the live utterances (`vad:0.8/15`) as the
record, and they score 38.7% on `ami-sdm-long` against 24.9–25.3% for energy cuts. There are two
ways to get there:
- **(a) Pieces as the record.** Live capture cuts 15 s energy pieces as audio arrives and decodes
  each one. The VAD stays only for the speech indicator. This is v2t's design, and it makes both the
  record and the live text ungated.
- **(b) Re-decode on stop.** Keep today's live text as a preview, and re-decode the WAV with
  `energy:30/5` on stop. This is D3 in the voice2text doc.

Experiment:
- **Mac:** `ami-sdm-long` with `energy:10/3` and `energy:15/3`, to see how short a piece can get
  before accuracy drops. Shorter pieces mean fresher live text for (a).
- **Windows floor laptop:** RTF and peak memory of `energy:15/5` against `vad:0.8/15`, to check that
  (a) keeps up in real time. For (b), the wait after stop on the 55-minute set.

Expected: about −13 points WER on room audio (38.7 → ~25), at about 1.7× the decode work (RTF 0.067
against 0.040 on the Mac) and lower peak memory.

**2. Wherever a VAD stays, give it a levelled copy of the audio.** This is the quick win, and it
can ship before rec. 1.
- **The change.** Run a fast digital AGC (about a 3 s window, up to ~30 dB of gain, a peak limiter)
  on a copy of the audio that only the VAD sees. The decoder can keep the original. It's a few dozen
  lines of Rust, with no model and no measurable CPU.
- **Measured (3.1):** the live profile `vad:0.8/15` went from 38.7% to 27.9% on `ami-sdm-long`, and
  the file profile `vad:0.8/120` from 38.8% to 27.3%. Deletions fell from 34% to 19%.
- **Where it matters:** the live preview, and today's live record until rec. 1 lands.

Experiment:
- **Bench option.** Add `vad-agc:<silence>/<max>`: segment on the levelled copy, decode the
  original.
- **Sets.** Run it on `ami-sdm-long`, and on `earnings22-long` to check that clean audio doesn't
  regress or gain insertions. Insertions rose only from 0.6% to 1.2% here.
- **Windows floor laptop:** confirm the live RTF is unchanged.

Expected: about −11 points on room audio for the VAD paths, and no change on close-talk audio.

**3. Keep the decoded signal raw. No denoiser, no AGC on what the decoder hears, and ask Windows for
raw capture.** This is mostly
about avoiding harm.
- **Measured (3.1):** RNNoise costs 4.3 points on `ami-sdm-long`, and FFT noise reduction 1.8. AGC
  changes nothing beyond the noise of a ±1-point re-run. The literature agrees (3.3).
- **Mac:** Folio already records with cpal's plain Core Audio unit, which avoids Apple's voice
  processing and Mic Modes (inference). Keep it that way.
- **Windows:** cpal opens a shared stream in the default mode, so OEM processing applies. Add raw
  capture when the endpoint's `RawProcessingSupported` is true. That means `IAudioClient2::
  SetClientProperties` before `Initialize`, which cpal doesn't expose, so it needs a patch or a
  small WASAPI path. Read back whether effects are still active, and log it with the recording.
- **Never touch the OS input volume.** OpenWhispr's #476 shows what Chromium's analogue AGC does to
  it.

Experiment. On the Windows reference laptop, run rec. 6's replay recordings in default mode against
raw. Score WER, empty pieces and digital-silence warnings. Expected: average WER level or better
with raw, and fewer failures caused by processing. The failures we know of are zeros from Voice
Clarity and Realtek effects. Also rerun the 3.1 arms on the Windows laptop, to check that the Mac
results carry over. Expected: the same (RNNoise worse, AGC level).

**4. Diarization for rooms: copy OpenWhispr's clustering policy and ask for the head count.**
Speaker labels are off by default because they hurt. OpenWhispr runs the same sherpa-onnx models and
has published, measured fixes (3.6).

Experiment. Add DER and word-level speaker error to the bench for `ami-sdm-long`. AMI's manual
speaker segments are CC-BY-4.0, and 4 people speak in each meeting. Arms:
- today: English CAM++, threshold 0.5, automatic count;
- the zh-en advanced CAM++ at 0.6, rising to 0.7 between 15 and 60 minutes;
- that, plus phantom pruning and centroid merging at 0.5;
- each of the above with `num_speakers = 4`.

Expected: the right count does most of the work. OpenWhispr's merge got it right on 12 of 15 AMI
recordings, where a merge threshold of 0.6 got it right on 7. Turn labels on by default only if
word-level
speaker error on `ami-sdm-long` is low enough that a reviewer fixes a few labels, not most.

**5. Tell users how to set up the room, and name a £60–90 USB speakerphone for bigger rooms.**
Granola, Otter and Circleback all lead with this. It costs one help page and a tip on the Record
screen (draft in section 4), plus the "how many people?" field from rec. 4. No bench run is needed
to ship the wording. Rec. 6 supplies the distance and room-size numbers for it.

**Checked and dropped for rooms: Cohere Transcribe on long meetings.** The voice2text doc's
two-pass idea rested on Cohere's 24.4% on short `ami-sdm` clips. On the long meetings with
`energy:30/5`, Cohere scored **43.0%**, against Parakeet's 25.3%: 31.8% on IS1009a, 45.8% on
ES2004a and 48.8% on TS3003a. It also fell into repetition loops, such as "the only one who is not
the…" 84 times and "government of the United States" 29 times, and dropped whole stretches.
- **Why:** this is the hallucination its card warns about, on 30 s pieces of quiet, far audio.
- **What it means:** Cohere would need VAD-gated short pieces and a repetition guard before any
  two-pass run. With the AGC'd VAD from rec. 2, that is now testable:
  `--model cohere-transcribe --segmenter vad-agc:0.8/15`.
- **Until then:** don't plan an accuracy pass around it.

**6. Build a "laptop in the room" replay set before tuning capture.** `ami-sdm` is an
omnidirectional microphone in the middle of the table, not a laptop's beamformed array. Nothing in
the bench tells us:
- how far a laptop really reaches;
- whether people behind the screen fade;
- whether Windows raw mode, OEM modes or a speakerphone help.

Method:
1. Play the 150 `ami-ihm` clips, which have headset references and score about 10%, from a small
   speaker.
2. Place the speaker at 1, 2 and 3 m from the laptop, in front of the screen and behind it.
3. Record with Folio's own capture on the MacBook and the Windows reference laptop:
   - Windows in default mode, raw mode and each Lenovo mode;
   - an Anker S330 or Jabra Speak2 40 in place of the laptop mic.
4. Score each recording against the `ami-ihm` references with `whole`.

Expected: a distance and direction curve for each device. It decides how much rec. 2 gains on real
laptops, settles rec. 3,
and gives rec. 5's wording its numbers.

**7. Research option: fine-tune Parakeet v2 on far-field data.** Use NOTSOFAR-1's single-channel
training meetings (CC-BY-4.0) plus simulated room audio, then an int8 sherpa-onnx export.
- **Cost:** GPU time once. Nothing changes at run time, and egress stays zero.
- **Constraint:** keep AMI out of training so `ami-sdm` stays a test.
- **Expected:** a 10–20% relative gain on room audio, by analogy with Heidi's 18% from 1,500 h of
  in-domain data and NOTSOFAR's fine-tuning results. That's inference, not a measurement.
- **Licence:** Parakeet v2 is CC-BY-4.0, so a fine-tuned derivative can ship with attribution.
- **Order:** only after rec. 6 shows how much of the gap is acoustic rather than overlap.

**Not recommended:**
- **A denoiser** (RNNoise, DeepFilterNet, WebRTC NS, OS voice isolation) in front of ASR.
- **AGC or loudness normalisation on the decoder's input.** It's neutral (3.1). Rec. 2 uses it for the VAD only.
- **A lower VAD threshold as a substitute for rec. 1.** If the live preview keeps the VAD, a
  threshold option in the bench (`vad-th`) would show whether 0.3–0.35 helps the preview. It
  shouldn't decide the record.

## Sources

**Products: source code** (read at the commits in the header)
- OpenWhispr: <https://github.com/OpenWhispr/openwhispr>. Files: `src/helpers/microphoneSelection.js`,
  `src/stores/meetingRecordingStore.ts`, `native/meeting-aec-helper/src/aec_processor.cc`,
  `src/helpers/meetingMicGate.js`, `src/helpers/ipcHandlers.js` (`transcribeLocalMeetingChunk`),
  `src/helpers/diarizationPolicy.js`, `src/constants/speakerDetection.json` and `CHANGELOG.md` (#1726,
  #2452, #2719, #2782). Issue #2998: <https://github.com/OpenWhispr/openwhispr/issues/2998>
- Meetily: <https://github.com/Zackriya-Solutions/meetily>. Files:
  `frontend/src-tauri/src/audio/pipeline.rs`, `audio_processing.rs` and `ffmpeg_mixer.rs`.
- Anarlog: <https://github.com/fastrepl/anarlog>. Files: `tools/meeting-capture/README.md` and
  `crates/vad-masking`.

**Products: documentation and reviews**
- Granola security: <https://www.granola.ai/security>
- Granola, "How transcription works": <https://docs.granola.ai/help-center/taking-notes/transcription>
- Granola, "Transcription issues": <https://docs.granola.ai/help-center/troubleshooting/transcription-issues>
- Granola, AI note taker for in-person meetings: <https://www.granola.ai/blog/ai-note-taker-in-person-meetings>
- Granola sub-processors (aggregator): <https://sub-processors.com/company/granola.ai>
- Circleback (competitor), in-person workarounds: <https://circleback.ai/how-to/recording-in-person-meetings-with-granola-workarounds>
- HappyScribe (competitor), Granola review: <https://www.happyscribe.com/blog/granola-ai-review>
- Rimo (competitor), Granola review: <https://rimo.app/en/blogs/granola-review_en-US>
- Otter, best practices for in-person recordings: <https://help.otter.ai/hc/en-us/articles/31672594631063-Getting-the-most-out-of-Otter-ai-Best-Practices-for-in-person-recordings>
- Otter, accuracy tips: <https://help.otter.ai/hc/en-us/articles/18934488820887-Tips-on-improving-speech-transcript-accuracy>
- Otter, speaker identification: <https://help.otter.ai/hc/en-us/articles/37817241040535-Best-Practices-to-Maximize-Speaker-Identification>
- Jamie Trust Center: <https://trust.meetjamie.ai/>
- Jamie, in-person meetings: <https://www.meetjamie.ai/blog/ai-notetakers-for-in-person-meeting>
- Deepgram Nova-3 launch (vendor): <https://www.reworked.co/the-wire/introducing-nova-3-extending-deepgrams-leadership-in-voice-ai-for-enterprise-use-cases/>

**Research**
- NOTSOFAR-1 challenge paper: <https://arxiv.org/abs/2401.08887>
- Summary of the NOTSOFAR-1 Challenge: <https://arxiv.org/abs/2501.17304> (HTML: <https://arxiv.org/html/2501.17304>)
- USTC-NERCSLIP CHiME-8 NOTSOFAR-1 system: <https://arxiv.org/abs/2409.02041>
- NOTSOFAR dataset (CC-BY-4.0): <https://huggingface.co/datasets/microsoft/NOTSOFAR>
- Iwamoto et al., "How bad are artifacts?", Interspeech 2022: <https://arxiv.org/abs/2201.06685>
- Chondhekar et al., "When De-noising Hurts", 2025: <https://arxiv.org/abs/2512.17562>
- Huo et al., "Where Speech Enhancement Hurts Recognition", 2026: <https://arxiv.org/abs/2607.11157>
- Whisper-Fest, Samsung Research (ICASSP 2026): <https://research.samsung.com/blog/WHISPER-FEST-Single-Channel-Far-Field-Enhanced-Speech-to-Text-without-Parallel-Data>
- icefall AMI recipe results: <https://github.com/k2-fsa/icefall/blob/master/egs/ami/ASR/RESULTS.md>
- Wang et al., "Trainable frontend for robust and far-field keyword spotting" (PCEN), ICASSP 2017: <https://arxiv.org/abs/1607.05666>
- "ASR performance improvement … optimizing gain control strategy", Sensors 2022: <https://www.mdpi.com/1424-8220/22/8/3027>
- RNNoise model used in the runs (`sh.rnnn`): <https://github.com/GregorR/rnnoise-models>

**Models**
- Parakeet TDT 0.6B v2 model card: <https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2>
- pyannote speaker-diarization-3.1: <https://huggingface.co/pyannote/speaker-diarization-3.1>
- pyannote speaker-diarization-community-1: <https://huggingface.co/pyannote/speaker-diarization-community-1>

**OS audio processing**
- Microsoft, Audio signal processing modes: <https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/audio-signal-processing-modes>
- Microsoft, `AUDCLNT_STREAMOPTIONS`: <https://learn.microsoft.com/en-us/windows/win32/api/audioclient/ne-audioclient-audclnt_streamoptions>
- Microsoft Q&A, communication apps and raw mode: <https://learn.microsoft.com/en-us/answers/questions/5783329/why-do-communication-apps-teams-discord-default-to>
- Voice Clarity announcement (Neowin): <https://www.neowin.net/news/windows-11-insider-canary-chanel-build-26040-adds-voice-clarity-feature-and-more/>
- Raw capture fix after Voice Clarity (Kairo PR #42): <https://github.com/A-Miide/Kairo/pull/42>
- Lenovo Smart Noise Cancelling modes: <https://download.lenovo.com/pccbbs/pubs/ideapad_slim5_14_16/ug/html_en/en/smart_noise_cancelling.html>
- Apple MacBook Pro tech specs (three-mic array, beamforming): <https://support.apple.com/en-us/111902>
- Apple, Use Mic Modes on your Mac: <https://support.apple.com/guide/mac-help/use-mic-modes-on-your-mac-mchle82b42f0/mac>
- Asahi Linux, userspace audio (raw MacBook mics need beamforming): <https://asahilinux.org/docs/sw/audio-userspace/>
- Apple Developer Forums, bypassing built-in mic processing: <https://developer.apple.com/forums/thread/704661>

**Hardware**
- Anker PowerConf S330: <https://www.soundcore.com/products/a3308011>, UK prices <https://pricespy.co.uk/product.php?p=6678661>
- Jabra Speak2 40 UK prices: <https://pricespy.co.uk/product.php?p=9865830>. Room size:
  <https://www.notebookcheck.net/Jabra-Speak2-40-speakerphone-for-Teams-and-Zoom-remote-work-meetings-drop-to-lowest-price-ever.962139.0.html>

**Folio**
- `docs/research/voice2text-lessons-2026-11.md`, `live-capture-landscape-2026-11.md`,
  `windows-device-tiers-2026-11.md`, `scribe-landscape-2026-09.md`
- `tools/asr-bench` (README, `src/segment.rs`), `engine/src/lib.rs`, `engine/src/sherpa.rs`
- Parakeet v2 int8 encoder metadata: `normalize_type = per_feature`
  (`sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/encoder.int8.onnx`)
