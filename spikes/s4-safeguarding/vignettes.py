#!/usr/bin/env python3
"""Validate the S4 vignettes and give them timestamps.

  python vignettes.py check            validate every data/s4-*.json
  python vignettes.py stamp            (re)write start/end on every line, then validate
  python vignettes.py list             one row per vignette (id, category, template, counts, minutes)

Timestamps are synthetic but realistic: 2.5 spoken words per second (150 wpm), a minimum of 1.2 s per
line, and a 0.3-1.1 s pause between lines that is derived from a hash of the line, so it is stable.
"""
import argparse
import hashlib
import json
import pathlib
import sys

import common as C

CATEGORIES = ["physical_abuse", "sexual_abuse", "domestic_abuse", "neglect", "financial_abuse",
              "self_harm", "substance_use", "child_protection"]
STYLES = {"direct", "hesitant", "minimised", "retracted_restated", "third_party"}
KINDS = {"denial", "hypothetical", "hearsay"}
WPS = 2.5


def stamp(t):
    clock = 0.6
    for l in t["lines"]:
        words = len(l["text"].split())
        dur = max(1.2, words / WPS)
        h = int(hashlib.sha1(f"{t['id']}|{l['id']}".encode()).hexdigest()[:6], 16)
        l["start"] = round(clock, 1)
        l["end"] = round(clock + dur, 1)
        clock = l["end"] + 0.3 + (h % 9) / 10
        # keep key order id, start, end, speaker, text
        for k in ("speaker", "text"):
            l[k] = l.pop(k)


def check(t):
    errs = []
    for k in ("id", "title", "template", "category", "setting", "review", "speakers", "lines",
              "disclosures", "controls"):
        if k not in t:
            errs.append(f"missing {k}")
    if errs:
        return errs
    if t["review"] != "pending practitioner review":
        errs.append("review field")
    if t["category"] not in CATEGORIES:
        errs.append(f"category {t['category']}")
    if t["template"] not in C.KEY_SECTION:
        errs.append(f"template {t['template']}")
    lines = t["lines"]
    by_id = {l["id"]: l for l in lines}
    for i, l in enumerate(lines, 1):
        if l["id"] != f"L{i:04d}":
            errs.append(f"line id {l['id']} at position {i}")
            break
        if l["speaker"] not in t["speakers"]:
            errs.append(f"{l['id']} speaker {l['speaker']}")
    if not 60 <= len(lines) <= 110:
        errs.append(f"{len(lines)} lines (want 60-110)")
    if all("end" in l for l in lines):
        m = C.duration_min(lines)
        if not 5.0 <= m <= 8.0:
            errs.append(f"{m:.1f} min (want 5-8)")
    else:
        errs.append("not stamped")
    ds = t["disclosures"]
    if not 2 <= len(ds) <= 5:
        errs.append(f"{len(ds)} disclosures (want 2-5)")
    for i, d in enumerate(ds, 1):
        if d["id"] != f"D{i:02d}":
            errs.append(f"disclosure id {d['id']}")
        if d.get("style") not in STYLES:
            errs.append(f"{d['id']} style {d.get('style')}")
        missing = [x for x in d["lines"] if x not in by_id]
        if missing or not d["lines"]:
            errs.append(f"{d['id']} bad lines {missing or 'none'}")
            continue
        toks = C.norm_tokens(" ".join(by_id[x]["text"] for x in d["lines"]))
        if not d.get("must_keep"):
            errs.append(f"{d['id']} no must_keep")
        for term in d.get("must_keep", []):
            if not C.has_term(toks, term):
                errs.append(f"{d['id']} must_keep '{term}' not in its lines")
    cs = t["controls"]
    if not 1 <= len(cs) <= 2:
        errs.append(f"{len(cs)} controls (want 1-2)")
    for i, c in enumerate(cs, 1):
        if c["id"] != f"C{i:02d}":
            errs.append(f"control id {c['id']}")
        if c.get("kind") not in KINDS:
            errs.append(f"{c['id']} kind {c.get('kind')}")
        if not c.get("must_not_claim"):
            errs.append(f"{c['id']} no must_not_claim")
        if not c.get("flag_terms"):
            errs.append(f"{c['id']} no flag_terms")
        missing = [x for x in c.get("lines", []) if x not in by_id]
        if missing or not c.get("lines"):
            errs.append(f"{c['id']} bad lines {missing or 'none'}")
    return errs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["check", "stamp", "list"])
    ap.add_argument("files", nargs="*")
    a = ap.parse_args()
    paths = [pathlib.Path(f) if pathlib.Path(f).exists() else C.DATA / (f if f.endswith(".json") else f + ".json")
             for f in a.files] or C.vignette_paths()
    bad = 0
    for p in paths:
        t = C.load_transcript(p)
        if a.cmd == "stamp":
            stamp(t)
            p.write_text(json.dumps(t, indent=1, ensure_ascii=False) + "\n")
        if a.cmd == "list":
            words = sum(len(l["text"].split()) for l in t["lines"])
            styles = ",".join(sorted({d["style"] for d in t["disclosures"]}))
            kinds = ",".join(c["kind"] for c in t["controls"])
            print(f"{t['id']:8} {t['category']:17} {t['template']:15} lines={len(t['lines']):3} "
                  f"words={words:4} min={C.duration_min(t['lines']):.1f} D={len(t['disclosures'])} "
                  f"C={len(t['controls'])} styles={styles} controls={kinds}")
            continue
        errs = check(t)
        if errs:
            bad += 1
            print(f"{p.name}: " + "; ".join(errs))
        else:
            print(f"{p.name}: ok ({len(t['lines'])} lines, {C.duration_min(t['lines']):.1f} min)")
    if a.cmd != "list":
        print(f"{len(paths) - bad}/{len(paths)} ok")
        sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
