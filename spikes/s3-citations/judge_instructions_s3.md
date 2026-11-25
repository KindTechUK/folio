# Judge instructions (blind sentence-level review of S3 notes)

Each packet `results/judge/<run>/<id>.md` holds one machine-drafted note, or one part of a long note.
The model that wrote it is hidden. The packet lists:

- every note sentence to judge, with an id (`S001`, ...), its note section, and the transcript lines it
  cites, quoted verbatim with their `[mm:ss]` start times;
- the path of the full transcript (`results/judge/<run>/transcripts/<transcript id>.txt`);
- the transcript's **facts checklist** (part 1 of a note only) and its **traps**.

**Read the whole transcript before you judge.** Judge every sentence against the full transcript, not only
against the lines it cites. A sentence can be true while its citations are wrong, and it can be false while its
citations look relevant.

## Verdict file

Write `<id>.verdict.json` next to the packet. It must include every sentence id in the packet:

```json
{
  "sentences": {
    "S001": {"label": "supported", "cites_support": "yes", "trap": null, "why": ""},
    "S002": {"label": "contradiction", "cites_support": "no", "trap": "T03",
             "why": "L72-74: she says she did not hit her head"}
  },
  "facts": {"F01": "present", "F02": "partial", "F03": "absent"},
  "notes": "optional: one or two lines on overall quality"
}
```

If the packet says "Not judged in this part" under the facts checklist, write `"facts": {}`.

## Per sentence

**`label`** is one of the following, from Minute's taxonomy. If a sentence has several problems, pick the
first one that applies in this order: `contradiction`, `unsupported_fabrication`, `over_inference`, `supported`.

- `contradiction`: the transcript says something different. Examples: something that was denied is stated as
  true; the value a speaker corrected is used instead of the correction; a view, wish or action is given to the
  wrong person; a date, amount, name or number differs from the one said.
- `unsupported_fabrication`: the sentence states something that has no basis anywhere in the transcript.
  Examples: an invented detail, event, decision, diagnosis, date or figure.
- `over_inference`: the sentence starts from something that was said but claims more than was said. Examples:
  hearsay stated as established fact; a possibility, hypothetical or suggestion stated as a plan, decision or
  event; "agreed" when something was only discussed; a conclusion, diagnosis or judgement the speakers did not
  draw; uncertainty dropped ("about", "might", "I think").
- `supported`: everything the sentence states is said in the transcript. It has the right speaker, the right
  modality (denied, hypothetical, second-hand or uncertain where the transcript says so), and the corrected
  value. Paraphrase, summarising and referring to a speaker by their code (`S2`) or role are fine.

Tiny slips that change nothing material (a filler word, "Monday" for "on Monday") are `supported`. A wrong
detail that a reader could act on (a wrong date, amount, dose, owner or deadline) is `contradiction`.

**`cites_support`** says whether the **cited lines alone** support the sentence as written. Judge it whatever
the label is.

- `yes`: the cited lines support everything the sentence states.
- `partly`: the cited lines support part of the sentence, or support it only with help from lines that are not
  cited.
- `no`: the cited lines don't support it. They are unrelated, don't exist, there are no citations, or they
  say something different from the sentence. A sentence that misstates the lines it cites gets `no` or
  `partly`, never `yes`.

**`trap`**: the id of the trap whose forbidden claim the sentence makes, even implicitly (`"T07"`), or a list
when it makes several (`["T07", "T12"]`). Otherwise `null`. A sentence that makes a trap's claim is never
`supported`. (The S3 run's judges were given the single-id form only.)

**`why`**: up to 25 words, with line references. It is required whenever the label is not `supported` or
`cites_support` is not `yes`. Otherwise leave it empty.

## Per fact (part 1 of each note only)

Judge each fact against the whole note. Part 1 of a split note lists the whole note for this purpose.

- `present`: the note states the fact correctly. Paraphrase is fine.
- `partial`: the note is correct but misses a key detail, such as the date, the amount or who does it.
- `absent`: the fact is missing, or the note states it wrongly.

## Consistency

Be strict and apply the same standard to every packet. Don't guess which model wrote a note. If a label is a
close call, choose the one that a professional relying on the note would care about, and say why in `why`.
