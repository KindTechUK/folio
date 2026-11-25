# Spike S3: citations and faithfulness

> Status: **NO-GO as specified, with a clear way forward** · Run 2026-11-24 on a Mac (a proxy for the reference
> laptop).
> Plan: [`next-phase-spikes.md`](next-phase-spikes.md#s3-citations-and-faithfulness).
> Harness: `spikes/s3-citations/` (setup and commands are in its README). Builds on S2's harness.

## Verdict

The two S2 finalists cite real transcript lines almost perfectly, but the cited lines alone fully support only
about 7 sentences in 10. About 1 sentence in 7 is not supported by the transcript, and 1 in 10 contradicts it.
No cheap automatic flag catches those sentences without flagging nearly every sentence.

| Pass criterion | Result | |
|---|---|---|
| ≥ 95% of sentences carry a valid citation (the plan: the span exists **and** supports the sentence) | **Exists:** 100% of sentences cite real lines (1 bad cite in 1,415). **Supports:** the judges found the cited lines alone fully support the sentence for 67% (test set) and 72% (tuning set); fully or partly for 94% and 97% | **Fail** (the lines exist; they don't support enough sentences) |
| Zero fabrications or contradictions go unflagged on the trap set | Only by flagging every sentence. At any useful threshold, 5–12 contradictions on the test set go unflagged | **Fail** |
| Flag precision ≥ 70% | 0.22 at best (lexical overlap or NLI) | **Fail** |

**What the errors are.** The models rarely invent things: 2 of 1,099 sentences were outright fabrications.
The 156 problem sentences are mostly contradictions (114) and over-inference (40). Counting keywords in the
judges' reasons:
- **Dates and years**, about a third (53). The recording date is never given, and the current year comes up
  only in passing ("fourteenth of August twenty twenty-six" on a dismissal letter; a deadline in "December
  twenty twenty-six"). Both models fill the year in from training instead (2023 or 2024 for events in 2026).
  One such error, dating the 2026 events to 2024, accounts for 17 of the 28 contradictions in Qwen3.5-4B's
  legal-interview note. They also move deadlines into the wrong month.
- **Wrong figures (31):** "like twenty-seven [thousand]" became "£27 per hour" in both models.
- **Wrong speaker:** the solicitor's advice became the client's objective; "Hannah reported that Tom…" when
  Tom said it himself.
- **Hedges dropped:** "I don't know" about legal expenses insurance became "does not have it".

**Traps.** Only 4 of 232 trap opportunities were hit (12 notes, each against its transcript's 16 or 21 traps). Negation,
hearsay and hypotheticals were mostly kept. The errors fall outside the traps we wrote, in ordinary details.
(Judges could record one trap per sentence, so a sentence hitting two would count once; the format now takes a
list.)

**Recommendation for P2/P3:**
1. **Give the model the recording's date** (and the participants' names and roles) in the prompt. That
   removes the largest single class of contradictions at no cost.
2. **Don't rely on an automatic "unsupported sentence" flag.** Make every note sentence one click from its
   cited lines, shown with the line before them (usually the question being answered), so the reviewer can
   check it. "Partly" support was mostly a cited answer without its question.
3. **If an automatic check is wanted, test a second LLM pass** that checks each sentence against its cited
   lines and the lines around them. Lexical overlap and a small NLI model can't separate a faithful
   paraphrase of conversational speech from a contradiction.
4. **Neither finalist is clearly better here.** Qwen3.5-4B made more contradictions on the two long transcripts
   (38 against 20). Qwen3-4B made more on the test set's tribunal interview (17 against 6) and the only 2
   fabrications. S4 decides between them.

**How much to trust the labels.** Blind Claude judges labelled every sentence, as in S2. You spot-checked a few
cards (3 S3 sentences, all agreed) and judged the labels broadly sound. That is a light check, not a measured
agreement rate.

## Environment

- **Machine:** Apple M5 Pro (18 cores, 48 GB), macOS 27.2. Not the reference laptop. Quality only: timings were
  not measured, and the machine was heavily loaded by the parallel S4 and S5 spikes.
- **Runtime:** llama.cpp `llama-server` 0.9.0, CPU only (`-ngl 0 -dev none`), 8 threads, 8k context, temperature
  0.2, seed 42, JSON-schema-constrained output.
- **Models:** the S2 finalists, Q4_0: Qwen3-4B-Instruct-2507 and Qwen3.5-4B (hashes in the S2 write-up).
- **Strategy:** S2's sectioned notes (map calls over ~10-minute parts, facts grouped by template heading), map
  output cap 3,072 tokens.
- **Citations:** lines are shown to the model as `L52 [12:34] S2: text`. The model cites line numbers, which it
  copies reliably. The harness maps each cite to the line's `{start, end}`, giving the planned
  `{section, sentence, cites: [{start, end}]}` shape (`results/notes/s3/*/*.spans.json`). No model ever cited a
  time instead of a line.
- **Transcripts** (all fictional):
  - **Tuning set:** S2's two, the 61-minute care assessment and the 46-minute legal interview (32 traps).
  - **Test set:** four new trap-dense transcripts of 12–14 minutes, each with at least 3 traps of every kind
    (negation, hypothetical, hearsay, corrected, not said, speaker attribution) plus 3–4 omission facts:
    - a mental-health review (assessment template);
    - a housing assessment (assessment);
    - an employment-tribunal client interview (attendance note);
    - a 4-speaker child-in-need planning meeting (minutes).
- **Judging:** 16 blind packets (model hidden behind a hash), 8 Claude judges reading every transcript in full.
  Each note sentence was labelled supported / unsupported fabrication / contradiction / over-inference (Minute's
  taxonomy), plus whether its cited lines support it (yes / partly / no), plus any trap hit.
- **Flags tested:** (a) lexical overlap of the sentence with its cited lines; (b) the NLI cross-encoder
  `cross-encoder/nli-deberta-v3-small` (premise: the cited lines; hypothesis: the sentence), used for evaluation
  only; (c) both combined. Thresholds were chosen on the tuning set and applied to the test set.

## Commands

```bash
cd spikes/s3-citations
python stamp_data.py --check
python run.py --run s3 --models qwen3-4b,qwen3.5-4b --transcripts all
python score.py auto --run s3
python score.py packets --run s3
HF_HUB_DISABLE_XET=1 <venv>/bin/python flag.py nli --run s3
# blind judges write results/judge/s3/<packet>.verdict.json, then:
python score.py collect --run s3
<venv>/bin/python flag.py eval --run s3
python report.py --run s3
```

Raw data is in `spikes/s3-citations/results/`:
- the run: `s3.jsonl` (every map call's raw output), `s3.scores.jsonl`, `s3.verdicts.jsonl` and `s3.nli.jsonl`;
- the notes: `notes/s3/`;
- the judging: packets, verdicts and the model key under `judge/s3/`.

## Results

### Labels per model

| Model | Set | Sentences | Supported | Contradiction | Over-inference | Fabrication | Not supported | Cites: yes / yes or partly |
|---|---|---:|---:|---:|---:|---:|---:|---|
| Qwen3-4B | tuning | 342 | 308 | 20 | 14 | 0 | 9.9% | 72% / 95% |
| Qwen3-4B | test | 233 | 191 | 25 | 15 | 2 | 18.0% | 67% / 94% |
| Qwen3.5-4B | tuning | 306 | 262 | 38 | 6 | 0 | 14.4% | 71% / 99% |
| Qwen3.5-4B | test | 218 | 182 | 31 | 5 | 0 | 16.5% | 67% / 95% |

### Per note

| Set | Model | Transcript | Sentences | Contradiction | Over-inference | Fabrication | Fact recall | Traps hit |
|---|---|---|---:|---:|---:|---:|---:|---|
| tuning | Qwen3-4B | care assessment | 192 | 7 | 8 | 0 | 0.67 | T11 |
| tuning | Qwen3.5-4B | care assessment | 167 | 10 | 5 | 0 | 0.80 | – |
| tuning | Qwen3-4B | legal interview | 150 | 13 | 6 | 0 | 0.71 | – |
| tuning | Qwen3.5-4B | legal interview | 139 | 28 | 1 | 0 | 0.55 | – |
| test | Qwen3-4B | housing | 60 | 2 | 4 | 0 | 0.68 | – |
| test | Qwen3.5-4B | housing | 58 | 4 | 0 | 0 | 0.53 | T13 |
| test | Qwen3-4B | mental-health review | 53 | 4 | 4 | 0 | 0.53 | T10 |
| test | Qwen3.5-4B | mental-health review | 47 | 14 | 2 | 0 | 0.53 | – |
| test | Qwen3-4B | planning meeting | 60 | 2 | 3 | 1 | 0.77 | – |
| test | Qwen3.5-4B | planning meeting | 60 | 7 | 3 | 0 | 0.77 | T09 |
| test | Qwen3-4B | tribunal interview | 60 | 17 | 4 | 1 | 0.41 | – |
| test | Qwen3.5-4B | tribunal interview | 53 | 6 | 0 | 0 | 0.47 | – |

Fact recall counts present = 1 and partial = 0.5. A fact stated with the wrong year counts as absent, which
drives the tribunal and legal-interview recall down. The mental-health review notes left out the whole risk
questioning (current suicide risk, the past overdose).

### Flags (positives = contradiction, over-inference, fabrication)

| Flag | Threshold (chosen on tuning) | Test precision | Test recall | Unflagged fabrications/contradictions (test) |
|---|---|---:|---:|---:|
| Lexical, best F1 | 0.50 | 0.21 | 0.74 | 12 |
| Lexical, catch-all | 0.75 | 0.19 | 0.96 | 2 |
| NLI, best F1 | 0.70 | 0.22 | 0.85 | 5 |
| NLI, catch-all | 1.00 (every sentence) | 0.17 | 1.00 | 0 |
| Combined, best F1 | (0.01, 0.70) | 0.22 | 0.85 | 5 |

The NLI model almost never finds entailment between conversational lines and a written sentence. The median
P(entailment) is 0.012–0.016, and 56–59% of sentences score below 0.1. So it flags faithful sentences as
readily as wrong ones.

## Confounds

- **The schema's 30-facts-per-call limit.** 29 of 40 map calls hit it (18 of 20 for Qwen3-4B). So 10-minute
  parts are probably cut short, which costs recall, and short parts get padded with trivia. This limit is S2's,
  and it also capped S4.
- **The test-set timestamps are synthetic** (seeded from word counts). They don't affect the labels.
- **A note item is scored as one sentence.** The schema doesn't force one sentence per item. 2 of the 1,099
  items hold two sentences (both short and both judged as a unit); quoted speech accounts for the other items
  with inner full stops. Too few to change any rate.

## Changes to the PRD

- **The note prompt includes the recording date, the participants and their roles.**
- **Every note sentence links to its cited transcript lines, shown with the line before them.** Faithfulness is
  checked by the reviewer, with the evidence one click away. There is no automatic "unsupported" warning in P3
  unless a second-pass LLM check passes its own spike.
- **Raise the map call's 30-fact limit, or use shorter parts,** before measuring recall again.

## Still to do

1. Re-run with the recording date in the prompt and the fact limit raised, and measure the change in
   contradictions.
2. Prototype a second-pass LLM check (sentence plus cited lines plus neighbours → supported / not), and measure
   its precision and recall against these labels.
3. A human spot-check at a larger scale, if the verdict is to be relied on beyond P2 planning.
