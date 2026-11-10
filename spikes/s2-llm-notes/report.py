#!/usr/bin/env python3
"""Turn results/<run>.jsonl (+ .scores.jsonl, bench-*.jsonl) into markdown tables for the write-up.

  python report.py --run screen [--run full ...]
"""
import argparse
import json

import common as C


def load_jsonl(p):
    return [json.loads(l) for l in p.read_text().splitlines() if l.strip()] if p.exists() else []


def fmt_s(x):
    if x is None:
        return "–"
    return f"{x / 60:.1f} min" if x >= 90 else f"{x:.0f} s"


def bench_table():
    rows = []
    for p in sorted(C.RESULTS.glob("bench-*.jsonl")):
        for r in load_jsonl(p):
            test = f"pp{r['n_prompt']}" if r.get("n_prompt") else f"tg{r['n_gen']}"
            rows.append((p.stem.removeprefix("bench-"), r["n_threads"], test, r["avg_ts"]))
    if not rows:
        return ""
    out = ["| model | threads | test | tok/s |", "|---|---:|---|---:|"]
    out += [f"| {m} | {t} | {test} | {ts:.1f} |" for m, t, test, ts in rows]
    return "\n".join(out)


def grid_table(run):
    recs = load_jsonl(C.RESULTS / f"{run}.jsonl")
    scores = {(s["model"], s["transcript"], s["strategy"]): s
              for s in load_jsonl(C.RESULTS / f"{run}.scores.jsonl")}
    single = {(r["model"], r["transcript"]): r for r in recs if r.get("stage") == "single"}
    summ = [r for r in recs if r.get("stage") == "summary"]
    out = ["| model | transcript | strategy | prompt tok | prefill tok/s | gen tok/s | after stop | "
           "incremental after stop | peak RAM | JSON ok | support | weak | fact recall | traps hit |",
           "|---|---|---|---:|---:|---:|---:|---:|---:|:-:|---:|---:|---:|---|"]
    for r in sorted(summ, key=lambda r: (r["transcript"], r["model"], r["strategy"])):
        k = (r["model"], r["transcript"], r["strategy"])
        s = scores.get(k, {})
        sp = single.get((r["model"], r["transcript"])) if r["strategy"] == "single" else None
        # "sectioned" is derived from the mapreduce run's map calls (no reduce).
        call_strategy = "mapreduce" if r["strategy"] == "sectioned" else r["strategy"]
        stages = ("map",) if r["strategy"] == "sectioned" else ("map", "reduce", "single")
        maps = [x for x in recs if x.get("model") == r["model"] and x.get("transcript") == r["transcript"]
                and x.get("strategy") == call_strategy and x.get("stage", "").startswith(stages)]
        ptok = sp["prompt_n"] if sp else sum(x.get("prompt_n") or 0 for x in maps)
        ptps = [x["prompt_tps"] for x in maps if x.get("prompt_tps")]
        gtps = [x["gen_tps"] for x in maps if x.get("gen_tps")]
        traps = s.get("trap_violations")
        out.append(
            f"| {r['model']} | {r['transcript']} ({r['minutes']:.0f} min) | {r['strategy']} | {ptok} | "
            f"{sum(ptps) / max(len(ptps), 1):.0f} | {sum(gtps) / max(len(gtps), 1):.1f} | {fmt_s(r.get('after_stop_s'))} | "
            f"{fmt_s(r.get('incremental_after_stop_s'))} | {r.get('peak_rss_mb', 0) / 1024:.1f} GB | "
            f"{'✓' if r.get('json_ok') else '✗'} | {s.get('support', '–')} | {s.get('weak_rate', '–')} | "
            f"{s.get('fact_recall', '–')} | {', '.join(traps) if traps else ('none' if traps == [] else '–')} |")
    return "\n".join(out)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", action="append", required=True)
    a = ap.parse_args()
    b = bench_table()
    if b:
        print("### llama-bench (CPU only, -ngl 0 -dev none)\n\n" + b + "\n")
    for run in a.run:
        print(f"### Run `{run}`\n\n" + grid_table(run) + "\n")
