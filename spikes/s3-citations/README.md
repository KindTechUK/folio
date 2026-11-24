# Spike S3: citations and faithfulness

The harness for spike S3 in [`docs/spikes/next-phase-spikes.md`](../../docs/spikes/next-phase-spikes.md):
can a small local model write note sentences with transcript time-span citations, how often does it state
things that were never said, and can a cheap flag catch the unsupported sentences?

This is throwaway spike code, copied from `spikes/s2-llm-notes/` and cut down. It is plain Python (stdlib only)
driving a **CPU-only `llama-server`**. The NLI flag needs `transformers` and `torch`, for evaluation only. S3
measures quality, not speed.

## What changed from S2

- **Time-stamped transcript lines.** The model sees `L52 [12:34] S2: text` (`common.render_lines`) and cites
  line numbers. The JSON schema forces those to be integers. After generation the harness maps each cited line
  to that line's `{start, end}` and writes the planned product shape next to the note:
  `results/notes/<run>/<model>/<transcript>.sectioned.spans.json` holds one
  `{id, section, sentence, cites: [{start, end}], lines: [n]}` per sentence.
- **Sectioned strategy only.** S2 recommended it: map calls over ~10-minute parts, and the note is the
  section-tagged facts. The map cap is 3072 tokens, as S2 recommended (S2 used 2048). `--strategies mapreduce`
  also runs S2's reduce call, which S3 doesn't score.
- **Two finalists only:** `qwen3-4b` (Qwen3-4B-Instruct-2507) and `qwen3.5-4b` (Qwen3.5-4B), both Q4_0.
  Defaults: 8 threads, 8k context, port 8093, temperature 0.2, seed 42.
- **Sentence-level scoring.** `score.py` scores each sentence: invalid cites, out-of-part cites and lexical
  support. Blind packets are per sentence (`judge_instructions_s3.md`), and `flag.py` evaluates the flag.

## Data

| File | Set | Template | Length | Facts (omission) | Traps |
|---|---|---|---|---|---|
| `data/synthetic_care_assessment.json` | tuning (S2) | assessment | 61 min, 680 lines | 35 | 16 |
| `data/synthetic_legal_interview.json` | tuning (S2) | attendance_note | 46 min, 323 lines | 29 | 16 |
| `data/synthetic_mh_review.json`: community mental health review, 3 speakers | test | assessment | 13.5 min, 179 lines | 18 (3) | 21 |
| `data/synthetic_housing_assessment.json`: housing and benefits assessment, 3 speakers | test | assessment | 12.4 min, 152 lines | 19 (4) | 21 |
| `data/synthetic_tribunal_interview.json`: employment-tribunal client interview, 3 speakers | test | attendance_note | 13.5 min, 171 lines | 17 (4) | 21 |
| `data/synthetic_planning_meeting.json`: multi-agency child-in-need review, 4 speakers | test | general_minutes | 13.5 min, 163 lines | 17 (4) | 21 |

All are fictional. The four test transcripts were written for S3 and are trap-dense. Each has at least 3 traps
of every kind (`negation`, `hypothetical`, `hearsay`, `corrected`, `not_said`, `speaker_attribution`). Each
also has at least 3 facts marked `"omission": true`: important facts mentioned only in passing. The test
transcripts were written as speaker turns without times. `stamp_data.py` added the times from word counts
(seeded, at about 150–190 words per minute plus pauses) and validated each file.

## Run

```bash
cd spikes/s3-citations
python stamp_data.py --check                                     # validate all six transcripts
python run.py --run s3 --models qwen3-4b,qwen3.5-4b --transcripts all    # one model at a time, port 8093
python score.py auto --run s3                                    # per-sentence automatic metrics
python score.py packets --run s3                                 # blind sentence-level judge packets
V=<venv with transformers torch sentencepiece protobuf>
$V/bin/python flag.py nli --run s3                               # NLI score per sentence (downloads the model once)
python report.py --run s3                                        # markdown tables
# ...blind judges write results/judge/s3/<packet>.verdict.json per judge_instructions_s3.md, then:
python score.py collect --run s3                                 # verdicts -> s3.verdicts.jsonl, judge metrics in s3.scores.jsonl
python flag.py eval --run s3                                     # flag precision/recall, citation validity
python report.py --run s3
```

`run.py --transcripts` also accepts `tuning`, `test`, or a comma-separated list of file stems. `run.py` appends to
`results/<run>.jsonl`, so delete it before a clean re-run.

## Outputs (`results/`)

- `s3.jsonl`: one record per map call (with the model's raw output, `json_ok`, `finish_reason` and the transcript
  part's line range), plus one `summary` per note.
- `notes/s3/<model>/<transcript>.sectioned.json`: the note (`sections -> sentences {text, cites}`).
- `notes/s3/<model>/<transcript>.sectioned.spans.json`: the same note as time spans.
- `s3.sentences.jsonl`: one row per note sentence. Fields:
  - `sid`: the sentence id (`S001`...), used by packets and verdicts;
  - `cites` and `invalid_cites`: the cited lines, and those that don't exist;
  - `out_of_part_cites`: cited lines that exist but are outside the transcript part the map call saw;
  - `support`: the lexical support.
- `s3.scores.jsonl`: one row per note. Judge metrics are added by `collect`.
- `s3.nli.jsonl`: P(entailment / neutral / contradiction) per sentence.
- `judge/s3/`: the packets `<id>.md` (a long note is split into `<id>-1`, `<id>-2`, ... of 120 or fewer sentences),
  plus `_key.json` (packet → model, transcript and sentence ids; **don't show it to judges**),
  `transcripts/<id>.txt` (each full transcript as the model saw it) and, later, `<id>.verdict.json`.

## Judging

Give each judge one or more packets plus `judge_instructions_s3.md`, but not `_key.json` and not this folder's
`results/notes/`. A judge labels every sentence:

- `label`: `supported`, `unsupported_fabrication`, `contradiction` or `over_inference`;
- `cites_support`: `yes`, `partly` or `no`;
- `trap`: the id of the trap the sentence hits, if any;
- `why`.

On part 1 of each note, the judge also marks each checklist fact as `present`, `partial` or `absent`. Judges check
each sentence against the full transcript, not only against the lines it cites.

## Flag evaluation (`flag.py eval`)

- **Positive class:** sentences labelled `unsupported_fabrication`, `contradiction` or `over_inference`.
- **Flags:**
  - lexical: `support < t`;
  - NLI: `P(entailment) < t`, where the premise is the cited lines and the hypothesis is the sentence
    (`cross-encoder/nli-deberta-v3-small`);
  - combined: either of the two.
- **Thresholds** are picked on the tuning set (the two S2 transcripts) in two ways, best F1 and "catch-all",
  then applied unchanged to the test set (the four S3 transcripts). Catch-all is the highest precision with no
  fabrication or contradiction left unflagged.
- **Reported:** precision, recall and F1, the number of unflagged fabrications or contradictions, and citation
  validity (the share of sentences with `cites_support` = `yes`, and = `yes` or `partly`).
