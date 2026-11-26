#!/usr/bin/env python3
"""Fast checks for unsupported sentences: a speaker rule, a number rule, and MiniCheck.

  python fastcheck.py minicheck --run s3 [--threads 4]   -> results/<run>.minicheck.jsonl (downloads the model once)
  python fastcheck.py eval --run s3                      -> results/<run>.fastcheck.md

Follow-up 3 of the S3 write-up. The judged errors that remain are mostly the wrong speaker and changed details,
and check.py's second pass with the note model reached precision 0.49-0.68. Industry checkers pair a cheap,
deterministic check for specific error types with a small trained fact-checker. Both are tried here, on the
sentences the judges already labelled, so no new judging is needed. The rules were fixed before any result.

Evidence for every check = the sentence's valid cited lines plus the line before each (check.py's `prev`).

speaker rule  The sentence attributes something to a participant: a participant reference (a name from the
              speaker labels, a first name or surname, or a role such as "the solicitor" or "her son") comes
              before a reporting verb (said, reported, explained, wants, feels, ...) within six words, or
              follows "According to". Flagged if none of those participants spoke any evidence line.
number rule   A number in the sentence (digits, ordinals or number words; the words "one" and "zero" are
              ignored) is not among the numbers in the evidence lines (spoken numbers are parsed, and
              "twenty twenty-six" also counts as 2026) and is not the recording's day or year.
rules         speaker rule OR number rule.
minicheck     MiniCheck-Flan-T5-Large (0.8B, MIT licence; arXiv 2404.10774): P(supported) for (evidence,
              sentence), with the recording date as the evidence's first line. Flagged below a threshold
              chosen on the tuning set, like the other score flags.
combinations  with check.py's `prev` score (p_ok), on the runs that have one: both scores below their
              thresholds (AND), either below (OR), and the mean of the two below a threshold.

Positives, splits and metrics as in flag.py: thresholds are chosen on the tuning set and applied unchanged to
the test set. For a run whose judged notes include copies of another run's notes (v2's `@s3` calibration
copies), only the run's own notes are scored.
"""
import argparse
import json
import re
import time

import common as C
import flag as F
import score as S
from run import find_transcript

MINICHECK = "lytang/MiniCheck-Flan-T5-Large"

# ---- evidence ----


def evidence(by_num, cites):
    keep = set()
    for c in cites:
        if c in by_num:
            keep.update(n for n in (c - 1, c) if n in by_num)
    return [by_num[n] for n in sorted(keep)]


# ---- speaker rule ----

REPORTING = set("""said say says reported reports stated states explained explains described describes confirmed
confirms mentioned mentions told tells asked asks agreed agrees noted notes expressed expresses advised advises
disclosed discloses denied denies raised raises suggested suggests recommended recommends informed informs
highlighted highlights indicated indicates shared shares admitted admits emphasised emphasized emphasises
emphasizes acknowledged acknowledges wants wanted wishes wished feels felt believes believed thinks thought
prefers preferred requested requests proposed proposes offered offers clarified clarifies recalled recalls
added adds insisted insists worries worried concerned explained hopes hoped plans planned intends intended
outlined outlines""".split())
POSSESSIVE = {"the", "her", "his", "their", "a"}


def words(text):
    return re.findall(r"[A-Za-z]+(?:'[A-Za-z]+)?", text.replace("’", "'"))


def participant_refs(label):
    """('Kevin Doyle (son)') -> name tokens {'kevin', 'doyle', 'kevin doyle'} and roles ['son']. The name is
    the part of the label whose words are all capitalised; the other part is the role."""
    parts = [p.strip() for p in re.split(r"[()]", label) if p.strip()]
    names, roles = set(), []
    for p in parts:
        ws = words(p)
        if ws and all(w[0].isupper() for w in ws):
            names.add(" ".join(ws).lower())
            names.update(w.lower() for w in ws if len(w) > 2)
        else:
            head = p.split(",")[0].lower()
            roles += [r.strip() for r in head.split(" and ") if r.strip()]
    return names, roles


def find_refs(tokens, refs):
    """Positions (start, end) in `tokens` where a participant is referred to, with the speaker code."""
    out = []
    for code, (names, roles) in refs.items():
        for name in names:
            nt = name.split()
            for i in range(len(tokens) - len(nt) + 1):
                # "Hannah's mother said": a possessive name is not the one speaking
                if tokens[i:i + len(nt)] == nt:
                    out.append((i, i + len(nt), code))
        for role in roles:
            rt = role.split()
            for i in range(1, len(tokens) - len(rt) + 1):
                if (tokens[i - 1] in POSSESSIVE or tokens[i - 1].endswith("'s")) and tokens[i:i + len(rt)] == rt:
                    out.append((i - 1, i + len(rt), code))
    return out


def attributed_to(text, refs):
    """The speaker codes a sentence attributes its content to, or an empty set."""
    toks = [w.lower() for w in words(text)]
    found = find_refs(toks, refs)
    if toks[:2] == ["according", "to"]:
        return {c for s, _, c in found if s == min((s for s, _, _ in found), default=-1)}
    verb = next((i for i, w in enumerate(toks) if w in REPORTING), None)
    if verb is None:
        return set()
    return {c for s, e, c in found if e <= verb and verb - e <= 6}


def speaker_flag(s, by_num, refs):
    subj = attributed_to(s["text"], refs)
    if not subj:
        return False
    return not (subj & {l["speaker"] for l in evidence(by_num, s["cites"])})


# ---- number rule ----

UNITS = {w: i for i, w in enumerate("zero one two three four five six seven eight nine ten eleven twelve "
                                     "thirteen fourteen fifteen sixteen seventeen eighteen nineteen".split())}
TENS = {w: 10 * i for i, w in enumerate("_ _ twenty thirty forty fifty sixty seventy eighty ninety".split()) if i >= 2}
ORDINALS = {"first": 1, "second": 2, "third": 3, "fourth": 4, "fifth": 5, "sixth": 6, "seventh": 7, "eighth": 8,
            "ninth": 9, "tenth": 10, "eleventh": 11, "twelfth": 12, "thirteenth": 13, "fourteenth": 14,
            "fifteenth": 15, "sixteenth": 16, "seventeenth": 17, "eighteenth": 18, "nineteenth": 19,
            "twentieth": 20, "thirtieth": 30}
SCALES = {"hundred": 100, "thousand": 1000, "million": 1000000}


def word_numbers(text):
    """Numbers spelt out in words: 'twenty-seven thousand' -> 27000; 'twenty twenty-six' -> 20, 26 and 2026;
    'twenty-first' -> 21. Returns (value, from_word_one_or_zero) pairs."""
    toks = re.findall(r"[a-z]+", text.lower().replace("-", " "))
    toks = ["one" if w == "a" and i + 1 < len(toks) and toks[i + 1] in SCALES else w for i, w in enumerate(toks)]
    out, groups = [], []  # groups: consecutive simple numbers (< 100) for year readings

    def flush(total, cur, small):
        if total + cur or small:
            out.append((total + cur, small))

    i, n = 0, len(toks)
    while i < n:
        w = toks[i]
        if w in UNITS or w in TENS or w in ORDINALS:
            total, cur, small, start = 0, 0, False, i
            while i < n and (toks[i] in UNITS or toks[i] in TENS or toks[i] in ORDINALS or toks[i] in SCALES
                             or (toks[i] == "and" and i + 1 < n and (toks[i + 1] in UNITS or toks[i + 1] in TENS))):
                x = toks[i]
                if x in TENS and cur % 100 and cur % 100 < 100 and i > start and toks[i - 1] not in SCALES and toks[i - 1] != "and":
                    # 'twenty twenty': a new group starts (a year or time reading)
                    break
                if x in UNITS:
                    # 'twenty eighteen' is two groups (a year), not 38; 'five six' is two numbers
                    if (UNITS[x] >= 10 and cur % 100) or (cur % 10 and toks[i - 1] not in TENS):
                        break
                    cur += UNITS[x]
                elif x in TENS:
                    cur += TENS[x]
                elif x in ORDINALS:
                    cur += ORDINALS[x]
                    i += 1
                    break
                elif x in SCALES:
                    cur = max(cur, 1) * SCALES[x]
                    if SCALES[x] >= 1000:
                        total, cur = total + cur, 0
                i += 1
            value = total + cur
            # 'one', 'zero', 'first', 'second', 'third' alone are usually not quantities ('second-hand')
            small = i - start == 1 and toks[start] in ("one", "zero", "first", "second", "third")
            out.append((value, small))
            if toks[i:i + 3] == ["and", "a", "half"]:
                out.append((value + 0.5, False))                     # 'nine and a half' -> 9.5
            groups.append((start, i, value))
        else:
            i += 1
    for (s1, e1, v1), (s2, e2, v2) in zip(groups, groups[1:]):
        if e1 == s2 and 10 <= v1 < 100 and 0 < v2 < 100:
            out.append((v1 * 100 + v2, False))                       # 'twenty twenty-six'
        if e1 == s2 and 0 < v1 <= 12 and v2 in (15, 30, 45):
            out.append((v1 + v2 / 100, False))                       # 'two thirty' -> 2.30
        if e1 + 1 == s2 and toks[e1] == "point" and 0 <= v2 < 10:
            out.append((v1 + v2 / 10, False))                        # 'two point five' -> 2.5
    return out


def digit_numbers(text):
    t = re.sub(r"(?<=\d),(?=\d{3})", "", text)
    t = re.sub(r"\b[SL]\d+\b", " ", t)                                 # speaker codes and line ids
    out = []
    for h, m in re.findall(r"\b(\d{1,2})[:.](\d{2})\b", t):          # 4:00, 2.30
        out += [float(h), float(h) + int(m) / 100]
    t = re.sub(r"\b\d{1,2}[:.]\d{2}\b", " ", t)
    return out + [float(x) for x in re.findall(r"\d+(?:\.\d+)?", t)]


def numbers(text, keep_small=True):
    vals = set(digit_numbers(text))
    vals.update(float(v) for v, small in word_numbers(text) if keep_small or not small)
    return vals


def number_flag(s, by_num, rec_date):
    said = set()
    for l in evidence(by_num, s["cites"]):
        said |= numbers(l["text"])
    said |= {float(rec_date.day), float(rec_date.year)}
    return bool(numbers(s["text"], keep_small=False) - said)


# ---- MiniCheck ----


def premise(t, lines):
    return f"Recorded on {C.recorded_on(t)}.\n" + "\n".join(
        f"{t['speakers'].get(l['speaker'], l['speaker'])}: {l['text']}" for l in lines)


def minicheck(a):
    import torch
    import transformers
    from transformers import AutoModelForSeq2SeqLM, AutoTokenizer

    torch.set_num_threads(a.threads)
    tok = AutoTokenizer.from_pretrained(MINICHECK)
    model = AutoModelForSeq2SeqLM.from_pretrained(MINICHECK).eval()
    sents = S.load_jsonl(C.RESULTS / f"{a.run}.sentences.jsonl")
    tcache, items = {}, []
    for s in sents:
        if s["transcript"] not in tcache:
            t = C.load_transcript(find_transcript(s["transcript"]))
            tcache[s["transcript"]] = (t, {C.line_num(l["id"]): l for l in t["lines"]})
        t, by_num = tcache[s["transcript"]]
        ev = evidence(by_num, s["cites"])
        items.append((s, premise(t, ev) if ev else None))
    out, t0 = [], time.time()
    todo = [(s, p) for s, p in items if p]
    with torch.no_grad():
        for i in range(0, len(todo), 8):
            batch = todo[i:i + 8]
            enc = tok(["predict: " + p + tok.eos_token + s["text"] for s, p in batch], max_length=2048,
                      truncation=True, padding=True, return_tensors="pt")
            dec = torch.zeros((enc["input_ids"].size(0), 1), dtype=torch.long)
            logits = model(**enc, decoder_input_ids=dec).logits.squeeze(1)
            probs = torch.softmax(logits[:, [3, 209]], dim=-1)[:, 1].tolist()  # token 3 = "0", 209 = "1"
            for (s, _), p in zip(batch, probs):
                out.append({"model": s["model"], "transcript": s["transcript"], "sid": s["sid"],
                            "fp": S.sentence_fp(s), "p_support": round(p, 4)})
            print(f"  {min(i + 8, len(todo))}/{len(todo)}", end="\r", flush=True)
    for s, p in items:
        if not p:
            out.append({"model": s["model"], "transcript": s["transcript"], "sid": s["sid"], "fp": S.sentence_fp(s),
                        "p_support": 0.0, "no_evidence": True})
    secs = time.time() - t0
    meta = {"checker": MINICHECK, "threads": a.threads, "transformers": transformers.__version__,
            "torch": torch.__version__, "seconds_per_sentence": round(secs / max(len(todo), 1), 3)}
    C.write_text(C.RESULTS / f"{a.run}.minicheck.jsonl", "".join(json.dumps({**r, **meta}) + "\n" for r in out))
    print(f"\n{len(out)} sentences in {secs:.0f}s -> {a.run}.minicheck.jsonl")


# ---- evaluation ----


def choose(rows, flag, grid):
    scored = [(t, F.metrics(rows, [flag(r, t) for r in rows])) for t in grid]
    return max(scored, key=lambda x: (x[1]["f1"], x[1]["recall"]))


def evaluate(a):
    verdicts, _, missing = S.load_verdicts(a.run)
    if missing:
        raise SystemExit(f"{len(missing)} packets have no verdict yet")
    sents = {(s["model"], s["transcript"], s["sid"]): s for s in S.load_jsonl(C.RESULTS / f"{a.run}.sentences.jsonl")}
    own = {s["model"] for s in sents.values() if "@" not in s["model"]}

    def scores(path, field):
        if not path.exists():
            return None
        d = {(r["model"], r["transcript"], r["sid"]): r for r in S.load_jsonl(path)}
        return {k: r[field] for k, r in d.items() if k in sents and r["fp"] == S.sentence_fp(sents[k])}

    mc = scores(C.RESULTS / f"{a.run}.minicheck.jsonl", "p_support")
    ok = scores(C.RESULTS / f"{a.run}.check-prev.jsonl", "p_ok")
    tcache, rows = {}, []
    for v in verdicts:
        if v["model"] not in own:
            continue
        k = (v["model"], v["transcript"], v["sid"])
        s = sents[k]
        if s["transcript"] not in tcache:
            t = C.load_transcript(find_transcript(s["transcript"]))
            import datetime
            tcache[s["transcript"]] = ({C.line_num(l["id"]): l for l in t["lines"]},
                                       {c: participant_refs(lab) for c, lab in t["speakers"].items()},
                                       datetime.date.fromisoformat(t["recorded"]))
        by_num, refs, rec = tcache[s["transcript"]]
        sp, nu = speaker_flag(s, by_num, refs), number_flag(s, by_num, rec)
        rows.append({**v, "text": s["text"], "pos": v["label"] in S.POSITIVE, "speaker": sp, "number": nu,
                     "mc": (mc or {}).get(k), "p_ok": (ok or {}).get(k)})
    tune = [r for r in rows if r["split"] == "tuning"]
    test = [r for r in rows if r["split"] == "test"]
    L = [f"# Fast checks, run `{a.run}`", "", f"Generated by `python fastcheck.py eval --run {a.run}`.", "",
         f"Judged sentences scored: tuning {len(tune)} ({sum(r['pos'] for r in tune)} positive), test {len(test)} "
         f"({sum(r['pos'] for r in test)} positive); notes from {', '.join(sorted(own))}.", "",
         "## Rules (no threshold, no model)", ""]
    rules = {"speaker rule": lambda r: r["speaker"], "number rule": lambda r: r["number"],
             "speaker OR number": lambda r: r["speaker"] or r["number"]}
    for name, rule in rules.items():
        for split, part in (("tuning", tune), ("test", test)):
            if part:
                L.append(f"- **{name}**, {split}: {F.fmt(F.metrics(part, [rule(r) for r in part]))}")
    L += ["", "Labels of the sentences each rule flags (both splits):", ""]
    for name in ("speaker", "number"):
        fl = [r for r in rows if r[name]]
        counts = {}
        for r in fl:
            counts[r["label"]] = counts.get(r["label"], 0) + 1
        L.append(f"- {name} rule: {len(fl)} flagged: " + ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))

    flags = {}
    if mc is not None and all(r["mc"] is not None for r in rows):
        flags["minicheck p < t"] = (lambda r, t: r["mc"] < t, F.GRID)
        flags["minicheck p < t OR rules"] = (lambda r, t: r["mc"] < t or r["speaker"] or r["number"], F.GRID)
        if ok is not None and all(r["p_ok"] is not None for r in rows):
            pairs = [(x, y) for x in F.GRID for y in F.GRID]
            flags["minicheck < t1 AND p_ok < t2"] = (lambda r, t: r["mc"] < t[0] and r["p_ok"] < t[1], pairs)
            flags["minicheck < t1 OR p_ok < t2"] = (lambda r, t: r["mc"] < t[0] or r["p_ok"] < t[1], pairs)
            flags["mean(minicheck, p_ok) < t"] = (lambda r, t: (r["mc"] + r["p_ok"]) / 2 < t, F.GRID)
            flags["p_ok < t (check.py prev, for reference)"] = (lambda r, t: r["p_ok"] < t, F.GRID)
    elif mc is None:
        L += ["", f"No MiniCheck scores yet: run `python fastcheck.py minicheck --run {a.run}`."]
    if flags and tune and test:
        sec = S.load_jsonl(C.RESULTS / f"{a.run}.minicheck.jsonl")[0].get("seconds_per_sentence")
        L += ["", "## Score flags (threshold chosen for best F1 on the tuning set)", "",
              f"MiniCheck seconds per sentence: {sec} (batches of 8, CPU)."]
        for name, (flag, grid) in flags.items():
            t, m_tune = choose(tune, flag, grid)
            m_test = F.metrics(test, [flag(r, t) for r in test])
            L.append(f"- **{name}**, t = {t}\n  - tuning: {F.fmt(m_tune)}\n  - test:   {F.fmt(m_test)}")
    text = "\n".join(L) + "\n"
    C.write_text(C.RESULTS / f"{a.run}.fastcheck.md", text)
    print(text)
    if a.show:
        for r in rows:
            if r[a.show] and r["split"] == "tuning":
                print(f"[{r['label']}] {r['text']}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["minicheck", "eval"])
    ap.add_argument("--run", required=True)
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--show", choices=["speaker", "number"], help="print the tuning-set sentences a rule flags")
    a = ap.parse_args()
    {"minicheck": minicheck, "eval": evaluate}[a.cmd](a)
