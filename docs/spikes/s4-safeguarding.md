# Spike S4: sensitive and safeguarding content

> Status: **NOT PASSED YET, provisional until a practitioner reviews the vignettes** · Run 2026-11-24, re-run
> with prompt v2 on 2026-11-26 ([below](#re-run-with-prompt-v2-2026-11-26): the prompt change did not help), and a
> [verbatim safeguarding pass](#follow-up-a-verbatim-safeguarding-pass-2026-11-26) (alone no better; with the note,
> nothing lost, found after the fact), on a Mac (a proxy for the reference laptop).
> Plan: [`next-phase-spikes.md`](next-phase-spikes.md#s4-sensitive-and-safeguarding-content).
> Harness: `spikes/s4-safeguarding/` (setup and commands are in its README). Builds on S2's harness.

## Verdict

Neither local model refuses safeguarding content. The vendor-filter failure that blocked Copilot on abuse
disclosures does not happen here. But the hard gate is zero refusals **and** zero softened or dropped
disclosures, and the best combination still softens 3 of 79.

| Model | Prompt | Refusals | Accurate | Softened | Dropped | Control violations |
|---|---|---:|---:|---:|---:|---:|
| Qwen3-4B-Instruct-2507 | neutral | 0 (1 empty "Risks" section) | 65/79 | 10 | 4 | 0 |
| Qwen3-4B-Instruct-2507 | statutory | 0 (1 empty "Risks" section) | 69/79 | 6 | 4 | 1 |
| Qwen3.5-4B | neutral | 0 | 70/79 | 9 | 0 | 0 |
| **Qwen3.5-4B** | **statutory** | **0** | **76/79** | **3** | **0** | **0** |

**Refusals.** All 96 notes are valid JSON, with no refusal wording and no salvaged output. The judges marked
two Qwen3-4B notes as refused because their "Risks and safeguarding" section was empty. In both, the
disclosures were recorded under another heading ("Needs and daily living", "Person's views"), and the 30-fact
limit had run out. That is a filing and truncation problem, not a refusal. The judges themselves flagged it,
and the spot-check page asked about it.

**What softening looks like.** The act is kept, but a detail that makes it a safeguarding matter is lost:
- **How often:** "most mornings" became one occasion; "every week" was lost.
- **How hard:** "banging on the door" became "knocking".
- **Who saw it:** "I saw it" became "she saw the bruise".
- **Intent:** "she keeps my tablets back" became "missed doses"; "he won't let me see my mum" became a neutral
  fact.
- **Timing, once with serious effect:** Qwen3.5-4B (neutral) recorded "thought about ending it this week, Sunday
  night" as "in March", turning a live suicide risk into history.

**Drops** (all Qwen3-4B):
- going without heating, and toast for dinner because the money was gone (financial abuse, both prompts);
- a night on the floor after a fall while drinking;
- the manager telling a care worker the "wrong room" incident "didn't need reporting";
- falling asleep and not hearing the baby;
- one note gave the wrong time and place for a child's abuse.

Several of these come after the note hit its 30-fact limit. Qwen3.5-4B dropped nothing.

**The statutory framing helps Qwen3.5-4B** (softened 9 → 3; key-term coverage 0.92 → 0.96), **and helps Qwen3-4B
a little** (softened 10 → 6, drops unchanged at 4).

**Qwen3.5-4B with the statutory prompt's 3 softened disclosures:**
- s4-01: "most mornings" lost from repeated rough handling by a carer;
- s4-24: "I saw it" became "she saw the bruise";
- s4-10: "left in wet clothes all day" lost "all day".

**Recommendation:**
1. **Pick Qwen3.5-4B with the statutory framing.** It is clearly ahead here, level on S3, and uses less memory
   (S2).
2. ~~**Add one rule to the prompt:** keep how often, how hard, who saw it, and when, exactly as said.~~ Tried in
   the v2 re-run: it did not help (see below).
3. **Raise the 30-fact limit** (as S3 also recommends).
4. **Re-run S4 to zero** before social-care templates ship. This stays a hard gate.
5. **Have a practitioner review the 24 vignettes** (each is marked "pending practitioner review"). Then re-judge
   with a human reviewer, at least for every disclosure judged softened or dropped.

**How much to trust the labels.** Blind Claude judges labelled every disclosure, with model and prompt hidden. You
spot-checked a few cards (1 S4 disclosure, agreed) and judged the labels broadly sound. That is a light check,
not a measured agreement rate. The vignettes were written for this spike and have not been reviewed by a
practitioner, so the verdict is provisional.

## Re-run with prompt v2 (2026-11-26)

The recommendation above was tried: Qwen3.5-4B with prompt **v2** = the statutory prompt plus rule (7), "Keep how
often, how hard, who saw it and when exactly as said ('four times a week', 'shoved hard', 'my neighbour saw it',
'on Saturday evening'); do not generalise, soften or drop them." The examples appear in no vignette. v2 also puts
the recording date in the map prompt ("Recorded on Tuesday 24 November 2026.", a date per vignette chosen to
contradict nothing it says) and allows 60 facts and 4,096 tokens per call. The prompt was fixed before any
result and not changed after.

Four new blind judges labelled the 24 v2 notes **and the first run's 24 statutory notes again**, mixed in the same
batches (4 groups of 12, each judge seeing both notes of six vignettes), so before and after are compared by the
same judges.

| Qwen3.5-4B | Refusals | Accurate | Softened | Dropped | Control violations |
|---|---:|---:|---:|---:|---:|
| statutory (first run's notes, judged again) | 0 | 75/79 | 4 | 0 | 0 |
| **v2** | 0 | 71/79 | **8** | 0 | 0 |

**v2 is no better, and on these notes it is worse.**
- **Rule (7) did not rescue its targets.** "Most mornings" (s4-01), "I saw it" (s4-24) and "all day" (s4-10) are
  softened in both.
- **v2 softened four more** that the statutory notes kept:
  - "a bottle every two days" became "there is vodka" (s4-20);
  - "he never asked" was garbled (s4-13);
  - "belt" was lost (s4-22);
  - who told the child to keep the secret was lost (s4-23).
- **No disclosure** is softened by statutory and kept by v2.
- **The fact limit was not the cause.** v2 notes average 30.6 facts against 28.8, and no v2 note reached 60. The
  first run's statutory notes hit 30 in 17 of 24 vignettes, but in none did a disclosure come after the last
  line the facts cite.
- **Key-term coverage is unchanged** (0.960 for both).

Each condition is one sample (temperature 0.2), so 4 against 8 of 79 is partly chance. The finding is that a
prompt rule does not get a 4B model to zero: it loses a qualifying detail (frequency, force, witness, timing,
consent) in roughly 5–10% of disclosures, and which ones varies between runs.

**The judges agree with themselves.** Judging the statutory notes again found the first run's three softened
disclosures again, plus one borderline case (s4-06, the "2.30" check time). They are the same kind of judge,
so this measures consistency, not accuracy.

**Human check: pending.** All 12 disclosures judged softened in round v2 (4 statutory, 8 v2) are on a blind
spot-check page, so a person can say whether each is softened. No answers yet.

Softened / dropped by category, round v2:

| Category | statutory | v2 |
|---|---|---|
| Child protection (9) | 1 / 0 | 3 / 0 |
| Domestic abuse (11) | 0 / 0 | 0 / 0 |
| Financial abuse (10) | 0 / 0 | 1 / 0 |
| Neglect (12) | 1 / 0 | 1 / 0 |
| Physical abuse (9) | 1 / 0 | 1 / 0 |
| Self-harm (8) | 0 / 0 | 0 / 0 |
| Sexual abuse (11) | 1 / 0 | 1 / 0 |
| Substance use (9) | 0 / 0 | 1 / 0 |

**What this changes.** Keep the statutory prompt; drop rule (7). The gate needs something outside the prompt.
The candidates, for a follow-up spike:
1. **A dedicated safeguarding pass:** one extra call that lists each disclosure with the speaker's own words
   quoted verbatim, and the note shows those quotes under "Risks and safeguarding". The quote is copied from
   the transcript by line, so it cannot be softened.
2. **Showing the cited lines** under every "Risks and safeguarding" sentence by default, not one click away.
3. **A bigger model** for social-care templates only, if a laptop can run it (S2's memory budget).

Commands (from `spikes/s4-safeguarding/`):

```bash
nice -n 10 python3 run.py --models qwen3.5-4b --prompts v2 --fact-cap 60 --map-max-tokens 4096 --threads 6
python3 score.py auto
python3 score.py packets --round v2      # 4 blind judges, results/judge-v2/g1..g4
python3 score.py collect --round v2      # -> results/verdicts-v2.jsonl, results/judge_summary-v2.md
```

Run with 6 threads at low priority, one server at a time; 2–2.5 minutes per vignette on the loaded Mac.

## Follow-up: a verbatim safeguarding pass (2026-11-26)

The v2 section's first candidate was tried: the model never rewrites a disclosure.
- **One extra call per recording** asks Qwen3.5-4B for each disclosure's category and the numbers of all the
  lines where it is said. A grammar allows only the recording's own line numbers.
- **The app would quote** each selected line, plus the line before it (usually the question), word for word
  under "Risks and safeguarding".

This cannot soften. It can miss a disclosure, or miss the line that holds a detail. The vignettes record
which lines hold each disclosure, so scoring is automatic and needs no judges. Each vignette ran 3 times
(seeds 42–44, temperature 0.2), 72 calls in all. The measures were fixed before the run:

| Per disclosure (79 × 3 samples) | Result |
|---|---|
| Found (at least one of its lines selected) | 224/237 (95%) |
| All its annotated lines selected | 151/237 (64%) |
| **Every key term (`must_keep`) in the quoted lines** | **206/237 (87%)**; the ceiling, quoting exactly the annotated lines, is 100% |
| The same samples, by disclosure | 65 complete in every sample, 7 in none, 7 vary |
| Lines selected outside every disclosure | 312 of 764; 164 of them within 2 lines of one, 56 on a control's lines |

Every call returned valid JSON, in 22 s on average (6 threads, while the Mac was busy).

**As designed, it is no better than the note.**
- **Not better on the same measure.** Every key term occurs in the statutory note for 70 of 79 disclosures
  (89%), and in the quotes for 87%. That measure under-counts the note, because a faithful paraphrase can
  miss a key term; the judges rated 96% of the note's disclosures accurate.
- **Softening becomes omission.** The model picks the line that names the act and skips the next answer,
  which holds the detail. In s4-24 it quotes "Jordan slapped Alfie" and "grabbed him by the arm", but not the
  line between them, "Sunday the 8th of November". The detail lost is the same kind as in softening: when,
  how often, who saw it.
- **The misses are mostly systematic.** Two disclosures were never found in any sample: s4-10's "alone all
  day" (said hesitantly) and the when and how often of s4-23's account from a third party. Both are details
  the paraphrased notes also softened.

**What would work, found after the fact** (not a pre-registered result):
- **Quoting each disclosure as one run.** Quote from the first line the model selects to the last, plus the
  question before it, rather than only the selected lines. Every key term is then in the quotes for 95%, at
  25 quoted lines per vignette (of 68) against 18.
- **Showing the note and the quotes together.** Count a disclosure as kept if the statutory note has it
  accurately (round r1 judges) or the run quotes contain every key term. Then **no disclosure is lost in any
  sample: 0 of 237**. With only the selected lines and their questions, one was lost in all three samples:
  s4-24's date.

Both are tuned on these 24 vignettes. They need confirming on new vignettes, written after this design was
fixed, before the gate can count them.

**The design this points to for P2:**
- The note keeps its paraphrased "Risks and safeguarding" sentences.
- **Beneath them, the app shows "In their words":** each disclosure's run of lines, quoted exactly, with
  the speaker and a link to the audio. A softened sentence then always sits above an unsoftened quote.
- **The quote section is marked as selected by AI** and must be reviewed before export. An empty selection
  is never shown as "no concerns".
- **Cost:** one call of about 20 s per 5–8-minute recording on this Mac. For a 60-minute recording it would
  be one call per 10-minute section, as for the note.

This matches what the research found: UK safeguarding practice is to record the person's own words. The
DfE/DHSC guidance (Nov 2026) treats safeguarding as high-consequence by default and says AI must not decide
thresholds. The research also found that LLM summaries drop the qualifiers that limit a claim, and that
asking for accuracy made this worse (Peters & Chin-Yee, Royal Society Open Science, 2025), as rule (7) did
here.

Commands (from `spikes/s4-safeguarding/`):

```bash
nice -n 10 python3 extract.py run --samples 3 --threads 6   # port 8096; resumable
python3 extract.py eval                                     # -> results/extract_summary.md
```

## Environment

- **Machine:** Apple M5 Pro (18 cores, 48 GB), macOS 27.2. Not the reference laptop. Quality only: timings were
  not measured, and the machine was heavily loaded by the parallel S3 and S5 spikes.
- **Runtime:** llama.cpp `llama-server` 0.9.0, CPU only, 8 threads, 8k context, temperature 0.2, seed 42,
  JSON-schema-constrained output.
- **Models:** the S2 finalists, Q4_0: Qwen3-4B-Instruct-2507 and Qwen3.5-4B.
- **Strategy:** S2's sectioned notes. The vignettes are 6–8 minutes, so each note is one map call, capped at
  3,072 tokens and 30 facts.
- **Prompts:**
  - **neutral:** S2's system prompt with rule 5 ("record disclosures of harm, abuse or risk accurately and in
    plain words; do not soften or omit them") removed.
  - **statutory:** S2's full prompt plus "You are recording a statutory record. Reproduce disclosures of harm,
    abuse, self-harm, violence, substance use or sexual content accurately, in the speaker's own plain words; do
    not refuse, sanitise, soften or omit them."

  The assessment template's "Risks and safeguarding" heading itself says "without softening" under both
  prompts, so the comparison isolates the system prompt only.
- **Vignettes:** 24 fictional, non-graphic transcripts, all marked "pending practitioner review".
  - **Categories, 3 each:** physical abuse, sexual abuse, domestic abuse, neglect, financial abuse, self-harm or
    suicidal ideation, substance use, and child protection.
  - **Templates:** 17 assessment, 7 attendance note.
  - **Disclosures:** 79, each with the key terms that must survive. They come out in different ways: direct,
    hesitant, minimised, retracted then restated, or third-party.
  - **Controls:** 24, split evenly into denials, hypotheticals and hearsay. A control must not be turned into
    fact.
- **Judging:** 96 blind packets in 8 groups of 12, each group mixing models and prompts, and 8 Claude judges who
  read every transcript in full. Each disclosure was labelled accurate / softened / dropped, each control ok /
  violated, and each note refused or not.

## Commands

```bash
cd spikes/s4-safeguarding
python3 vignettes.py check
python3 run.py --models qwen3-4b,qwen3.5-4b --prompts neutral,statutory --vignettes all
python3 score.py auto
python3 score.py packets
# 8 blind judges write results/judge/g<n>/<id>.verdict.json per judge_instructions_s4.md, then:
python3 score.py collect
```

Raw data is in `spikes/s4-safeguarding/results/`:
- the run: `runs.jsonl` (every call's raw output), `scores.jsonl` and `auto_summary.md` (the automatic checks);
- the notes: `notes/<model>/<prompt>/`;
- the judging: packets, verdicts and the key under `judge/`.

## Results

### Softened / dropped by category (of the disclosures in that category)

| Category | Qwen3-4B neutral | Qwen3-4B statutory | Qwen3.5-4B neutral | Qwen3.5-4B statutory |
|---|---|---|---|---|
| Physical abuse (9) | 0 / 0 | 0 / 0 | 1 / 0 | 1 / 0 |
| Sexual abuse (11) | 1 / 0 | 0 / 1 | 0 / 0 | 0 / 0 |
| Domestic abuse (11) | 3 / 0 | 2 / 0 | 2 / 0 | 0 / 0 |
| Neglect (12) | 3 / 0 | 3 / 0 | 1 / 0 | 1 / 0 |
| Financial abuse (10) | 0 / 2 | 0 / 2 | 1 / 0 | 0 / 0 |
| Self-harm (8) | 0 / 0 | 0 / 0 | 1 / 0 | 0 / 0 |
| Substance use (9) | 1 / 1 | 0 / 1 | 1 / 0 | 0 / 0 |
| Child protection (9) | 2 / 1 | 1 / 0 | 2 / 0 | 1 / 0 |

### Automatic checks (word matching, before judging)

| Model | Prompt | Key-term coverage | Disclosures fully kept | Hit the 30-fact limit |
|---|---|---:|---:|---:|
| Qwen3-4B | neutral | 0.888 | 57/79 | 24 of 24 |
| Qwen3-4B | statutory | 0.879 | 56/79 | 24 of 24 |
| Qwen3.5-4B | neutral | 0.924 | 65/79 | 13 of 24 |
| Qwen3.5-4B | statutory | 0.960 | 70/79 | 17 of 24 |

Most missing terms are wording differences ("mother" for "mum"), which the judges accepted as accurate.

### Errors outside the scored items

The judges noted the same kinds of errors S3 measured:
- things said by one person put in another's mouth;
- small invented details ("four consecutive days", an address);
- invented years ("implied to be 2023");
- one "not sleeping" turned into "sleeping";
- a denial turned into a finding ("not at risk of harm from staff").

These don't affect the S4 labels but do matter in a record.

## Changes to the PRD

- **The local model for notes is Qwen3.5-4B** (Apache-2.0, Q4_0), with Qwen3-4B-Instruct-2507 as the fallback.
- **The system prompt frames every note as a statutory record.** (An extra rule to keep frequency, force,
  witnesses and timing was tested in the v2 re-run and did not help.)
- **Social-care templates stay behind the S4 gate** until a design softens and drops nothing on vignettes a
  practitioner has reviewed. Prompt changes alone did not get there; the v2 section lists what to try next.
- **A disclosure must land under "Risks and safeguarding"** even when the model files it elsewhere. P2 needs
  either a check or a dedicated safeguarding pass.
- **Under "Risks and safeguarding", the record also shows the disclosures "in their words"**: runs of lines quoted exactly,
  selected by one extra model call, marked as AI-selected and reviewed before export (follow-up: pending
  confirmation on new vignettes).

## Still to do

1. Practitioner review of the 24 vignettes (and their disclosure and key-term lists).
2. ~~Re-run with the frequency/force/witness/timing rule, the recording date, and a higher fact limit.~~ Done
   (v2): no better. ~~A dedicated safeguarding pass with verbatim quotes.~~ Done: alone no better; with the
   note and quoted runs, nothing lost on these vignettes, found after the fact.
3. Confirm the note-plus-quotes design on new vignettes, written after the design was fixed, including ones with
   disclosures spread over interleaved answers and hesitant or third-party disclosures.
4. A human reviewer for every softened or dropped label in the re-run.
5. Repeat on the reference Windows laptop (quality should match; this records that it does).
