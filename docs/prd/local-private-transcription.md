# PRD: Privacy-First Local Transcription & Review App (Windows, law-firm-grade)

> Status: ready-for-agent · Recorded 2026-06-10 · Source: design + two deep-research passes + grill-me session.
> Builds on the existing `~/podcast-transcribe` Python pipeline (WhisperX + pyannote), which remains for
> internal/high-fidelity use but is the wrong *shape* to ship to clients.

## Problem Statement

A UK law firm needs to transcribe sensitive, privilege-bearing audio (client interviews, meetings, dictation)
but cannot send that audio to any cloud service. Existing options fail them: cloud transcribers (Otter,
Fireflies, Deepgram, ElevenLabs) breach confidentiality by definition; the developer's current pipeline
requires Python, ffmpeg, a Hugging Face token, and an hour of CPU time — unusable by a non-technical fee-earner.
There is also no pleasant way to *review and correct* a machine transcript: today the output is a static `.docx`
that's painful to fix and re-attribute speakers in. The firm needs something its IT can deploy on locked-down
Windows laptops, that provably keeps data on the device, and that a fee-earner can actually use to clean up a
transcript quickly.

## Solution

A self-contained Windows desktop app that transcribes and diarizes audio **entirely on the device**, with a
**sentence-level review editor** for fast correction and speaker relabeling. It makes **zero outbound network
connections** (verifiable in airplane mode), so client data never leaves the laptop and the vendor never receives
it — which means the vendor is, by design, **not a data processor** under UK GDPR. It installs as a **signed MSI**
the firm's IT pushes via Intune/SCCM, with models **bundled in the installer** so it works offline from first
launch. New community models arrive as **signed, versioned model packs** dropped into a models folder (or pushed
by IT) — no app reinstall, no internet fetch in the hardened build. Output is always positioned as a
**machine-generated draft requiring human verification**, and the review editor is the control that makes that safe.

## User Stories

1. As a fee-earner, I want to transcribe an audio file fully on my laptop, so that privileged client material never leaves the device.
2. As a fee-earner, I want the app to work with no internet connection, so that I can use it on a train or in a secure room and trust nothing is uploaded.
3. As a fee-earner, I want to open an audio file and get a speaker-separated transcript, so that I can see who said what.
4. As a fee-earner, I want to play the audio while the spoken word highlights in the transcript, so that I can follow along and spot errors.
5. As a fee-earner, I want to change playback speed (0.75×–2×), so that I can review faster or slow down for unclear passages.
6. As a fee-earner, I want to click a word to jump the audio to that moment, so that I can re-listen to a specific phrase.
7. As a fee-earner, I want to double-click a sentence and retype it freely, so that I can fix transcription errors quickly without fiddling with individual words.
8. As a fee-earner, I want to add words the transcriber missed by typing them into a sentence, so that the record is complete.
9. As a fee-earner, I want to delete junk text, so that the transcript reads cleanly.
10. As a fee-earner, I want to rename a speaker once and have it apply everywhere, so that I can turn SPEAKER_03 into the client's name in one action.
11. As a fee-earner, I want to reassign a single sentence to a different speaker, so that I can fix diarization mistakes mid-conversation.
12. As a fee-earner, I want to export the corrected transcript (DOCX/JSON/text), so that I can use it in my matter file.
13. As a fee-earner, I want every export to carry a "machine-generated draft — verify before reliance" notice, so that nobody mistakes it for a certified transcript.
14. As a fee-earner, I want a one-click "delete this matter" that removes the audio and transcript, so that I can dispose of client data when done.
15. As a fee-earner, I want the app to warn me if disk encryption (BitLocker) is off, so that I don't store privileged data unprotected.
16. As a fee-earner, I want to resume a review session where I left off, so that long recordings don't have to be done in one sitting.
17. As firm IT, I want a signed MSI I can deploy via Intune/SCCM, so that I can roll it out under our managed-software policy.
18. As firm IT, I want the app to install without requiring each user to have admin rights, so that it fits our locked-down laptops (per-user fallback).
19. As firm IT, I want a build with networking compiled out, so that I can prove to our security team the app cannot connect to anything.
20. As firm IT, I want to push new model packs through the same channel as the app, so that updates fit our existing change-management.
21. As firm IT, I want the app to refuse unsigned or tampered model packs, so that nobody can inject an untrusted model.
22. As firm IT, I want no auto-updater, so that the app never reaches out and updates happen only when I deploy a new signed MSI.
23. As a compliance officer/DPO, I want a data-flow diagram and a "data never leaves the device / vendor is not a processor" statement, so that I can complete our DPIA and risk file.
24. As a compliance officer, I want airplane-mode verification steps, so that I can independently confirm the zero-egress claim.
25. As a compliance officer, I want a list of bundled third-party components and their licenses, so that I can clear it through procurement.
26. As a security reviewer, I want the source to be open and auditable, so that I can verify there is no hidden telemetry.
27. As a fee-earner needing support, I want to export diagnostics to a local file that I can inspect before sending, so that I never leak content unknowingly.
28. As the vendor, I want to adopt a new community model by converting, quantizing, evaluating, license-vetting, and signing it, so that shipping a better model is a config step, not a rebuild.
29. As the vendor, I want each model pack to carry a manifest (id, version, engine-min-version, license, attribution, hash), so that the app can validate compatibility and display attribution.
30. As the vendor, I want the engine behind a stable adapter interface, so that I can switch between whisper.cpp and sherpa-onnx without touching the rest of the app.
31. As an open-segment SME user (non-locked-down), I want an optional in-app model catalog I can explicitly opt into, so that I can fetch new models conveniently when my environment allows it.
32. As a fee-earner, I want the app to auto-detect the number of speakers, so that I don't have to know it in advance for a multi-party interview.
33. As a fee-earner, I want the default bundled model to run at acceptable speed on a CPU-only laptop, so that I'm not waiting excessively for a transcript.

## Implementation Decisions

**Stack & architecture**
- Desktop shell: **Tauri** (Rust backend + web frontend in a native webview). Fork **Vibe** (MIT) as the base, or harvest its proven pieces (whisper-rs integration, model manager, installer config).
- Engine layer is **Python-free**: ASR via **whisper.cpp** (ggml) and/or **sherpa-onnx** (ONNX); diarization via **sherpa-onnx**. The existing WhisperX/pyannote Python pipeline is retained only for internal high-fidelity work, not shipped.
- Strip Vibe's networked features (URL ingestion, cloud summarization, updater). The current Python diarization path in Vibe (if pyannote-based) is removed in favor of sherpa-onnx.

**Privacy & deployment (settled in grill-me)**
- **Zero outbound network by design.** No telemetry, crash reporting, analytics, or update pings. Diagnostics are local-only and user-initiated.
- Consequence: data never leaves the device and the vendor never receives it → **vendor is not a UK GDPR data processor** (no DPA, no international-transfer analysis, trivial DPIA). This is the core commercial differentiator.
- **Models bundled in the installer** for a default airplane-mode-clean first launch.
- **IT-managed deployment**: signed **MSI** for Intune/SCCM + a **no-admin per-user installer** fallback (installs to %LOCALAPPDATA%).
- **Code signing** via a standard **OV Authenticode certificate** (sufficient for IT-managed distribution). Prerequisite: a registered company entity to hold the cert.
- **Data at rest**: rely on the firm's **BitLocker** full-disk encryption (documented dependency); the app checks BitLocker status on launch and warns if off. No app-level encryption (avoids key-management liability).
- Working/temp files (ffmpeg decode, model scratch) live on the encrypted volume in the app's own directory and are wiped on completion — never an arbitrary system temp location.
- **No auto-updater**; IT pushes a new signed MSI.

**Model-update system**
- Models are **decoupled, signed, versioned model packs** in a `models/` directory — not baked into the app binary. Updating = drop in a new signed pack; the app auto-detects it. No app reinstall.
- Each pack carries a manifest (decision-encoding schema, from design):
  ```json
  {
    "id": "whisper-medium-en",
    "version": "1.2.0",
    "engine": "whisper.cpp",      // or "sherpa-onnx"
    "engine_min_version": "1.7.0",
    "task": "asr",                 // or "diarization"
    "license": "MIT",
    "attribution": "OpenAI Whisper",
    "sha256": "…",
    "signature": "…"               // vendor signature over the manifest+weights
  }
  ```
- The app **verifies the signature and hash before loading** and **refuses unsigned/tampered packs** (app embeds the vendor public key).
- **Dual delivery from one codebase**: (1) offline import (watch folder / file-picker, zero network — law-firm path, IT-pushed); (2) optional in-app catalog from the vendor's signed server, explicit and **off by default** (open-SME segment). A **hardened build variant compiles networking out** entirely for clients requiring a provably-offline binary.
- **Honest bound**: painless updates apply to weights for architectures the engine already supports; a genuinely new architecture requires an engine upgrade = a new app/MSI release.
- **License-vetting is a release gate** in the vendor pipeline: reject CC-BY-NC / gated weights (e.g. the Rev `reverb-diarization` model). Prefer MIT segmentation (pyannote/segmentation-3.0) + a permissively-licensed embedding model; Parakeet weights are CC-BY-4.0 (attribution required and surfaced in-app).

**Diarization specifics**
- sherpa-onnx pipeline = converted-pyannote segmentation (MIT) + a vetted speaker-embedding model + clustering.
- **Auto-detects speaker count** via cluster threshold (no need to know count in advance); optional explicit count when known.
- Accepted accuracy tier ≈ open pyannote-3.1 (~11.5% English DER) — meaningfully below pyannote's paid API (6.6%), but acceptable because the **human relabel step is the accuracy backstop**.

**Transcript contract & editing model**
- Engine→editor contract is the existing word-level sidecar shape (decision, already in use):
  ```json
  { "segments": [ { "start": 0.0, "end": 4.2, "text": "...", "speaker": "SPEAKER_00",
                    "words": [ { "word": "I", "start": 3.44, "end": 3.54, "speaker": "SPEAKER_00" } ] } ] }
  ```
- **Word-level timestamps are a playback aid only, not an edit surface.** Native engine timestamps (±200–500ms) are sufficient; forced alignment (WhisperX/wav2vec2) is **not** required — this is what allows the Python-free engine.
- **Editing is sentence-level**: double-click a sentence → edit as plain text → on commit, re-tokenize and re-spread the sentence's time span across the new words (so highlighting keeps working).
- **Speaker relabel is global** (positional labels map to display names); **per-sentence speaker reassignment** is also supported for mid-turn diarization fixes.
- Turn building uses **duration-weighted speaker majority per sentence** (ported from `transcribe.py`), preventing mid-sentence speaker flips.

**Output positioning / liability**
- All output is a **machine-generated draft requiring human verification**; a disclaimer is shown in the UI and embedded in every export header. The review/edit pass is a first-class, expected step. Liability is capped contractually.

**Assurance**
- Ship a **compliance/assurance pack**: data-flow diagram (zero egress), non-processor statement, airplane-mode verification steps, bundled-component + license list, BitLocker dependency note, DPIA-support template, plus the open-source repo for deep audit.

## Testing Decisions

**What makes a good test here:** assert on *external behavior* and contracts, not implementation details — given an input fixture, the module produces the expected structured output. Use real recorded sidecars (`out/*.json`) and small synthetic fixtures as inputs. Avoid asserting on internal data shapes that may change.

Modules to be tested (confirmed):
1. **Transcript/turn builder** — pure functions `splitSentences(words)` and `buildTurns(words)`. Tests: sentence boundaries on `.?!`; duration-weighted majority assigns the right speaker to a mixed-speaker sentence; consecutive same-speaker segments merge into one turn; punctuation/whitespace cleanup. **Prior art:** the existing `transcribe.py` `build_turns` / `split_sentences` / `_SENT_END` logic — port and pin its behavior with fixtures derived from `out/fraudster_v3.json`.
2. **Model-pack manager** — `list/verify/load`. Tests: a correctly-signed pack with matching sha256 loads; a tampered weights file is rejected; an unsigned pack is rejected; a manifest with `engine_min_version` newer than the app is refused with a clear reason; attribution surfaces from the manifest.
3. **Diarization speaker-assignment** — `assignSpeakers(words, speakerSegments)` (the mapping logic, **not** the ML model). Tests: each word gets the speaker whose segment overlaps it; words in gaps inherit the nearest/again-weighted speaker; boundary words resolve deterministically. Fixtures: synthetic word + segment lists with known expected assignments.
4. **Editor behavior** — sentence edit + relabel + export. Tests: editing a sentence rebuilds its words with timestamps re-spread across the original span (monotonic, within the sentence's [start,end]); emptying a sentence deletes it; global rename updates all occurrences of a speaker; per-sentence reassignment changes only that sentence; export JSON/text reflects live edited text and renamed speakers. **Prior art:** the existing `karaoke_editor.html` prototype encodes the intended behavior.

## Out of Scope

- Cloud/streaming/real-time transcription; any server-side or multi-user central deployment.
- Mobile (the target is Windows desktop; macOS/Linux are incidental from the Tauri base, not a requirement).
- App-level encryption of stored files (rely on BitLocker).
- Court-certified / verbatim transcription guarantees — output is explicitly a draft.
- Languages beyond English-first (architecture should not preclude it, but it isn't a launch requirement).
- Automatic engine/architecture upgrades — capability jumps ride app releases, not model packs.
- Demucs speech-under-music isolation (an existing pipeline capability; optional/internal, not part of this product's core path).
- The vendor's commercial/pricing model (separate decision).

## Further Notes

- **Open empirical risk (do this first):** the sherpa-onnx diarization accuracy and CPU speed are *inferred*, not measured on representative legal audio. Run a half-day spike on a real (or representative) legal clip — measure transcription RTF on CPU, diarization quality vs the existing pyannote output, and the word-highlight feel — before committing promises. Misattribution is the likeliest failure; risk shrinks if the firm's audio is mostly 1–2 speakers (dictation/client calls) vs many-party interviews.
- **Prerequisite:** a registered company entity to hold the OV code-signing certificate.
- **Commercial model (unconfirmed):** likely paid setup + the compliance pack + support + customization around an open-source core.
- "Just download and set up" at a law firm realistically means "IT packages and pushes a signed MSI" — set this expectation with the buyer.
- Sequenced next steps: (1) sherpa-onnx spike on real audio; (2) inspect Vibe source for a concrete fork plan (Tauri version, diarization path to remove, network features to strip, files to touch); (3) scaffold the compliance/assurance pack; (4) design the model-pack format + signing/verification.
- Related internal note: Demucs `--isolate-vocals` recovers speech-under-music in the existing pipeline (see project memory).
