# PRD: Folio next: live capture, local notes, source-linked verification

> Status: proposed · Recorded 2026-09-25.
> Source: the landscape research in `docs/research/scribe-landscape-2026-09.md`.
> Go/no-go gates: `docs/spikes/next-phase-spikes.md`. **No phase starts until its spikes pass.**
> Builds on the original local-transcription PRD (removed from the tree in 59ba8a8; see git history). That PRD listed "streaming/real-time
> transcription" as out of scope. This one **reopens live capture only**. It stays fully
> on-device: still no cloud, no server and no network.

## Problem Statement

Folio today transcribes a recording **after the fact** and produces a verbatim draft for a human to
check. The organisations we serve (law firms, charities, social-care and advice services) mostly
don't want a transcript as the end product. They want the **note**: an attendance note, a case note, a
Care Act-style assessment. They also want it **during or straight after** the conversation, without
first exporting a file and importing it.

The cloud tools that do this (Heidi, Beam Notes, i.AI Minute / Local Transcribe) show strong
demand and real time savings. They also show three problems we can address:

1. **Every one sends audio and text to third-party AI sub-processors.** For privileged legal material
   and safeguarding conversations, that is the barrier.
2. **Their notes hallucinate and omit things.** Oxford University Hospitals found 6.2% of uses contained
   hallucinations. Ada Lovelace found fabricated suicidal ideation and garbled accounts in social-work
   records. Nothing lets a reviewer check a note sentence against the moment it was said.
3. **They are expensive or closed to our users.** Beam starts at £30k/yr. Minute and Local Transcribe
   are public-sector only.

## Solution

Extend Folio so a user can **record a conversation in the app**. They see the transcript build up as they talk,
and when they stop they get a **draft note from a template**, generated **on the device** by a local LLM. **Every
sentence of the note links to the part of the transcript and audio it came from.** The verbatim
transcript stays the primary record, and the note is a draft layered on top of it that the reviewer checks
sentence by sentence. Zero egress, signed model packs, the non-processor position and the assurance pack
all carry over unchanged.

Delivered in phases, each gated by spikes:

| Phase | Capability | Gated by |
|---|---|---|
| **P1** | Live mic capture + incremental transcription; diarisation on stop | S1, S6 (S7 only as a fallback) |
| **P2** | Local LLM notes from templates | S2, S4, S5 |
| **P3** | Source-linked note verification | S3 |
| **P4** | Call capture (Teams/Zoom system audio) | S8 |

## User Stories

**Live capture (P1)**
1. As a caseworker on a home visit, I want to press Record in Folio with no internet connection, so that I can capture the conversation without relying on signal.
2. As a fee-earner, I want to see the transcript appear while we talk (a few seconds behind), so that I know the capture is working.
3. As a user, I want to pause and resume a recording, so that off-record moments aren't captured.
4. As a user, I want the recording to survive an app crash or a flat battery (up to the last few seconds), so that I don't lose a 60-minute interview.
5. As a user, I want speaker detection to run after I stop, with a speaker-count hint (usually 2) that I can set or change at any point during the recording, so that the finished transcript is labelled without the live view slowing down.
6. As a user, I want the finished recording to open in the existing review editor and Transcripts view, so that correction, export and "Delete transcript and audio" work as they do today.
7. As a user, I want a clear on-screen recording indicator and an optional spoken-consent prompt, so that everyone present knows they are being recorded.
8. As a fee-earner, I want to type private notes while recording, kept with the transcript afterwards, so that I can capture what I noticed without a separate notebook. (P2 can later use them as an extra input.)

**Local notes (P2)**
9. As a caseworker, I want to pick a template (e.g. "Care assessment", "Attendance note", "Case note", "General minutes") and get a draft note when I stop recording, so that I'm not writing it from scratch.
10. As a team lead, I want to create and edit templates (sections plus instructions), so that notes match our organisation's format.
11. As a user, I want to regenerate one section, or the whole note, after correcting the transcript, so that fixes flow through.
12. As a user, I want note generation to show honest progress and let me carry on reviewing the transcript meanwhile, so that a slow CPU doesn't block me.
13. As a user, I want to export the note (DOCX/text) with the machine-draft disclaimer, alongside or without the transcript.

**Source-linked verification (P3)**
14. As a reviewer, I want every note sentence to show where it came from, and clicking it should play that audio and highlight the transcript, so that I can check it in seconds.
15. As a reviewer, I want note sentences with **no supporting source** to be visibly flagged, so that I can spot possible fabrications before relying on the note.
16. As a reviewer, I want to mark each note sentence as checked, so that I (and my supervisor) can see verification is complete.
17. As a DPO or safeguarding lead, I want the exported note to record that it was machine-drafted and human-verified, and by whom, so that the audit trail stands up.

**Call capture (P4)**
18. As a user on a Teams or phone-via-PC call, I want to capture both my mic and the other party's audio without echo or duplicated text, so that remote meetings work like in-person ones.

**IT / compliance**
19. As firm IT, I want the LLM delivered as a signed model pack through the same channel as speech models, so that nothing new needs internet access.
20. As a compliance officer, I want the assurance pack updated to cover recordings at rest and the LLM component, so that our DPIA stays accurate.

## Implementation Decisions

**Live capture (P1): reuse the engine, don't replace it**
- Feed the **existing Silero VAD + `OfflineRecognizer` (Parakeet TDT v2)** path from a live source
  instead of a decoded file. The loop in `engine/src/sherpa.rs` (`vad_chunks` + `transcribe`) already
  processes audio window by window. P1 separates the *source* (file vs. mic ring buffer) from the
  *chunk → recognise* step. Each VAD-closed utterance is decoded and emitted as an event, which fits the
  existing `transcribe-progress` event pattern in `app/src-tauri/src/commands.rs`.
- A **live VAD profile** with a shorter `max_speech_duration` (≈15–20 s, tuned in S1) bounds the lag. The
  offline profile (0.8 s silence / 120 s max) stays for file transcription.
- A true streaming model (Nemotron Speech Streaming via sherpa-onnx `OnlineRecognizer`) is **only a
  fallback** if S1 misses its lag target. Its NVIDIA Open Model License must pass the licence gate first.
- Mic input through a cross-platform audio crate (e.g. `cpal`), resampled with the existing `rubato`. It must
  pass `scripts/check-no-network.sh`.
- **Recording at rest:** audio is appended in fixed-length WAV chunks under the app-data directory (the
  BitLocker volume), then finalised on stop. The session's `audioPath` (`app/src/session.ts`) points
  at the finalised recording, so the Transcripts view and "Delete transcript and audio" work unchanged.
- **Diarisation runs on stop**, using the existing `diarize` path with the speaker-count hint. Live diarisation
  is out of scope (no CPU-viable option in sherpa-onnx; Sortformer is capped at 4 speakers).

**Local notes (P2)**
- **Runtime:** llama.cpp, in-process through a Rust binding, built statically from vendored sources. There is **no
  runtime download**, and the zero-egress CI gate must stay green (S5).
- **Weights:** a 3–4B instruction model, quantised (Q4), chosen by S2/S3/S4 and **licence-vetted**. Prefer
  Apache-2.0 or MIT; avoid licences with use restrictions that complicate redistribution. S2's finalists are
  **Qwen3-4B-Instruct-2507** and **Qwen3.5-4B** (both Apache-2.0, Q4_0); S3 and S4 choose between them.
- **Delivery:** as a **signed model pack**, reusing `modelpack/` and the fail-closed
  `resolve_verified_models_dir` in `app/src-tauri/src/models.rs`. The manifest gains `task: "llm"`.
- **Drafting during the recording (S2):** every ~10 minutes, the section just recorded goes to the LLM, which
  extracts its facts with citations under the template's headings. After stop, only the last section remains,
  so a 60- or 90-minute recording has its note in about 1–3 minutes on the S2 Mac. A single pass or map-reduce
  started at stop took 7–47 minutes. Imported recordings have no "during": they are processed section by
  section in the background, with progress shown.
- **The note is the section facts.** The first version of the note is those facts grouped by template heading.
  S2 found that one "compose" call over all the facts drops about half of them, so a shorter prose note
  needs a design that doesn't lose facts.
- **Memory:** while recording, ASR and LLM are both loaded. The LLM runs with an 8k context: about 4.4–4.7 GB
  peak, plus about 1.1–1.3 GB for live ASR (S1 and S2). That fits a 16 GB office laptop. The 8 GB target is
  tight and needs measuring on the reference laptop.
- **Map output cap** of about 3k tokens (2k truncated some sections in S2), or sections shorter than 10 minutes.
- **Templates:** local, versioned JSON (sections, per-section instructions, required fields, citation
  requirement). A small bundled seed set:
  - *General minutes*, *Executive summary*, *Care assessment*: adapted from i.AI Minute's templates
    (General, Executive Summary, Care Assessment V2) **only once i.AI confirms the licence**. Its
    `pyproject.toml` declares MIT but the repo has no LICENSE file. Until then, write our own from public
    Care Act eligibility criteria.
  - *Legal attendance note* and *Charity case note*: our own, co-designed with the first law-firm and
    charity users.
- Templates are **not** model packs. They are plain data that users may edit. Bundled templates are
  read-only and user copies are editable.

**Source-linked verification (P3)**
- The prompt contract requires the model to output **structured JSON**: each note sentence plus
  the **transcript sentence references** that support it. Grammar-constrained decoding (llama.cpp GBNF / JSON
  schema) enforces the shape.
- **Anchor citations to audio time ranges, not sentence ids.** Editor sentence ids (`sent-N` in
  `core/src/editor.ts`) are positional and change when sentences are merged or split. Time spans survive
  edits, because edits re-spread words within the original span. At render time a citation is resolved to whatever
  sentences currently overlap its span.
- **Unsupported-sentence flagging:** a note sentence with no valid citation, or whose cited span doesn't
  plausibly support it (a cheap lexical/entailment check, tuned in S3), is flagged. Flag categories
  borrow Minute's hallucination taxonomy (e.g. *fabrication*, *contradiction*), shown to the user
  in plain English.
- The UI reuses click-to-seek and highlighting from the review editor (`app/src/Editor.tsx`,
  `app/src/highlight.ts`). The note pane sits alongside the transcript.
- **Verbatim-first stays the rule:** the note is always derived from the (optionally corrected)
  transcript and never replaces it. Exports can include both.

**Call capture (P4)** is deferred. The open-source apps struggle here most (echo, duplicated text), so it
gets its own spike and phase. Lessons from their source and issue trackers
(`docs/research/live-capture-landscape-2026-11.md`):
- **Windows: endpoint loopback, not process loopback.** Anarlog shipped WASAPI process loopback (excluding
  its own process tree) and removed it, because new Teams produced all-zero buffers on that path (their PRs
  #7283 → #7590). It now loops back the output endpoint that another app is actually rendering to,
  re-checked every 2 s. macOS: a CoreAudio process tap.
- **Keep the microphone and call audio as separate channels.** Meetily mixes them before transcription and
  lost who said what, with ±60 ms drift (their issues #642, #851).
- **Pad loopback silence.** WASAPI loopback delivers no packets while nothing plays, so the call channel
  drifts against the mic unless gaps are filled (Vibe #1592).
- **Echo cancellation only when the output isn't headphones.** AEC removed the user's own voice with
  headphones in OpenWhispr (#1025); Anarlog skips AEC when every active output is headphones (#7273).
- **Watch the call channel for silence:** a loopback helper can report healthy while capturing zeros
  (OpenWhispr #1891).

**Assurance-pack changes (ship with each phase)**
- `docs/assurance/data-at-rest.md`: recordings are now created by Folio (previously only referenced), and the
  crash-recovery chunks and their deletion must be covered. The current claim that the engine writes no temp
  files must be reworded for the recording path.
- `docs/assurance/third-party-components.md`: the audio capture crate, llama.cpp and the LLM weights licence.
- `docs/assurance/data-flow.md`: mic → recorder → engine → LLM, all on-device.
- `docs/assurance/dpia-support.md`: recording consent, and the note as a machine draft requiring verification.
- MHRA (29 Jul 2026): tools that only transcribe, summarise or draft for professional review are not medical
  devices. Keep Folio's notes strictly in that category. **No diagnostic, eligibility-decision or
  automated-action features.**

**Actions (not engineering)**
- Ask i.AI to confirm the licence of `github.com/i-dot-ai/minute` (and add a LICENSE file).
- Legal review of the NVIDIA Open Model License before any Nemotron model ships.
- Recruit one law-firm and one charity/social-care user to co-design the seed templates and provide
  consented, representative test recordings.

## Testing Decisions

Same philosophy as the base PRD: test **external behaviour and contracts** with fixtures.

1. **Live pipeline equivalence:** replaying a fixture WAV through the live source at real-time speed
   produces a transcript whose text matches the file path's output within a small WER delta. It must not
   drop or duplicate words at chunk boundaries.
2. **Crash recovery:** killing the app mid-recording loses at most the last chunk, and on restart the
   partial recording is offered for recovery. "Delete transcript and audio" removes finalised and partial files.
3. **Template engine (pure, `core/`):** template JSON validation. Rendering the prompt from template +
   transcript is deterministic. Parsing model output into cited note sentences rejects malformed
   JSON and unknown citations.
4. **Citation resolution (pure, `core/`):** a time-span citation resolves to the correct sentences
   after merge, split and retype edits (fixtures built from `core/src/editor.ts` ops).
5. **Faithfulness regression set:** a fixed set of transcripts with reference notes and known
   traps (things *not* said), scored for unsupported sentences. Run per model-pack release as a
   **release gate**, in the same spirit as the WER gate for ASR packs.
6. **Sensitive-content set:** synthetic safeguarding transcripts must be summarised without refusal or
   softening (S4). This is also a per-release gate.
7. **Zero-egress gate:** `scripts/check-no-network.sh` stays green with the new crates.

## Out of Scope

- Any cloud, server or hybrid mode, including "optional cloud LLM" toggles.
- Live (streaming) diarisation.
- Clinical decision support, eligibility decisions, coding, or automated actions (these would change the regulatory status).
- Case-management system integrations (Mosaic, LiquidLogic, practice-management systems). Copy/export only for
  now, as the incumbents also do.
- Non-English capture and notes (the architecture should allow them later).
- Domain fine-tuning of the ASR model (Heidi-style). Revisit once consented domain audio exists.
- Mobile apps.

## Further Notes

- **Why this ordering:** P1 reuses the engine and is low risk. P2 is where the go/no-go uncertainty sits (CPU
  speed and quality of a local LLM on long transcripts), so its spikes run **first**, in parallel with P1
  build. P3 is the differentiator but depends on P2's output format. P4 is the hardest platform work and the
  least needed for in-person work.
- **Honest positioning:** a local 3–4B model will be slower (minutes, not seconds) and less fluent than
  Heidi or Beam's cloud models. The pitch is not "as good as the cloud". It is **"nothing leaves the
  device, and every sentence can be checked against the recording"**.
- **Open question:** whether the default output should be *note + transcript* or *transcript only, note
  on request*. Decide with the first users.
