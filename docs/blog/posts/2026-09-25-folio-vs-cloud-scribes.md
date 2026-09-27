---
date: 2026-09-25
categories:
  - Comparisons
slug: folio-vs-cloud-scribes
---

# AI scribes in UK public services: how Folio compares

AI "scribes" that listen to a conversation and write up the notes have moved very quickly from pilots to
procurement. NHS England Midlands has picked a preferred supplier for around 70,000 clinicians,
hundreds of councils use one in social care, and the government is building its own. We looked at
what is out there, what the independent evidence says, and where Folio fits. We also cover the
things Folio doesn't do yet.

<!-- more -->

!!! note "About this comparison"
    Every claim about another product links to a public source, and most figures are the
    vendor's own. The full research notes, including the claims we couldn't verify, are in the
    [research record](../../research/scribe-landscape-2026-09.md). Checked September 2026. Products
    change quickly. If we've got something wrong, please
    [open an issue](https://github.com/KindTechUK/folio/issues).

## The evidence so far: real time savings, real errors

Independent evaluations agree on two things.

**The time savings are real.**
- On Oxford University Hospitals ENT ward rounds, median documentation time fell from 5.0 to 2.1 minutes
  ([BMJ Open Quality](https://pmc.ncbi.nlm.nih.gov/articles/PMC13052744/)).
- In adult social care, an Alan Turing Institute evaluation found Care Act assessment conversations fell from
  about 90 to 35 minutes, and case-note writing from about 4 hours to 1.5 hours
  ([Community Care, Feb 2025](https://www.communitycare.co.uk/content/news/ai-tool-improves-direct-work-in-adult-social-care-despite-accuracy-concerns-practitioners-report)).
- An evaluation in Kent measured a 48% cut in write-up time
  ([Health Innovation KSS](https://healthinnovation-kss.com/ai-scribe-reduces-admin-time-by-almost-50-in-adult-social-care-assessments-independent-evaluation/)).

**The errors are real too.**
- **Hospitals:** Oxford University Hospitals' evaluation of four suppliers found 37% of logged uses
  needed accuracy edits and 6.2% contained hallucinations. The report notes that this figure may be inflated
  by users labelling ordinary errors as hallucinations, and that accuracy varied more with the clinical
  setting than with the supplier
  ([OUH / TheHill, Feb 2026](https://www.thehilloxford.org/wp-content/uploads/2026/02/Ambient-Voice-Technology-AVT-in-Acute-Services-An-Evaluation.pdf)).
- **Social care:** the Ada Lovelace Institute's
  [*Scribe and prejudice?*](https://www.adalovelaceinstitute.org/report/scribe-and-prejudice/) (Feb 2026)
  carried out 39 interviews across 17 local authorities, most of them with social workers. Interviewees
  described transcripts recording suicidal ideation that had never been mentioned, a child's account turned
  into "fish fingers or flies or trees", and a general-purpose AI tool's content filters blocking
  transcription of abuse disclosures.

The lesson isn't "don't use AI scribes". A human has to check the output, so the tool should make
checking fast.

## The main options

### Heidi (NHS clinical scribe)

Heidi is an ambient scribe for clinicians: templates for 200+ specialties, many languages, and
integrations with EHRs such as Epic and EMIS. NHS England Midlands selected it as preferred
provider in July 2026
([NHS England](https://www.england.nhs.uk/midlands/2026/07/15/midlands-leads-the-way-on-ambient-voice-technology/)).

- **Processing:** cloud. A GP practice's published DPIA for Heidi says UK data is processed within the UK, and
  audio from clinical encounters is processed in real time and not stored
  ([practice DPIA](https://bellhousedoctor.co.uk/wp-content/uploads/2025/09/Data-Privacy-Impact-Assessment-Heidi.pdf)).
- **Speech recognition:** Heidi says it fine-tuned **NVIDIA Parakeet TDT 0.6B v2** on clinical audio,
  cutting word error rate from 11.5% to 9.4%
  ([Heidi blog](https://www.heidihealth.com/en-us/blog/how-heidi-improved-asr-nvidia-nemotron)).
- **Strengths:** mature clinical templates, EHR integrations, a heavy NHS assurance portfolio.

### Beam Notes (formerly Magic Notes): councils and social care

Beam Notes, from London social enterprise Beam, is widely used in adult and children's social care. It has
templates for Care Act and other assessments, speaker separation, and a Windows app in the Microsoft Store.

- **Processing:** cloud. Newcastle City Council's algorithmic transparency record lists **Deepgram** for
  speech-to-text and an **OpenAI** model for summaries. Sub-processors include AWS, Azure, OpenAI, Google
  Cloud and ElevenLabs
  ([ATRS record](https://www.gov.uk/algorithmic-transparency-records/newcastle-city-council-magic-notes)).
- **Price:** an organisation-wide licence starting at **£30,000 a year** on G-Cloud
  ([pricing document](https://assets.applytosupply.digitalmarketplace.service.gov.uk/g-cloud-14/documents/717297/985901463553519-pricing-document-2026-05-13-0819.pdf)).
- **Strengths:** built around social-care workflows, large council footprint, full pilot support.

### i.AI Minute → Local Transcribe and Justice Transcribe (government-built)

The government's AI incubator built **Minute** for civil servants and piloted it with councils. It is
now being developed into **Local Transcribe** for councils, with a public beta planned for English
councils from spring 2027
([MHCLG Digital](https://mhclgdigital.blog.gov.uk/2026/07/23/building-local-transcribe-for-local-government/)).
The Ministry of Justice's **Justice Transcribe**, built from Minute, is used by over 12,000 probation officers
([MoJ Justice AI Unit](https://ai.justice.gov.uk/our-work/justice-transcribe)).

- **Processing:** cloud. The public code uses Azure Speech or AWS Transcribe, and Azure OpenAI or Google
  Gemini ([github.com/i-dot-ai/minute](https://github.com/i-dot-ai/minute)).
- **Access:** public-sector bodies.
- **Strengths:** free to participating public bodies, code published openly, thoughtful templates
  (including a Care Act–based assessment template that requires citations).

### Open-source meeting notetakers

There is a lively open-source scene of "Granola-style" desktop notetakers:
[Meetily](https://github.com/Zackriya-Solutions/meetily),
[anarlog](https://github.com/fastrepl/anarlog) (formerly Hyprnote) and
[OpenWhispr](https://github.com/OpenWhispr/openwhispr), among others. Several can transcribe locally and
summarise with a local LLM. They are mostly aimed at individual knowledge workers on meeting calls. On-device
support varies by platform: anarlog's local transcription, for example, currently requires Apple
Silicon.

## Side by side

| | Heidi | Beam Notes | Minute / Local Transcribe | Folio |
|---|---|---|---|---|
| **Built for** | Clinicians | Social care, councils | Public sector | Legal, charities, anyone with sensitive audio |
| **Where audio is processed** | Vendor cloud | Vendor cloud + AI sub-processors | Government cloud + Azure/AWS/Google | **On your device** |
| **Works with no internet** | Records, uploads later | Records, uploads later | No | **Yes, fully** |
| **AI-written notes from templates** | Yes | Yes | Yes | **Not yet** (planned) |
| **Record in the app** | Yes | Yes | Yes | **Not yet** (planned); you import a recording today |
| **Transcript review with word-synced audio playback** | Not a stated focus | Not a stated focus | Not a stated focus | **Yes** |
| **Price** | Free tier; paid plans | From £30k/yr per organisation | Free to participating public bodies | Free, open source (Apache-2.0) |
| **Source code** | Closed | Closed | Published | Published |

"Not a stated focus" means the product's public material centres on the generated note rather than on
reviewing the transcript against the audio. It is not a claim that the feature is absent.

## Where Folio fits

Folio takes a narrower approach, deliberately:

- **Nothing leaves the device.** Transcription runs on an ordinary laptop CPU. The app contains no
  networking code, and CI checks that it stays that way. Because the vendor never receives your data, there
  is no AI sub-processor to add to your DPIA. See [Privacy & security](../../privacy-security.md).
- **Verbatim first, human-verified.** Folio produces a transcript you check against the audio: a
  word-synced cursor, click-to-seek, sentence editing and speaker renaming. Every export is marked as a machine
  draft.
- **A competitive speech model.** Folio's default engine is Parakeet TDT 0.6B v2, the same base model Heidi
  says it fine-tuned. In our own small test on the AMI meeting corpus, it was both more accurate and
  much faster on a CPU than Whisper medium.en.
- **Affordable for small organisations.** It is open source, with signed installers and model packs.

**What Folio doesn't do today:** it does not write notes from templates, and it does not transcribe
live while you record. You record, then import the file. For many users the note is the product, so
this is a real gap, and it is where we are heading next.

## What's next

We are planning three additions, all still fully on-device:

1. **Live capture:** press record, watch the transcript build up, and get speaker labels when you stop.
2. **Local notes from templates:** attendance notes, case notes and assessment-style templates,
   written by a small AI model that runs on your laptop.
3. **Source-linked notes:** every sentence in a draft note links back to the moment in the recording it came
   from, and sentences with no support are flagged. Minute already asks its model for citations
   to the transcript, which is good prior art. We want to take that one step further, to one click that plays the audio.
   This is our answer to the hallucination problem the evaluations describe.

Running a language model on an office laptop CPU is a real constraint, so each step is gated on
measured experiments before we build it. The plan is public:
[PRD](../../prd/folio-next-live-capture-and-notes.md) and
[spike plan](../../spikes/next-phase-spikes.md). If you work in a charity, advice service or law firm and
want to help shape the templates, we'd like to hear from you.
