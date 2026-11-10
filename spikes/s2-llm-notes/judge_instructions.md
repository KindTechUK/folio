# Judge instructions (blind review of S2 notes)

Each packet `results/judge/<run>/<id>.md` contains:
- one machine-drafted note, with the model and strategy hidden
- the path of the source transcript
- the transcript's **facts checklist**
- the transcript's **traps**

Write `<id>.verdict.json` next to the packet:

```json
{
  "facts": {"F01": "present", "F02": "partial", "F03": "absent"},
  "traps": {"T01": "ok", "T02": "violated"},
  "other_fabrications": [
    {"sentence": "exact note sentence", "why": "what the transcript actually says"}
  ],
  "notes": "one or two lines on overall quality (optional)"
}
```

Rules:
- **facts**
  - `present`: the note states the fact correctly, allowing paraphrase.
  - `partial`: the note is correct but misses a key detail, e.g. the date, the amount or who does it.
  - `absent`: the fact is missing, or stated wrongly.
- **traps**
  - `violated`: any note sentence makes the forbidden claim, even implicitly. For example: a hypothetical
    stated as a plan, hearsay stated as established fact, the uncorrected value used, or one person's view
    attributed to another.
  - `ok`: no sentence makes the forbidden claim. Silence on the topic counts as `ok`.
- **other_fabrications**: note sentences that state something the transcript does not support and
  that no trap covers. Check them against the transcript file. Ignore harmless paraphrase and
  reasonable summarising. List only content that is wrong or unsupported.
- Judge the note text only. Citations are scored automatically elsewhere.
- Be strict and consistent: the same standard for every packet.
