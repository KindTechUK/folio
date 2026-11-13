# Spike plan: go/no-go experiments for live capture and local notes

> Status: planned · Recorded 2026-09-25.
> Gates the phases in `docs/prd/folio-next-live-capture-and-notes.md`.
> Evidence base: `docs/research/scribe-landscape-2026-09.md`.
> Format follows the original sherpa-onnx spike (removed from the tree in 59ba8a8; see git history): each spike ends with a written
> **GO / NO-GO / GO-WITH-CONDITIONS** verdict, exact commands and raw numbers.

## Ground rules

- **Measure on the target hardware.** The reference machine is a **Windows 11 office laptop, Intel Core i5/i7
  U-series (12th–13th gen) or equivalent, 16 GB RAM, no discrete GPU, on battery and on mains**. There is also a
  stretch run on an **8 GB** machine. Mac numbers are recorded only as a proxy. The earlier spike assumed
  Windows is 2–4× slower than Apple Silicon, and these spikes should replace that assumption with real figures.
- **Spike code is throwaway** and lives in `models/` or an `engine/examples/` binary, not the app.
- **Evaluation may use the network. The product may not.** Public datasets and an offline judge
  are fine for scoring. Nothing in the spike's shipped path may make network calls.
- **Use consented or synthetic audio and text only.** No client material.

## Summary

| # | Question | Pass signal | Time-box | Unblocks |
|---|---|---|---|---|
| **S1** | Can the existing VAD + Parakeet path keep up **live** on a Windows laptop? | p95 lag ≤ 5 s; CPU ≤ 60% avg; flat memory over 2 h | 2 days | P1 |
| **S2** | Can a local 3–4B LLM turn a **long transcript (15–90 min)** into a note fast enough on CPU? | 60-min transcript → note ≤ 5 min after stop; peak RAM ≤ 6 GB | 3 days | P2 |
| **S3** | Can a small model **cite its sources** reliably, and how often does it fabricate? | ≥ 95% of note sentences have valid citations; 0 fabrications in blind review | 3 days | P3 |
| **S4** | Does the local model **refuse or soften** safeguarding content? | 0 refusals, 0 softened facts on the sensitive set | 1 day | P2 (social care) |
| **S5** | Can we **package** llama.cpp + weights without breaking zero-egress or licensing? | Gate green; static build; licence-clean pick; signed pack verifies | 2 days | P2 |
| **S6** | Is **crash-safe recording** simple and does "Delete transcript and audio" still purge everything? | ≤ 1 chunk lost on `kill`; full purge | 1 day | P1, assurance |
| S7 | *(only if S1 fails)* Does **Nemotron Streaming** fix live lag on CPU? | Word lag ≤ 1 s at ≤ 60% CPU | 2 days | P1 fallback |
| S8 | *(deferred to P4)* Can we capture **call audio** without echo or duplicates? | No duplicated or echoed text with speakers on | 3 days | P4 |

**Order:** S2 and S1 first, because together they decide whether local-notes Folio is viable at all. Then S3,
S4, S5 and S6 (they can run in parallel). S7 runs only on an S1 failure. S8 runs when P4 is scheduled.

---

## S1: Live transcription on the existing engine

> **Result: go, with conditions** (Mac and Windows CI runner; reference laptop still to run): see [`s1-live-lag.md`](s1-live-lag.md). Harness: `engine/examples/live_replay.rs`.

**Question.** If we feed mic audio into the same Silero VAD → Parakeet TDT v2 `OfflineRecognizer`
path (`engine/src/sherpa.rs`), how far behind speech does the text appear, and does the laptop cope?

**Why it matters.** If yes, P1 needs no new model or crate, which keeps the licence and zero-egress
story unchanged. Parakeet has RTF 0.026 on Mac for the AMI eval, so even a 4× Windows penalty leaves headroom. But
live use adds per-utterance model calls, audio capture and UI load at the same time.

**Method.**
1. Add an example binary that plays a fixture WAV **at real-time speed** into the VAD, simulating a mic.
   Then run the same test with a real mic, using `cpal`.
2. Sweep the live VAD profile: `min_silence_duration` ∈ {0.3, 0.5, 0.8} s and `max_speech_duration` ∈ {10, 15, 20} s.
3. Log for each utterance: speech end → text emitted (**lag**), decode time, and CPU %. Log RSS every
   minute for a **2-hour soak**.
4. Compare the live transcript with the offline file transcript of the same audio (WER delta, words dropped or
   duplicated at chunk boundaries).

**Pass.** p95 lag ≤ 5 s. Average CPU ≤ 60% on the reference laptop. RSS flat (±5%) over 2 h. WER delta against the
offline path ≤ 1 point.
**If it fails.** Shorter chunks, fewer threads, then S7.

## S2: Local LLM on long transcripts, the headline question

> **Result: go, with conditions** (Mac, 6 models screened; reference laptop still to run): see [`s2-local-llm-notes.md`](s2-local-llm-notes.md). Harness: `spikes/s2-llm-notes/`.

**Question.** On the reference laptop, how long does a local 3–4B model take to produce a template note from
transcripts of **15, 45, 60 and 90 minutes**, and does quality hold at those lengths?

**Why it matters.** This decides whether offline Folio can offer notes at all, or only transcripts.
Speech runs at about 150 words per minute, so a 60-minute conversation is about 9k words, or **about 12k tokens**
(about 15k with citation markup). A 90-minute conversation is about 18–22k tokens. On a laptop CPU the **prompt
processing (prefill)** at these lengths, not text generation, is the likely bottleneck.

**Candidates** (licence to be re-checked at spike time): Qwen3-4B-Instruct (Apache-2.0), Phi-4-mini-instruct
(MIT), SmolLM3-3B (Apache-2.0), Gemma 3 4B (Gemma terms), Llama 3.2 3B (Llama Community License). All at Q4_K_M
via llama.cpp, with 4 and 8 threads.

**Method.**
1. Transcripts: the AMI meeting corpus (has human abstractive summaries; licence to be checked), ACI-Bench
   doctor–patient dialogues (reference notes; licence to be checked), and 2–3 synthetic social-care and
   legal role-plays that we write ourselves. Concatenate them to reach the 60- and 90-minute lengths.
2. Measure three strategies:
   - **(a) single pass:** the whole transcript in context.
   - **(b) map-reduce after stop:** summarise each ~10-minute section with citations, then compose the template.
   - **(c) incremental map *during* recording:** section summaries are produced while the conversation is
     still going, so only the reduce step remains when the user presses stop.
3. Record prefill tok/s, generation tok/s, **time from stop to note**, peak RAM (including KV cache at
   16k/32k context), and whether the model respects the template.
4. Quality: score against the reference notes, blind-rated by us plus an offline LLM judge, focusing on
   **omissions** (the failure every independent evaluation found).

**Pass.** For at least one model and strategy: a 60-minute transcript gives a note **≤ 5 min after stop**; a 90-minute
transcript gives one ≤ 8 min; peak RAM ≤ 6 GB with ASR unloaded; and no significant rise in omissions from
15 to 60 minutes.
**If it fails.** Consider strategy (c) alone, a smaller model for the map step, or "notes as an overnight/background job".
As a last resort, ship P1 without P2 and position Folio as transcript-first.

## S3: Citations and faithfulness

**Question.** Can a small model output note sentences **with transcript time-span citations** in a strict
JSON shape, and how often does it state things that were never said?

**Why it matters.** Source-linked notes are the differentiator (Heidi, Beam and Minute don't have them) and
the answer to the OUH hallucination findings (6.2% of uses) and Ada Lovelace's.

**Method.**
1. Give the model a transcript with lines numbered and time-stamped. Constrain the output with a JSON schema / GBNF
   grammar: `{section, sentence, cites: [{start, end}]}`.
2. Build a **trap set**: transcripts engineered with omissions, negations ("no thoughts of self-harm"),
   hypotheticals and third-party speech, which are where fabrications usually appear.
3. Score each note sentence using Minute's taxonomy (supported / unsupported-fabrication /
   contradiction / over-inference) by blind human review. Measure citation validity (does the span exist,
   and does it support the sentence?).
4. Prototype the cheap **unsupported-sentence flag** (lexical overlap and/or small NLI model) and measure its
   precision and recall against the human labels.

**Pass.** ≥ 95% of sentences carry a valid citation. Zero fabrications or contradictions go **unflagged** in the
trap set (fabrications may happen, but the flag must catch them). Flag precision ≥ 70%, so reviewers don't
tune it out.

## S4: Sensitive and safeguarding content

**Question.** Do the S2 candidates refuse, sanitise or soften descriptions of abuse, self-harm, violence,
substance use or sexual content in a transcript?

**Why it matters.** Ada Lovelace documented Copilot's content filters blocking social-work transcripts of
abuse disclosures. Local models have no vendor filter, but they do have safety tuning, which may refuse or
paraphrase away the facts a safeguarding record needs.

**Method.** Run 20–30 synthetic safeguarding transcripts, written with a practitioner, through each candidate
and template. Check for refusals, softened wording ("had a disagreement" for "hit") and dropped
disclosures. Try system-prompt framing ("you are recording a statutory record; reproduce disclosures
accurately").

**Pass.** Zero refusals and zero softened or dropped disclosures for the chosen model and prompt. **This is a
hard gate for social-care templates.**

## S5: Packaging, licensing and zero-egress

**Question.** Can llama.cpp be linked in-process, built from vendored sources, and shipped with its weights
as a signed model pack, without adding a network crate or a licence we can't redistribute?

**Method.**
1. Evaluate the Rust bindings (e.g. `llama-cpp-2`): static build on Windows/macOS/Linux CI, with **no
   build-time download** (same concern as `SHERPA_ONNX_LIB_DIR`). Run `scripts/check-no-network.sh`.
2. Package the chosen GGUF with `tools/modelpack-sign` (`task: "llm"`) and load it through
   `resolve_verified_models_dir` (`app/src-tauri/src/models.rs`). Tampering must fail closed.
3. Record installer and pack sizes (a Q4 4B model is about 2.5 GB), and check that the per-user install and MSI paths still work.
4. Licence review of the chosen weights: redistribution, attribution, and use restrictions. Record the result in
   `docs/assurance/third-party-components.md`.

**Pass.** Gate green. Reproducible static build. Signed pack verifies and loads. Weights licence cleared.

## S6: Crash-safe recording

**Question.** Does chunked recording to the app-data directory survive crashes, and does "Delete transcript and audio"
still remove everything?

**Method.** Record into rolling WAV chunks of 5–10 s, finalised and concatenated on stop. `kill -9` and pull the power
mid-recording, restart, and check recovery. Run "Delete transcript and audio" on finalised and partial recordings and inspect the
filesystem. Draft the wording changes to `docs/assurance/data-at-rest.md`.

**Pass.** At most the last chunk is lost. Recovery is offered on restart. No residual files after deletion.

## S7: Streaming ASR fallback (conditional)

Run only if S1 fails its lag target. Swap in **Nemotron Speech Streaming EN 0.6B (int8)** through the sherpa-onnx
`OnlineRecognizer`. Measure word-by-word lag, CPU and WER against Parakeet offline. **Before any code:** legal review of
the NVIDIA Open Model License for redistribution in our installer. Alternative: Moonshine v2 (MIT), which
would need a new runtime path.

## S8: Call capture (deferred to P4)

On Windows, WASAPI **endpoint loopback** of the device the call app is actually rendering to, kept as a
separate channel from the mic, with silence gaps padded; acoustic echo cancellation only when the output is
not headphones. On macOS, a CoreAudio process tap. (Process loopback in EXCLUDE mode was the first idea, but
Anarlog found new Teams gives all-zero buffers on it; see the PRD and
`docs/research/live-capture-landscape-2026-11.md`.) Test a Teams call on laptop speakers and on headphones:
the other party's words must appear once, and our own words must not appear twice. Prior art to study:
OpenWhispr's AEC helper and Anarlog's endpoint-following loopback and AEC.

---

## Deliverable of each spike

Each spike ends with a write-up in this folder (`docs/spikes/<name>.md`) containing:
- a verdict
- the environment
- exact commands
- raw numbers
- what changes in the PRD

This follows the original sherpa-onnx spike write-up.
