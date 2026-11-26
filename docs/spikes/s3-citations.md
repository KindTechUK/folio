# Spike S3: citations and faithfulness

> Status: **NO-GO as specified, with a clear way forward** · Run 2026-11-24; follow-ups on 2026-11-26
> ([prompt v2](#follow-up-1-prompt-v2-2026-11-26): the date and a higher fact limit cut errors by a third and lift
> recall; [second-pass check](#follow-up-2-a-second-pass-check-2026-11-26): fails, precision at most 0.68;
> [fast checks](#follow-up-3-fast-checks-2026-11-26): a speaker rule is precise but rare, MiniCheck doesn't help), on a Mac (a proxy for the reference
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
   removes the largest single class of contradictions at no cost. *Confirmed by the v2 follow-up, together with
   a 60-fact limit: errors down by a third, recall 0.61 → 0.84.*
2. **Don't rely on an automatic "unsupported sentence" flag.** Make every note sentence one click from its
   cited lines, shown with the line before them (usually the question being answered), so the reviewer can
   check it. "Partly" support was mostly a cited answer without its question.
3. **If an automatic check is wanted, test a second LLM pass** that checks each sentence against its cited
   lines and the lines around them. Lexical overlap and a small NLI model can't separate a faithful
   paraphrase of conversational speech from a contradiction. *Tested in follow-up 2 with the note model
   itself: precision 0.49–0.68 and recall 0.35–0.54, so it fails the 70% bar. It would need a larger model
   or reasoning before the verdict.*
4. **Neither finalist is clearly better here.** Qwen3.5-4B made more contradictions on the two long transcripts
   (38 against 20). Qwen3-4B made more on the test set's tribunal interview (17 against 6) and the only 2
   fabrications. S4 decides between them.

**How much to trust the labels.** Blind Claude judges labelled every sentence, as in S2. You spot-checked a few
cards (3 S3 sentences, all agreed) and judged the labels broadly sound. That is a light check, not a measured
agreement rate.

## Follow-up 1: prompt v2 (2026-11-26)

Recommendation 1 was tried with Qwen3.5-4B (the model S4 picked), prompt **v2**:
- **The recording date** in each map prompt: "Recorded on Wednesday 23 September 2026." Each transcript got a
  date chosen to contradict nothing it says. The care assessment's weekdays ("the third of March. A Monday",
  "Tuesday the fifteenth of April") only fit 2025, so it is dated 2 April 2025. The participants and their roles
  were already in the prompt.
- **The fact limit** raised from 30 to 60 per call, and the output cap from 3,072 to 4,096 tokens.
- **S4's statutory framing and rule (7)**, so that one prompt serves both spikes.

The prompt was fixed before any result. Six new blind judges labelled the v2 notes **and the first run's
Qwen3.5-4B notes again**, mixed in the same batches; each judge had both notes of the same transcripts. Packets
showed the recording date.

| Qwen3.5-4B | Set | Sentences | Contradiction | Over-inference | Fabrication | Not supported | Cites: yes / yes or partly |
|---|---|---:|---:|---:|---:|---:|---|
| first run, judged again | tuning | 306 | 36 | 9 | 1 | 15.0% | 69% / 97% |
| first run, judged again | test | 218 | 30 | 5 | 0 | 16.1% | 67% / 95% |
| **v2** | tuning | 378 | 26 | 7 | 0 | **8.7%** | 75% / 95% |
| **v2** | test | 252 | 29 | 4 | 0 | **13.1%** | 72% / 95% |

| Transcript | First run (judged again): sentences / contradictions / fact recall | v2: sentences / contradictions / fact recall |
|---|---|---|
| care assessment | 167 / 8 / 0.83 | 223 / 12 / 0.90 |
| legal interview | 139 / 28 / 0.52 | 155 / 14 / 0.81 |
| housing | 58 / 4 / 0.55 | 59 / 8 / 0.74 |
| mental-health review | 47 / 13 / 0.58 | 64 / 10 / 0.72 |
| planning meeting | 60 / 7 / 0.68 | 62 / 7 / 0.91 |
| tribunal interview | 53 / 6 / 0.47 | 67 / 4 / 0.97 |

**What changed:**
- **Not supported:** down by about a third, from 15.5% to 10.5% over both sets.
- **Date errors:** judge reasons that mention a date, year, month or weekday fell from 25 to 7. The legal
  interview's contradictions halved (28 to 14), as its notes no longer date the 2026 events 2024.
- **Fact recall:** up on every transcript; the mean rose from 0.61 to 0.84. No map call reached 60 facts, and
  every call stopped on its own.
- **Notes are longer:** 630 sentences against 524.

**What didn't change:**
- **Citation support** is about the same.
- **About 1 sentence in 10** is still not supported. The rest are ordinary errors:
  - the wrong speaker ("Hannah reported" for what her partner said himself);
  - a pronoun resolved to the wrong person;
  - a garbled detail, such as "skips her medication" where she said she skips her tea so the children can eat.
- **The housing note got worse** (4 to 8 contradictions).

So recommendation 2 stands: the reviewer checks each sentence against its cited lines.

**The judges agree with themselves.** On the 524 first-run sentences, the re-judging gave the same label as the
first round for 98.3% of them, and the same supported-or-not call for 98.7%. They are the same kind of judge,
so this measures consistency, not accuracy.

Commands (from `spikes/s3-citations/`):

```bash
nice -n 10 python run.py --run v2 --models qwen3.5-4b --prompt v2 --fact-cap 60 --map-max-tokens 4096 \
    --threads 6 --transcripts all
python score.py packets --run v2 --calibrate s3:qwen3.5-4b
python score.py auto --run v2
# 6 blind judges write results/judge/v2/<packet>.verdict.json, then:
python score.py collect --run v2
```

## Follow-up 2: a second-pass check (2026-11-26)

Recommendation 3 was tried: the note model (Qwen3.5-4B) reads each sentence again with the lines it cites and
answers with one word: `supported`, `partly`, `not_supported` or `contradicted`. A grammar forces the word, so the
call returns no reasoning. The input also includes the recording date and the speakers. The score is
P(`supported`), read from the first token's log-probabilities. The check ran on the first run's 1,099 judged
sentences (both models), so the judges' labels are the answer key. A threshold was chosen on the tuning set and
then applied to the test set.

Two context variants, fixed before any result:
- **`prev`:** each cited line plus the line before it, which is what recommendation 2 shows the reviewer.
- **`window`:** each cited line plus three lines either side. This one ran only because `prev` missed the
  target.

| Flag (test set: 451 sentences, 78 positive) | Precision | Recall | Flagged | Contradictions or fabrications unflagged |
|---|---:|---:|---:|---:|
| `prev`, verdict is not_supported or contradicted | 0.54 | 0.42 | 61 | 27 |
| `prev`, P(supported) < 0.55 (best F1 on tuning) | 0.49 | 0.54 | 85 | 21 |
| `window`, verdict is not_supported or contradicted | 0.68 | 0.35 | 40 | 33 |
| `window`, P(supported) < 0.45 (best F1 on tuning) | 0.58 | 0.41 | 55 | 30 |
| either, flag every sentence (the only zero-miss threshold) | 0.17 | 1.00 | 451 | 0 |

**Verdict: fail.** Neither variant reaches 70% precision. `window`'s verdict rule comes closest at 0.68 on the
test set, but it scores only 0.51 on the tuning set, and it catches only about a third of the bad sentences.
It is still much better than the S3 flags (best precision 0.22). It is not good enough to show as a warning.

**What the checker gets wrong:**
- **It passes most contradictions.** With `window`, it calls 64 of the 114 contradictions `supported`, and 33
  of the 40 over-inferences. These are the same subtle errors as before: the wrong speaker, a pronoun resolved
  to the wrong person, a figure or date that changed. A 4B model reading its own kind of paraphrase sees no
  difference.
- **It rarely says `contradicted`** (3 of 114 contradictions, in either variant). When it does flag a
  contradiction, it calls it `not_supported`.
- **It never really uses `partly`.**
- **It is wrong in the other direction too.** It calls 36 to 90 supported sentences `not_supported`, more often
  with less context.

**Cost.**
- **Per sentence:** a mean of 2.3 s for `prev` (on 6 threads, prompts of about 320 tokens) and 4.8 s for
  `window` (on 4 threads, about 500 tokens). Both ran on this Mac while it was busy, with a load average of
  about 19.
- **The 61-minute care assessment** produces about 220 sentences with v2, so `prev` adds about 8 minutes after
  the note is written. On the reference Windows laptop (2–4× slower) that would be 17–34 minutes, against S2's
  5-minute budget for the whole note.

**What this means:**
- Recommendation 2 stands. The reviewer checks each sentence against its cited lines; P3 shows no automatic
  "unsupported" warning.
- If a check is tried again, it needs a larger checker model than the note model, or reasoning before the
  verdict. Both cost more time, so it would run as an optional "check this note" action, not on every note.
- The bonus run of the checker on the v2 sentences was skipped. With both variants well below target, a second
  test set would not change the verdict, and the machine was busy.

Commands (from `spikes/s3-citations/`):

```bash
nice -n 10 python check.py run --run s3 --variant prev --threads 6     # about 45 min
nice -n 10 python check.py run --run s3 --variant window --threads 4   # about 90 min
python check.py eval --run s3 --variant prev     # -> results/s3.check-prev.md
python check.py eval --run s3 --variant window   # -> results/s3.check-window.md
```

## Follow-up 3: fast checks (2026-11-26)

Research on current practice suggested two cheaper checks: a fixed rule for the error types that remain, and a
small trained fact-checker. Both were scored against the labels the judges had already given, for the first
run's notes (`s3`, both models) and the v2 notes (`v2`, Qwen3.5-4B only). The evidence for each check is the
cited lines plus the line before each, as in follow-up 2's `prev`.

- **Speaker rule.** The sentence attributes something to a participant: a name, first name, surname or role
  ("the solicitor", "her partner") comes before a reporting verb ("said", "reported", "explained", "wants",
  ...), or follows "According to". It is flagged if that participant spoke none of the evidence lines.
- **Number rule.** A number in the sentence does not occur in the evidence lines. Digits, ordinals and spoken
  numbers count; "twenty twenty-six" also counts as 2026, and the recording's day and year are allowed.
- **MiniCheck-Flan-T5-Large.** A 0.8B checker (MIT licence) that matches GPT-4-class models on the public
  LLM-AggreFact benchmark, including its interview and meeting summaries. Its threshold was chosen on the
  tuning set.

The rules were written before any result. After the first scoring, 5 parser bugs were fixed from the tuning
set's false alarms: "4:00" read as 4 and 0, speaker codes read as numbers, "twenty eighteen" read as 38,
"nine and a half", and "second-hand". The test sets were not inspected.

| Flag | `s3` test (451, 78 positive) | `v2` test (252, 33 positive) |
|---|---|---|
| Speaker rule | P 0.78, R 0.09 (9 flagged) | P 1.00, R 0.15 (5 flagged) |
| Number rule | P 0.53, R 0.24 | P 0.15, R 0.06 |
| Speaker OR number | P 0.58, R 0.33 | P 0.39, R 0.21 |
| MiniCheck, best F1 on tuning | P 0.29, R 0.73 | P 0.17, R 0.73 |
| check.py `prev` score (follow-up 2) | P 0.49, R 0.54 | not run |
| mean of MiniCheck and `prev` | P 0.50, R 0.60 | not run |

**What this shows:**
- **The speaker rule is precise but rare.** Across both runs and splits it flagged 20 sentences, 13 of them
  contradictions. It needs no model, so it costs nothing. It misses most wrong-speaker errors, because it
  fires only when a sentence names a participant before a reporting verb; a pronoun resolved to the wrong
  person passes.
- **The number rule worked only while dates were wrong.** On the first run's notes it caught invented years
  and changed figures at about 0.55 precision. On the v2 notes, which have the recording date, few number
  errors remain. Most of what it flags is a correct number that was said outside the cited lines ("three
  falls in the last six months"), or one worked out from the date ("by Monday, 28 September").
- **MiniCheck doesn't help here.** It is fast (0.15–0.25 s per sentence on CPU in batches of 8), but its
  precision is no better than the small NLI model's in the original run. Combined with the `prev` score, it
  adds almost nothing.

So no check reaches 70% precision with useful recall, and recommendation 2 stands. The speaker rule is the one
part worth keeping: as a hint, it could highlight the few sentences it flags for the reviewer at no cost. Its
recall would rise if the note named people instead of using pronouns, which is a prompt change for P3.

Commands (from `spikes/s3-citations/`; MiniCheck needs `transformers torch sentencepiece` and downloads about
3 GB once):

```bash
python fastcheck.py minicheck --run s3 --threads 4   # -> results/s3.minicheck.jsonl
python fastcheck.py minicheck --run v2 --threads 4
python fastcheck.py eval --run s3                    # -> results/s3.fastcheck.md (rules need no model)
python fastcheck.py eval --run v2
```

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

- **The note prompt includes the recording date, the participants and their roles** (v2 follow-up: confirmed).
- **Every note sentence links to its cited transcript lines, shown with the line before them.** Faithfulness is
  checked by the reviewer, with the evidence one click away. There is no automatic "unsupported" warning in P3.
  A second-pass check with the note model was tried and failed (follow-up 2).
- **The map call allows 60 facts and 4,096 output tokens** (v2 follow-up: recall 0.61 → 0.84, no call reached 60).

## Still to do

1. ~~Re-run with the recording date in the prompt and the fact limit raised.~~ Done: follow-up 1.
2. ~~Prototype a second-pass LLM check and measure its precision and recall against these labels.~~ Done:
   follow-up 2 (fails with the note model) and follow-up 3 (rules and MiniCheck: no better). Still open, if
   wanted: a larger checker model, or reasoning
   before the verdict, as an optional action.
3. A human spot-check at a larger scale, if the verdict is to be relied on beyond P2 planning.
