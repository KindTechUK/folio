# Research: the AI scribe / transcription landscape (September 2026)

> Status: research record · Compiled 2026-09-23 → 2026-09-25 from web research passes.
> Feeds: a planned public comparison post (held until spike S2 reports), the
> next-phase PRD (`docs/prd/folio-next-live-capture-and-notes.md`) and the spike plan
> (`docs/spikes/next-phase-spikes.md`).
> Key claims used in the blog were re-checked against their sources on 2026-09-25 and corrected where needed.
> Each section ends with a **Not verified** list. Most figures are the vendor's own and are
> labelled as such. Stars and versions are as of the date checked.

## TL;DR

- **Ambient scribes are now mainstream in UK public services.** Heidi is NHS England Midlands'
  preferred supplier, Beam Notes (formerly Magic Notes) is in hundreds of councils, and government is
  building its own (i.AI Minute → Local Transcribe / Justice Transcribe).
- **Every commercial and government option is cloud.** Each sends audio to US-headquartered AI
  sub-processors (Deepgram, OpenAI, Azure, Google, AWS). Their "offline" modes only record and upload later.
- **The evidence shows real time savings and real errors.** Independent evaluations report
  roughly 40–60% less documentation time. They also report hallucinations and omissions that matter
  for statutory records: Oxford University Hospitals' 4-vendor AVT evaluation, and the Ada Lovelace
  Institute's report on social care.
- **Heidi's English speech-to-text is Parakeet TDT 0.6B v2, fine-tuned.** That is the same base model Folio
  ships by default. The accuracy gap is domain fine-tuning, not architecture.
- **Source-linking notes back to the recording is rare.** We found no evidence that Heidi does it.
  Minute asks its model for inline citations to the *transcript* (its Care Assessment template requires them), but
  we found nothing that links a note sentence to the *moment in the audio* for one-click verification.
  Folio's word-synced player is a natural base for that.
- **Open-source "Granola clones" exist, but none targets CPU-only Windows with zero network.** The
  leading one with on-device transcription (anarlog) is Apple Silicon only.
- **Offline streaming speech-to-text on CPU is now viable** (Nemotron Speech Streaming, Moonshine v2), but
  Folio does not need it for v1. VAD-chunked Parakeet gives final-quality text a few seconds behind speech.

---

## 1. Heidi Health (NHS clinical scribe)

**Adoption and regulation**
- NHS England Midlands chose Heidi as preferred AVT provider for 15 acute/community trusts
  (~70,000 clinicians, 1,239 GP practices), 15 Jul 2026.
  <https://www.england.nhs.uk/midlands/2026/07/15/midlands-leads-the-way-on-ambient-voice-technology/>
- Heidi is on NHS England's self-certified AVT supplier registry (opened 16 Jan 2026, 23 suppliers).
  <https://digital.nhs.uk/services/ambient-scribing>
- On 29 Jul 2026 the MHRA clarified that AVTs which only transcribe, summarise or draft for clinician review are
  **not medical devices**. Diagnosis/treatment support and automated actions remain regulated.
  <https://www.gov.uk/government/news/mhra-clarifies-regulatory-status-of-ambient-voice-technologies-used-in-the-nhs>
- Heidi claims DTAC, DCB0129 (with named clinical safety officers), DSPT, ISO 27001/42001, SOC 2 Type II and
  Cyber Essentials. <https://www.heidihealth.com/en-gb/blog/uk-clinicians-using-heidi-satisfy-nhs-guidance>
- In UK sessions, Heidi Evidence (its clinical Q&A) is blocked because it would count as decision support.

**Architecture** (vendor sources)
- Speech-to-text: moved from an unnamed vendor API to **NVIDIA Parakeet TDT 0.6B v2 fine-tuned on ~1,500 h of
  clinical audio** (8×H100, 12 h). WER went 13% (old vendor) and 11.5% (base) → **9.4%**. Medical-term F1 is 93.5%.
  Latency went ~3.0 s → ~0.7 s and cost fell 64%. English only.
  <https://www.heidihealth.com/en-us/blog/how-heidi-improved-asr-nvidia-nemotron>
- Hosting: AWS (Bedrock plus self-hosted models on EKS). The note-generation LLM is not disclosed.
- Data handling, per a practice's copy of Heidi's DPIA (dated 21 Feb 2025):
  - Its **sub-processors'** primary data centres are in London, Dublin and Frankfurt, and UK processing is
    "exclusively within the UK".
  - **Clinical-encounter audio is processed in real time and not stored.** The apps hold audio temporarily until
    it is processed, and clinicians can opt in to saving dictation audio. Transcripts and notes are kept for up
    to 28 days.
  - A fine-tuned BERT model removes personal details before the LLM sees the text (from other Heidi sources).
  <https://bellhousedoctor.co.uk/wp-content/uploads/2025/09/Data-Privacy-Impact-Assessment-Heidi.pdf>
- The desktop app is Tauri (Rust + React), the same shell as Folio.

**Evidence**
- *Vendor:* Heidi-authored, Heidi-funded preprint (medRxiv, Aug 2026). 385 simulated consultations with actors.
  Heidi's notes scored PDQI-9 40.6 against 35.6 for junior/middle-grade doctors. Critical/high errors appeared in
  24.4% of Heidi notes against 61.0% of clinician notes.
  <https://www.medrxiv.org/content/10.64898/2026.08.18.26360701v1>
- *Vendor:* the evaluation framework is a gated release process: safety first, then quality. It uses a 4-level
  hallucination scale (benign restatement → useful inference → diagnostic over-reach → fabricated finding).
  No rates are published. <https://www.heidihealth.com/en-us/resources/evaluation-framework>
- *Independent:* Oxford University Hospitals / TheHill evaluation (Feb 2026) of Accurx Scribe, Heidi,
  Lyrebird and TORTUS.
  - 398 logged uses. **37.3% needed accuracy edits and 6.2% contained hallucinations** (24 uses). 44.4% of
    post-pilot survey participants saw at least one hallucination.
  - The report says the 6.2% "may be over representative", because users confused ordinary errors with
    hallucinations.
  - There was "very little variation in accuracy between solutions"; the use case mattered more.
  <https://www.thehilloxford.org/wp-content/uploads/2026/02/Ambient-Voice-Technology-AVT-in-Acute-Services-An-Evaluation.pdf>
- *Independent:* BMJ Open Quality, OUH ENT ward rounds. Median documentation time fell from 5.00 to 2.13 min;
  no hallucinations were found (small n). <https://pmc.ncbi.nlm.nih.gov/articles/PMC13052744/>
- *Independent:* *Eye* (Nature), ophthalmology simulation. Heidi had the highest PDQI-9 of 6 tools, but all
  tools omitted critical details. <https://pubmed.ncbi.nlm.nih.gov/41776297/>
- *Independent:* BGS abstract (Hull frailty team). Note-writing time fell from 29.7 to 18.3 min.
  <https://www.bgs.org.uk/evaluation-of-heidi-ai-scribing-technology-in-improving-efficiency-and-documentation-quality-in>

**No evidence found** that Heidi links note sentences to transcript spans. Its stance relies on
clinician review.

**Not verified:** UK (GBP) pricing (the USD figures come from a competitor's review); SystmOne
integration; whether the "Heidi Remote" clip-on device transcribes on-device; BGS study dates; Heidi's
subprocessor list.

---

## 2. Beam Notes, formerly Magic Notes (councils, social care, charities)

**Company:** Beam Up Ltd (beam.org), London. It calls itself a social enterprise but is venture-backed.
The product was renamed from Magic Notes to Beam Notes in 2026, and magicnotes.ai now redirects to Beam Notes. The
Microsoft Store listing (`9P3PWQCFGF83`, package `BeamUpLTD.MagicNotes`) is a thin desktop client that
needs a Beam Notes account and an internet connection. <https://apps.microsoft.com/detail/9p3pwqcfgf83>

**Adoption** (vendor figures): 350+ organisations and 100,000+ frontline workers. Named councils include Barnet,
Camden, Kent, Swindon, Somerset, Newcastle and Nottinghamshire. NHS: Hampshire & Isle of Wight ICB
(£87.5k, 1 year). <https://htn.co.uk/2026/04/24/hampshire-and-isle-of-wight-icb-procures-ambient-scribing-and-reporting-tool/>

**Architecture:** **all cloud.** Speech-to-text is Deepgram with Whisper as fallback, and summaries come from an OpenAI
model. Sub-processors: AWS, Azure, OpenAI, Kinde, GCP, Fastly and ElevenLabs (record published 16 Dec 2025).
<https://www.gov.uk/algorithmic-transparency-records/newcastle-city-council-magic-notes>
"Offline mode" records without a connection and uploads later. Retention is set per council (40–120 days
seen). Output reaches Mosaic and LiquidLogic by copy and paste; no API integration was found.

**Price:** an organisation-wide licence with unlimited users and prepaid hours. **From £30k/yr for 6,000 hours**,
up to £225k–£300k/yr. A 12-week pilot is £30k.
<https://assets.applytosupply.digitalmarketplace.service.gov.uk/g-cloud-14/documents/717297/985901463553519-pricing-document-2026-05-13-0819.pdf>

**Evidence**
- Alan Turing Institute evaluation (Swindon, Barnet, Camden; 91 staff), reported Feb 2025:
  - Care Act assessment **conversations** went from 90 to 35 min, and case-note writing from about 4 h to 1.5 h.
  - Staff also said: "Sometimes it can add in detail based on what's not been discussed."
  <https://www.communitycare.co.uk/content/news/ai-tool-improves-direct-work-in-adult-social-care-despite-accuracy-concerns-practitioners-report>
- Health Innovation Kent Surrey Sussex, Kent evaluation: write-up time fell 48%.
  <https://healthinnovation-kss.com/ai-scribe-reduces-admin-time-by-almost-50-in-adult-social-care-assessments-independent-evaluation/>
- **Ada Lovelace Institute, "Scribe and prejudice?"** (11 Feb 2026): 39 interviews (29 of them social workers)
  across 17 local authorities (14 English, 3 Scottish), naming Magic Notes and Microsoft Copilot.
  - Errors reported: suicidal ideation recorded that was never mentioned; "fish fingers or flies or trees"
    in a child's account; gibberish and foul language.
  - Copilot's content filters blocked transcription of abuse disclosures.
  - Evaluations mostly measure time saved, not accuracy.
  - The report attributes errors to "an AI transcription tool", not to a named product.
  <https://www.adalovelaceinstitute.org/report/scribe-and-prejudice/>
- Nesta public-attitudes study: 83% positive, 89% say human oversight is essential.
  <https://www.nesta.org.uk/report/ai-sral-magicnotes/>

**Not verified:** investors; desktop-app framework; full languages list; SOC 2 status (Beam's pages
are inconsistent).

---

## 3. i.AI Minute → Local Transcribe (MHCLG) and Justice Transcribe (MoJ)

**What it is:** a web app from i.AI (DSIT), launched January 2025 as part of the "Humphrey" suite.
You record or upload a meeting, it transcribes and labels speakers, then an LLM writes templated minutes.

**Rollout**
- 2025 council pilot: 25 councils were chosen from 52 applicants, with 25 free users each for about 6 weeks.
  Use cases included adult and children's social care, planning, HR and democratic services. Barriers were
  VPN problems, no single sign-on and uncertainty over long-term cost.
  <https://www.local.gov.uk/publications/community-led-innovation-local-government-insights-minute-pilot>
- **Local Transcribe (MHCLG):** housing and homelessness first, with partner councils Birmingham, Hart, Maidstone,
  Trafford and Haringey. Plan: ~1,000 users in 12+ councils by March 2027, then a **public beta for all
  English councils from spring 2027**.
  <https://mhclgdigital.blog.gov.uk/2026/07/23/building-local-transcribe-for-local-government/>
- **Justice Transcribe (MoJ):** a fork for probation.
  - Scaled to ~1,000 probation officers in October 2025, with 50% less note-taking time in the pilot.
    <https://ai.gov.uk/our-work/frontline-services/>
  - Now **over 12,000 probation officers**, with over 1.5 million meetings summarised between 7 Oct 2025 and 14 Sep 2026
    (page accurate as of 24 Sep 2026). <https://ai.justice.gov.uk/our-work/justice-transcribe>
- Access is public sector only. **No route for charities or private legal practices was found.**

**Code** (<https://github.com/i-dot-ai/minute>, checked 2026-09-25)
- The repo is public and active, labelled "incubation project".
- **Licence ambiguous:** `pyproject.toml` declares MIT, but there is no LICENSE file.
- Stack: Next.js (GOV.UK style), FastAPI, Postgres and Ray workers, with SQS/Azure Service Bus queues,
  S3/Azure Blob storage, and Terraform to AWS.
- Speech-to-text: **Azure Speech or AWS Transcribe** (no local model). LLM: **Azure OpenAI or Gemini** (Gemini is the
  default, with safety filters disabled).
- Templates: Cabinet, Planning Committee, **Care Assessment V2** (Care Act eligibility criteria, citations
  required), Delivery, Executive Summary, General, and user-defined templates.
- A hallucination taxonomy exists in code (e.g. `FACTUAL_FABRICATION`, `CONTRADICTION`), but the check is
  **off by default and not shown in the UI**.
- No case-management integrations. Teams integration and a full audit trail are open issues.

**Evidence:** only time-saved figures (about 1 hour saved per 60-minute meeting in central government; up to 50–90% at
some councils). No published accuracy or hallucination rates. No algorithmic transparency record (ATRS) entry
for Minute or Local Transcribe was found (Consult, Redbox and Parlex do have entries).

**Legal-sector context:** the CPS uses Beam Notes. HMCTS runs BenchNotes on Azure Speech within its own
boundary.

**Not verified:** the licence; "not available to new departments or local authorities" (search snippet only);
Local Transcribe's stack and pricing; a £0.50/meeting running-cost figure (no traceable source).

---

## 4. Open-source "Granola-style" notetakers

| Project | Licence / ★ | Stack | Live transcription | Capture (Win / Mac) | Diarisation | Local LLM |
|---|---|---|---|---|---|---|
| [Meetily](https://github.com/Zackriya-Solutions/meetily) | MIT, ~31k | Tauri + Rust; whisper.cpp + Parakeet | VAD-chunked; true streaming is an open issue (#604) | WASAPI loopback / CoreAudio tap | Not in free build (PRO: "coming soon") | Ollama |
| [anarlog](https://github.com/fastrepl/anarlog) (ex-Hyprnote) | MIT, ~9.4k | Tauri v2; ONNX AEC; pyannote ONNX | Streaming then batch re-pass; **on-device only on Apple Silicon** | WASAPI loopback / CoreAudio tap | Yes (batch) | Ollama, LM Studio |
| [OpenWhispr](https://github.com/OpenWhispr/openwhispr) | MIT, ~8.5k | Electron; whisper.cpp + sherpa-onnx; llama.cpp | True CPU streaming (Nemotron) | Process-loopback EXCLUDE / CoreAudio tap; WebRTC AEC | Yes (live) | llama.cpp |
| [Vibe](https://github.com/thewh1teagle/vibe) | MIT, ~7.6k | Tauri; whisper.cpp, Nemotron, Parakeet v3 | "Realtime preview"; mainly files | System audio (method not verified) | Yes | Ollama |
| [Scriberr](https://github.com/rishikanthc/Scriberr) | MIT, ~3.1k | Go + Python WhisperX; Docker web app | Upload only | — | Yes | Ollama |
| screenpipe, Natively | Source-available (not OSS) | — | — | — | — | — |

Common pain points: echo and duplicated text when the mic hears the speakers during calls; CPU load; crashes.
The projects that handle echo well ship acoustic echo cancellation plus per-process loopback that excludes their own output.

---

## 5. Offline streaming speech-to-text and diarisation on CPU

| Model | Licence | Latency | English WER | CPU / ONNX |
|---|---|---|---|---|
| [Nemotron Speech Streaming EN 0.6B](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b) | NVIDIA Open Model License (**check redistribution**) | 80 ms – 1.12 s chunks | Open ASR avg 6.93% (1.12 s) / 7.67% (160 ms) | int8 ONNX in sherpa-onnx; parakeet-rs |
| [Moonshine v2](https://github.com/moonshine-ai/moonshine) | MIT (English) | 50–258 ms (M3) | Open ASR avg: Small 7.84%, Medium 6.65% | ONNX Runtime; Windows not benchmarked |
| [Kyutai STT](https://github.com/kyutai-labs/delayed-streams-modeling) | CC-BY-4.0 | 0.5–2.5 s | 2.6B: Open ASR 6.4% | No ONNX; CPU real-time unlikely |
| [Voxtral Mini 4B Realtime](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602) | Apache-2.0 | 240 ms – 2.4 s | 8.72% (multilingual avg) | GPU (vLLM); not practical on CPU |
| sherpa-onnx streaming Zipformer | Apache-2.0 | Low | Below Nemotron; no punctuation | Very light, CPU real-time |
| Whisper streaming (SimulStreaming, WhisperLive) | MIT | ~1–3 s | Whisper-dependent | Authors say CPU is too slow with large models |

**Folio's current offline numbers for comparison** (from our own AMI eval, Mac CPU): Parakeet TDT v2 int8 has
WER 17.9% at RTF 0.026, against Whisper medium.en at 27.2% / 0.478. See the original sherpa-onnx spike (in git history before 59ba8a8) and PR #3.

**Streaming diarisation:** NVIDIA Streaming Sortformer v2.1 has **a maximum of 4 speakers** and a DER of 6.65% on CALLHOME
2-speaker, but it is not in sherpa-onnx (feature request #3497). diart is not recommended. sherpa-onnx and pyannote
are offline only, which is fine for a pass after recording stops.

**Two-pass pattern** (streaming draft, then offline re-pass): anarlog does this automatically (Mac only).
Meetily offers a manual "re-transcribe".

---

## 6. What this means for Folio

1. **The gap is real and specific.** No option combines on-device processing, zero sub-processors, affordability for
   small charities and law firms, and human-verifiable output.
2. **Folio's ASR is already at "base Heidi" level.** The differences are domain vocabulary and fine-tuning,
   not the engine.
3. **What customers actually buy is notes from templates** (Care Act assessments, attendance notes), not raw
   transcripts. Folio needs a local-LLM notes feature to be comparable.
4. **Faithfulness is the differentiator to aim for.** Sentence-level citations back to the audio, plus
   verbatim-first review, respond directly to the OUH and Ada Lovelace findings.
5. **Live capture is achievable with the existing engine.** The hard parts are the local LLM on CPU, call audio
   capture, and the new data-at-rest footprint of recording.

These are turned into requirements in `docs/prd/folio-next-live-capture-and-notes.md` and into
go/no-go experiments in `docs/spikes/next-phase-spikes.md`.
