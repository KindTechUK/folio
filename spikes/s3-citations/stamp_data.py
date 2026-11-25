#!/usr/bin/env python3
"""Add realistic start/end times to a hand-written synthetic transcript, validate it, and write data/<stem>.json.

The four S3 test transcripts were written as speaker turns without times. Times come from word counts:
about 150 words/min with +-15% per-line jitter and 0.3-1.2 s gaps between turns, seeded by the transcript
id so the output is reproducible. The base rate is nudged (2.0-3.2 words/s) to keep the total within
10-15 minutes.

  python stamp_data.py SRC.json [SRC.json ...]     # stamp, validate, write data/<stem>.json
  python stamp_data.py --check                     # validate every data/*.json
"""
import json
import pathlib
import random
import sys

import common as C

KINDS = ["negation", "hypothetical", "hearsay", "corrected", "not_said", "speaker_attribution"]


def stamp(t, rate):
    rnd = random.Random(t["id"])
    now, out = rnd.uniform(0.2, 1.5), []
    for l in t["lines"]:
        words = len(l["text"].split())
        dur = max(0.6, words / rate * rnd.uniform(0.85, 1.15))
        out.append({"id": l["id"], "start": round(now, 1), "end": round(now + dur, 1),
                    "speaker": l["speaker"], "text": l["text"]})
        now += dur + rnd.uniform(0.3, 1.2)
    return out


def validate(t, new=True):
    errs = []
    ids = [l["id"] for l in t["lines"]]
    if ids != [f"L{i:04d}" for i in range(1, len(ids) + 1)]:
        errs.append("line ids not sequential")
    errs += [f"{l['id']} unknown speaker {l['speaker']}" for l in t["lines"] if l["speaker"] not in t["speakers"]]
    known = set(ids)
    for item in t["facts"] + t["traps"]:
        errs += [f"{item['id']} cites missing {x}" for x in item["lines"] if x not in known]
    for a, b in zip(t["lines"], t["lines"][1:]):
        if not (a["start"] < a["end"] <= b["start"]):
            errs.append(f"times not increasing at {a['id']}")
            break
    counts = {k: sum(1 for x in t["traps"] if x["kind"] == k) for k in KINDS}
    bad_kinds = {x["kind"] for x in t["traps"]} - set(KINDS)
    if bad_kinds:
        errs.append(f"unknown trap kinds {bad_kinds}")
    mins = C.duration_min(t["lines"])
    n_omit = sum(1 for f in t["facts"] if f.get("omission"))
    if new:
        errs += [f"only {n} {k} traps" for k, n in counts.items() if n < 3]
        if n_omit < 3:
            errs.append(f"only {n_omit} omission facts")
        if not 10 <= mins <= 15:
            errs.append(f"duration {mins:.1f} min")
        if not 130 <= len(ids) <= 200:
            errs.append(f"{len(ids)} lines")
    summary = (f"{t['id']}: {len(ids)} lines, {mins:.1f} min, {len(t['facts'])} facts ({n_omit} omission), "
               f"{len(t['traps'])} traps {counts}")
    return errs, summary


def main():
    if sys.argv[1:] == ["--check"]:
        bad = 0
        for p in sorted(C.DATA.glob("*.json")):
            t = json.loads(C.read_text(p))
            errs, summary = validate(t, new="omission" in json.dumps(t["facts"]))
            print(summary)
            for e in errs:
                print("  ERROR", e)
            bad += len(errs)
        sys.exit(1 if bad else 0)
    for src in sys.argv[1:]:
        t = json.loads(C.read_text(src))
        words = sum(len(l["text"].split()) for l in t["lines"])
        rate = min(3.2, max(2.0, words / (10.5 * 60)))  # ~10.5 min of speech + ~2 min of gaps
        out = {k: t[k] for k in ("id", "title", "template", "speakers")}
        out["lines"] = stamp(t, rate)
        out["facts"] = [{"id": f["id"], "fact": f["fact"], "lines": f["lines"], "omission": bool(f.get("omission"))}
                        for f in t["facts"]]
        out["traps"] = [{k: x[k] for k in ("id", "kind", "must_not_claim", "lines", "note")} for x in t["traps"]]
        errs, summary = validate(out)
        print(summary, f"(rate {rate:.2f} words/s)")
        for e in errs:
            print("  ERROR", e)
        if not errs:
            dest = C.DATA / f"{pathlib.Path(src).stem}.json"
            C.write_text(dest, json.dumps(out, indent=1, ensure_ascii=False) + "\n")
            print("  wrote", dest)


if __name__ == "__main__":
    main()
