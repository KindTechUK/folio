#!/usr/bin/env python3
"""Markdown tables for the S3 write-up, from results/<run>.jsonl and results/<run>.scores.jsonl.

  python report.py --run s3
"""
import argparse
import json

import common as C
from score import load_jsonl


def run_table(run):
    recs = load_jsonl(C.RESULTS / f"{run}.jsonl")
    maps = [r for r in recs if r.get("stage", "").startswith("map")]
    out = ["| model | transcript | map calls | JSON valid | salvaged | hit the cap | facts |",
           "|---|---|---:|---:|---:|---:|---:|"]
    for r in sorted((r for r in recs if r.get("stage") == "summary"), key=lambda r: (r["split"], r["transcript"], r["model"])):
        ms = [m for m in maps if (m["model"], m["transcript"]) == (r["model"], r["transcript"])]
        out.append(f"| {r['model']} | {r['transcript']} ({r['minutes']:.0f} min) | {len(ms)} | "
                   f"{sum(m['json_ok'] for m in ms)} | {sum(m['salvaged'] for m in ms)} | "
                   f"{sum(m['finish_reason'] == 'length' for m in ms)} | {r['n_facts']} |")
    return "\n".join(out)


def score_table(run):
    rows = load_jsonl(C.RESULTS / f"{run}.scores.jsonl")
    judged = any("judged" in r for r in rows)
    head = ("| set | model | transcript | sentences | with ≥1 cite | cites | invalid cites | out-of-part cites | "
            "lexical support mean / median | weak (<0.25) |")
    sep = "|---|---|---|---:|---:|---:|---:|---:|---:|---:|"
    if judged:
        head += " supported | fabrication | contradiction | over-inference | cites yes | yes/partly | fact recall | traps hit |"
        sep += "---:|---:|---:|---:|---:|---:|---:|---|"
    out = [head, sep]
    for r in sorted(rows, key=lambda r: (r["split"], r["transcript"], r["model"])):
        line = (f"| {r['split']} | {r['model']} | {r['transcript']} | {r['n_sentences']} | {r['sent_with_cite']} | "
                f"{r['n_cites']} | {r['invalid_cites']} | {r['out_of_part_cites']} | "
                f"{r['support_mean']} / {r['support_median']} | {r['weak_rate']} |")
        if judged:
            line += (f" {r.get('supported', '–')} | {r.get('unsupported_fabrication', '–')} | "
                     f"{r.get('contradiction', '–')} | {r.get('over_inference', '–')} | {r.get('cites_yes', '–')} | "
                     f"{r.get('cites_yes_or_partly', '–')} | {r.get('fact_recall', '–')} | "
                     f"{', '.join(r.get('traps_hit', [])) or '–'} |")
        out.append(line)
    return "\n".join(out)


def support_hist(run):
    rows = load_jsonl(C.RESULTS / f"{run}.sentences.jsonl")
    bins = [0, 0.25, 0.5, 0.75, 1.0001]
    out = ["| model | 0–0.25 | 0.25–0.5 | 0.5–0.75 | 0.75–1 | sentences |", "|---|---:|---:|---:|---:|---:|"]
    for model in sorted({r["model"] for r in rows}):
        xs = [r["support"] for r in rows if r["model"] == model]
        counts = [sum(1 for x in xs if lo <= x < hi) for lo, hi in zip(bins, bins[1:])]
        out.append(f"| {model} | " + " | ".join(f"{c} ({c / len(xs):.0%})" for c in counts) + f" | {len(xs)} |")
    return "\n".join(out)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", required=True)
    a = ap.parse_args()
    print(f"### Run `{a.run}`: map calls\n\n{run_table(a.run)}\n")
    print(f"### Per note\n\n{score_table(a.run)}\n")
    print(f"### Lexical support per sentence\n\n{support_hist(a.run)}\n")
