#!/usr/bin/env python3
"""Extractive safeguarding pass: the model picks the transcript lines of each disclosure; the app quotes them.

  python extract.py run  [--samples 3] [--vignettes all]   -> results/extract.jsonl
  python extract.py eval                                   -> results/extract_summary.md

The S4 runs showed that a 4B model paraphrasing a disclosure sometimes loses a qualifier (how often, how
hard, who saw it, when), and that a prompt rule does not fix it. Here the model never writes the disclosure.
One call per vignette returns, for each disclosure it finds, a category and the line numbers where it is
said; a grammar limits the line numbers to the vignette's own. The app would then copy each selected line,
plus the line before it (usually the question), verbatim under "Risks and safeguarding". Softening is then
impossible by construction; what can fail is missing a disclosure, or missing the line that holds a detail.

Scored automatically against the vignette annotations (no judges), per disclosure:
  found      at least one of its annotated lines is selected
  all_lines  every annotated line is selected (the annotation lists the lines that carry its details)
  terms      every must_keep term occurs in one quoted line (selected lines plus the line before each);
             the ceiling is the same check when exactly the annotated lines are quoted
Per vignette: selected lines outside every disclosure (and how many are within two lines of one, or on a
control's lines). Quoting a denial or hypothetical verbatim keeps its wording, so control lines are reported,
not counted as errors.

Each vignette is run --samples times (temperature 0.2, seeds 42, 43, ...) so run-to-run variation shows.
Same server settings as run.py: Qwen3.5-4B Q4_0, CPU only. Resumes by skipping (vignette, seed) already done.
"""
import argparse
import json
import os
import statistics
import time
import urllib.request

import common as C
from run import Server

MODEL = "qwen3.5-4b"
OUT = C.RESULTS / "extract.jsonl"
CATEGORIES = ["physical_abuse", "sexual_abuse", "domestic_abuse", "financial_abuse", "neglect", "self_harm",
              "child_protection", "substance_use", "other"]
SYSTEM = (
    "You find safeguarding disclosures in a transcript of a professional conversation, for a statutory "
    "record. The transcript is machine-generated from speech. A disclosure is anything a speaker says that "
    "shows harm, abuse, neglect, exploitation, self-harm, or a risk to a child or an adult, including "
    "substance use that affects safety and things the speaker saw or was told. For each disclosure, give its "
    "category and the numbers of ALL the lines where it is said, including the lines that add how often, "
    "how badly, who did it, who saw it and when. The record will quote these lines word for word, so a line "
    "you leave out is lost from the record. List every disclosure in the transcript; if there are none, "
    "return an empty list."
)


def schema(t):
    nums = [C.line_num(l["id"]) for l in t["lines"]]
    item = {"type": "object",
            "properties": {"category": {"type": "string", "enum": CATEGORIES},
                           "lines": {"type": "array", "items": {"type": "integer", "enum": nums},
                                     "minItems": 1, "maxItems": 12}},
            "required": ["category", "lines"]}
    return {"type": "object", "properties": {"disclosures": {"type": "array", "items": item, "maxItems": 12}},
            "required": ["disclosures"]}


def messages(t):
    user = (f"Recorded on {C.recorded_on(t)}.\n\nSpeakers:\n{C.speakers_block(t)}\n\n"
            f"TRANSCRIPT\n{C.render_lines(t['lines'])}")
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}]


def call(url, t, seed, max_tokens):
    body = {"messages": messages(t), "temperature": 0.2, "seed": seed, "max_tokens": max_tokens,
            "cache_prompt": False,
            "response_format": {"type": "json_schema", "json_schema": {"name": "out", "schema": schema(t)}}}
    req = urllib.request.Request(url + "/v1/chat/completions", data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=3600) as r:
        resp = json.loads(r.read())
    choice = resp["choices"][0]
    raw = choice["message"].get("content") or ""
    try:
        parsed, ok = json.loads(raw), True
    except Exception:
        parsed, ok = None, False
    tm = resp.get("timings", {})
    return {"wall_s": round(time.time() - t0, 2), "prompt_n": tm.get("prompt_n"), "predicted_n": tm.get("predicted_n"),
            "finish_reason": choice.get("finish_reason"), "json_ok": ok,
            "disclosures": (parsed or {}).get("disclosures"), "raw": raw}


def load():
    return [json.loads(l) for l in C.read_text(OUT).splitlines() if l.strip()] if OUT.exists() else []


def run(a):
    paths = C.vignette_paths() if a.vignettes == "all" else [C.find_vignette(v) for v in a.vignettes.split(",")]
    seeds = [42 + i for i in range(a.samples)]
    done = {(r["vignette"], r["seed"]) for r in load()}
    todo = [(p, s) for s in seeds for p in paths if (p.stem, s) not in done]
    print(f"{len(todo)} calls to make ({len(done)} done)", flush=True)
    if not todo:
        return
    srv = Server(MODEL, a.threads, a.ctx, a.port, C.RESULTS / "logs")
    try:
        load_avg = [round(x, 1) for x in os.getloadavg()] if hasattr(os, "getloadavg") else None
        print(f"server up in {srv.load_s}s, loadavg {load_avg}", flush=True)
        with open(OUT, "a", encoding="utf-8") as f:
            for p, seed in todo:
                t = C.load_transcript(p)
                r = call(srv.url, t, seed, a.max_tokens)
                f.write(json.dumps({"model": MODEL, "vignette": t["id"], "seed": seed, "threads": a.threads, **r},
                                   ensure_ascii=False) + "\n")
                f.flush()
                print(f"  {t['id']} seed {seed}: {r['wall_s']}s prompt_n={r['prompt_n']} json_ok={r['json_ok']} "
                      f"items={len(r['disclosures'] or [])}", flush=True)
    finally:
        srv.stop()


def quoted(selected):
    """The lines the app would show: each selected line and the line before it."""
    return selected | {n - 1 for n in selected}


def terms_ok(t, line_nums, terms):
    toks = [C.norm_tokens(l["text"]) for l in t["lines"] if C.line_num(l["id"]) in line_nums]
    return all(any(C.has_term(x, term) for x in toks) for term in terms)


def score(t, rec):
    selected = {n for d in (rec["disclosures"] or []) for n in d["lines"]}
    shown = quoted(selected)
    discl = []
    for d in t["disclosures"]:
        lines = {C.line_num(x) for x in d["lines"]}
        discl.append({"id": d["id"], "found": bool(lines & selected), "all_lines": lines <= selected,
                      "terms": terms_ok(t, shown, d["must_keep"]),
                      "ceiling": terms_ok(t, quoted(lines), d["must_keep"]),
                      "missing_lines": sorted(lines - selected)})
    disc_lines = {C.line_num(x) for d in t["disclosures"] for x in d["lines"]}
    ctrl_lines = {C.line_num(x) for c in t["controls"] for x in c["lines"]}
    extra = selected - disc_lines
    near = {n for n in extra if any(abs(n - m) <= 2 for m in disc_lines)}
    return {"vignette": t["id"], "seed": rec["seed"], "json_ok": rec["json_ok"], "n_items": len(rec["disclosures"] or []),
            "n_selected": len(selected), "extra": len(extra), "extra_near": len(near),
            "extra_control": len(extra & ctrl_lines), "disclosures": discl}


def runs(rec):
    """Quote each returned disclosure as one run: its first selected line to its last, plus the line before."""
    return {n for d in (rec["disclosures"] or []) for n in range(min(d["lines"]) - 1, max(d["lines"]) + 1)}


def after_the_fact(recs, tcache):
    """Designs chosen after seeing the results above, so not a pre-registered result: quoting runs instead of
    lines, and showing the quotes beneath the statutory note (a disclosure is kept if the round-r1 judges rated
    the Qwen3.5-4B statutory note accurate on it, or the quotes hold every key term)."""
    accurate = {}
    for v in (json.loads(l) for l in C.read_text(C.RESULTS / "verdicts.jsonl").splitlines() if l.strip()):
        if v["model"] == MODEL and v["prompt"] == "statutory":
            accurate.update({(v["vignette"], did): d["label"] == "accurate" for did, d in v["disclosures"].items()})
    out = ["", "## After the fact (designs chosen after seeing the results above)", "",
           "| Design | Every key term kept | Quoted lines per vignette |", "|---|---:|---:|"]
    designs = {"selected lines + the line before each (as registered)": lambda r: quoted(
                   {n for d in (r["disclosures"] or []) for n in d["lines"]}),
               "each disclosure's run of lines + the line before": runs}
    for name, shown_of in designs.items():
        for with_note in (False, True):
            kept = n = nlines = 0
            for r in recs:
                t = tcache[r["vignette"]]
                shown = {x for x in shown_of(r) if 1 <= x <= len(t["lines"])}
                nlines += len(shown)
                for d in t["disclosures"]:
                    n += 1
                    kept += (with_note and accurate[(t["id"], d["id"])]) or terms_ok(t, shown, d["must_keep"])
            label = f"statutory note + {name}" if with_note else name
            out.append(f"| {label} | {pct(kept, n)} | {nlines / len(recs):.1f} |")
    return out


def pct(k, n):
    return f"{k}/{n} ({100 * k / n:.0f}%)" if n else "–"


def evaluate(_a):
    recs = load()
    if not recs:
        raise SystemExit("no results yet")
    tcache = {p.stem: C.load_transcript(p) for p in C.vignette_paths()}
    rows = [score(tcache[r["vignette"]], r) for r in recs]
    seeds = sorted({r["seed"] for r in rows})
    lines = ["# Extractive safeguarding pass", "", "Generated by `python extract.py eval`.", "",
             f"Model {MODEL}; {len(rows)} calls over {len({r['vignette'] for r in rows})} vignettes, seeds {seeds}. "
             f"JSON valid: {pct(sum(r['json_ok'] for r in rows), len(rows))}. Seconds per call: mean "
             f"{statistics.mean(x['wall_s'] for x in recs):.1f}, max {max(x['wall_s'] for x in recs):.1f}.", "",
             "## Per disclosure", "",
             "| Seed | Disclosures | Found | All annotated lines | must_keep terms in the quotes | Ceiling |",
             "|---|---:|---:|---:|---:|---:|"]
    for s in seeds + ["all"]:
        ds = [d for r in rows if s in ("all", r["seed"]) for d in r["disclosures"]]
        n = len(ds)
        lines.append(f"| {s} | {n} | {pct(sum(d['found'] for d in ds), n)} | {pct(sum(d['all_lines'] for d in ds), n)} "
                     f"| {pct(sum(d['terms'] for d in ds), n)} | {pct(sum(d['ceiling'] for d in ds), n)} |")
    # Disclosures missed in every sample versus in some: is a miss systematic or chance?
    by_d = {}
    for r in rows:
        for d in r["disclosures"]:
            by_d.setdefault((r["vignette"], d["id"]), []).append(d)
    lines += ["", "## Consistency across samples", ""]
    for key, label in (("found", "found"), ("all_lines", "all annotated lines selected"), ("terms", "terms in quotes")):
        always = sum(all(d[key] for d in v) for v in by_d.values())
        never = sum(not any(d[key] for d in v) for v in by_d.values())
        lines.append(f"- **{label}**: in every sample {always}, in none {never}, varies {len(by_d) - always - never} "
                     f"(of {len(by_d)} disclosures)")
    lines += ["", "## Lines selected outside every disclosure", "",
              "| Seed | Lines selected | Outside disclosures | of which within 2 lines of one | on a control's lines |",
              "|---|---:|---:|---:|---:|"]
    for s in seeds + ["all"]:
        rs = [r for r in rows if s in ("all", r["seed"])]
        lines.append(f"| {s} | {sum(r['n_selected'] for r in rs)} | {sum(r['extra'] for r in rs)} | "
                     f"{sum(r['extra_near'] for r in rs)} | {sum(r['extra_control'] for r in rs)} |")
    misses = [(r["vignette"], r["seed"], d["id"], d["missing_lines"]) for r in rows for d in r["disclosures"]
              if not d["all_lines"]]
    lines += ["", "## Disclosures with an annotated line not selected", ""]
    lines += [f"- {v} seed {s} {did}: missing L{', L'.join(map(str, m))}" for v, s, did, m in misses] or ["- none"]
    lines += after_the_fact(recs, tcache)
    text = "\n".join(lines) + "\n"
    C.write_text(C.RESULTS / "extract_summary.md", text)
    print(text)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["run", "eval"])
    ap.add_argument("--vignettes", default="all")
    ap.add_argument("--samples", type=int, default=3)
    ap.add_argument("--threads", type=int, default=6)
    ap.add_argument("--ctx", type=int, default=6144)
    ap.add_argument("--port", type=int, default=8096)
    ap.add_argument("--max-tokens", type=int, default=1024)
    a = ap.parse_args()
    {"run": run, "eval": evaluate}[a.cmd](a)
