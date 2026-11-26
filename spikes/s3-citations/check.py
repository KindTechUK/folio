#!/usr/bin/env python3
"""Second-pass check: the note model re-reads each note sentence against the lines it cites.

  python check.py run  --run s3 --variant prev [--limit N]   -> results/<run>.check-<variant>.jsonl
  python check.py eval --run s3 --variant prev               -> results/<run>.check-<variant>.md

One call per sentence to Qwen3.5-4B (the model that writes the notes, so the product needs one model). The
model sees the recording date, the speakers, the context lines and the sentence, and answers with one
word, supported | partly | not_supported | contradicted, forced by a grammar (no reasoning, to keep calls
short). Not JSON: llama.cpp's JSON grammar allows leading whitespace, and in the smoke run the model spent its
whole token budget on it for 6 of 20 sentences.

Context variants:
  prev    each cited line plus the line before it (usually the question being answered): what S3
          recommends showing the reviewer
  window  each cited line plus three lines either side

Score: p_ok = P(supported), read from the log-probabilities of the first generated token (the four
labels start with different letters). If the server returns no log-probabilities, p_ok is 1 or 0 from the
label. Evaluation reuses flag.py: positives are the judges' contradiction, over_inference and
unsupported_fabrication; the threshold on p_ok is chosen on the tuning set and applied to the test set.

`run` appends to the JSONL and skips sentences already checked with the same text and cites, so it can be
stopped and resumed.
"""
import argparse
import json
import math
import os
import time
import urllib.request

import common as C
import flag as F
import score as S
from run import Server, find_transcript

MODEL = "qwen3.5-4b"
LABELS = ["supported", "partly", "not_supported", "contradicted"]
VARIANTS = {"prev": (1, 0), "window": (3, 3)}  # lines before, lines after each cited line

SYSTEM = (
    "You check one sentence of a professional record against the transcript lines it cites. The transcript "
    "is machine-generated from speech. Lines marked * are the cited lines; the others are context. Answer "
    "with one verdict:\n"
    "- supported: the lines say this (a faithful paraphrase is fine).\n"
    "- partly: part of the sentence is supported, but a detail is not.\n"
    "- not_supported: the lines do not say this: an added detail, an inference, a different speaker, or a "
    "date, year or number that was not said.\n"
    "- contradicted: the lines say something different or the opposite.\n"
    "Answer with the verdict word only."
)
GRAMMAR = "root ::= " + " | ".join(f'"{x}"' for x in LABELS)


def context(t, cites, variant):
    before, after = VARIANTS[variant]
    by_num = {C.line_num(l["id"]): l for l in t["lines"]}
    keep = set()
    for c in cites:
        if c in by_num:
            keep.update(n for n in range(c - before, c + after + 1) if n in by_num)
    cited = set(cites)
    return "\n".join(("* " if n in cited else "  ") + C.render_line(by_num[n]) for n in sorted(keep))


def messages(t, s, variant):
    lines = context(t, s["cites"], variant) or "(the sentence cites no line that exists)"
    user = (f"Recorded on {C.recorded_on(t)}.\n\nSpeakers:\n{C.speakers_block(t)}\n\n"
            f"Transcript lines:\n{lines}\n\nSentence: {s['text']}")
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}]


def label_probs(choice, verdict):
    """P(label) from the log-probabilities of the first generated token, normalised over the four labels
    (the grammar allows only the labels, and they start with different letters)."""
    toks = ((choice.get("logprobs") or {}).get("content")) or []
    mass = {}
    for alt in (toks[0].get("top_logprobs") or []) if toks else []:
        a = alt.get("token", "")
        lab = next((x for x in LABELS if a and x[0] == a[0]), None)
        if lab:
            mass[lab] = mass.get(lab, 0.0) + math.exp(alt["logprob"])
    total = sum(mass.values())
    if total > 0:
        return {x: round(mass.get(x, 0.0) / total, 4) for x in LABELS}, True
    return {x: float(x == verdict) for x in LABELS}, False


def ask(url, msgs):
    body = {"messages": msgs, "temperature": 0, "seed": 42, "max_tokens": 16, "cache_prompt": False,
            "logprobs": True, "top_logprobs": 10, "grammar": GRAMMAR}
    req = urllib.request.Request(url + "/v1/chat/completions", data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=600) as r:
        resp = json.loads(r.read())
    choice = resp["choices"][0]
    content = choice["message"].get("content") or ""
    verdict = content.strip() if content.strip() in LABELS else None
    probs, from_logprobs = label_probs(choice, verdict)
    return {"verdict": verdict, "p": probs, "p_ok": probs["supported"], "from_logprobs": from_logprobs,
            "wall_s": round(time.time() - t0, 2), "prompt_n": resp.get("timings", {}).get("prompt_n"),
            "raw": content}


def out_path(run, variant, ext="jsonl"):
    return C.RESULTS / f"{run}.check-{variant}.{ext}"


def run_checks(a):
    sents = S.load_jsonl(C.RESULTS / f"{a.run}.sentences.jsonl")
    if a.models:
        sents = [s for s in sents if s["model"] in a.models.split(",")]
    out = out_path(a.run, a.variant)
    done = {(r["model"], r["transcript"], r["sid"], r["fp"]) for r in S.load_jsonl(out)}
    todo = [s for s in sents if (s["model"], s["transcript"], s["sid"], S.sentence_fp(s)) not in done]
    if a.limit:
        todo = todo[:a.limit]
    print(f"{len(sents)} sentences, {len(sents) - len(todo) if not a.limit else len(done)} already checked, "
          f"{len(todo)} to check (variant {a.variant})", flush=True)
    if not todo:
        return
    tcache = {}
    srv = Server(MODEL, a.threads, a.ctx, a.port, C.RESULTS / "logs")
    try:
        load = [round(x, 1) for x in os.getloadavg()] if hasattr(os, "getloadavg") else None
        print(f"server up in {srv.load_s}s, loadavg {load}", flush=True)
        with open(out, "a", encoding="utf-8") as f:
            for i, s in enumerate(todo, 1):
                if s["transcript"] not in tcache:
                    tcache[s["transcript"]] = C.load_transcript(find_transcript(s["transcript"]))
                r = ask(srv.url, messages(tcache[s["transcript"]], s, a.variant))
                f.write(json.dumps({"run": a.run, "variant": a.variant, "checker": MODEL, "threads": a.threads,
                                    "model": s["model"], "transcript": s["transcript"], "sid": s["sid"],
                                    "fp": S.sentence_fp(s), **r}) + "\n")
                f.flush()
                if i % 25 == 0 or i == len(todo):
                    print(f"  {i}/{len(todo)}  last: {r['verdict']} p_ok={r['p_ok']} {r['wall_s']}s", flush=True)
    finally:
        srv.stop()


def evaluate(a):
    verdicts, _, missing = S.load_verdicts(a.run)
    if missing:
        print(f"warning: {len(missing)} packets have no verdict yet")
    sents = {(s["model"], s["transcript"], s["sid"]): s for s in S.load_jsonl(C.RESULTS / f"{a.run}.sentences.jsonl")}
    checks = {(r["model"], r["transcript"], r["sid"]): r for r in S.load_jsonl(out_path(a.run, a.variant))}
    rows, absent = [], []
    for v in verdicts:
        k = (v["model"], v["transcript"], v["sid"])
        c = checks.get(k)
        if c is None or k not in sents or c["fp"] != S.sentence_fp(sents[k]):
            absent.append(k)
            continue
        rows.append({**v, "p_ok": c["p_ok"], "check": c["verdict"], "wall_s": c["wall_s"],
                     "from_logprobs": c["from_logprobs"], "pos": v["label"] in S.POSITIVE})
    lines = [f"# Second-pass check, variant `{a.variant}`, run `{a.run}`", "",
             f"Generated by `python check.py eval --run {a.run} --variant {a.variant}`.", ""]
    if absent:
        lines.append(f"**Incomplete:** {len(absent)} judged sentences have no current check (e.g. {absent[0]}). "
                     "Numbers below cover the checked sentences only.\n")
    tune = [r for r in rows if r["split"] == "tuning"]
    test = [r for r in rows if r["split"] == "test"]
    walls = [r["wall_s"] for r in rows]
    lines += [f"Checked: tuning {len(tune)} ({sum(r['pos'] for r in tune)} positive), test {len(test)} "
              f"({sum(r['pos'] for r in test)} positive). Probabilities from log-probs for "
              f"{sum(r['from_logprobs'] for r in rows)} of {len(rows)}.", ""]
    if walls:
        lines.append(f"Seconds per sentence: mean {sum(walls) / len(walls):.2f}, "
                     f"max {max(walls):.2f} (threads and machine load in the JSONL).\n")

    lines += ["## Checker label against judge label", "",
              "| judge \\ checker | " + " | ".join(LABELS) + " |", "|---|" + "---:|" * len(LABELS)]
    for jl in ["supported"] + sorted(S.POSITIVE):
        lines.append(f"| {jl} | " + " | ".join(str(sum(1 for r in rows if r["label"] == jl and r["check"] == cl))
                                              for cl in LABELS) + " |")

    lines += ["", "## Flags (positives = contradiction, over_inference, unsupported_fabrication)", ""]
    rules = {
        "label: not_supported or contradicted": lambda r: r["check"] in ("not_supported", "contradicted"),
        "label: anything but supported": lambda r: r["check"] != "supported",
    }
    for name, rule in rules.items():
        for split, part in (("tuning", tune), ("test", test)):
            if part:
                lines.append(f"- **{name}**, {split}: {F.fmt(F.metrics(part, [rule(r) for r in part]))}")
    if tune and test:
        F.FLAGS["checker"] = lambda r, t: r["p_ok"] < t
        best_f1, catch = F.choose(tune, "checker")
        for rule, pick in (("best F1", best_f1), ("catch-all", catch)):
            if pick is None:
                lines.append(f"- **p_ok < t**, {rule}: no threshold leaves zero unflagged on the tuning set")
                continue
            t, m_tune = pick
            m_test = F.metrics(test, [F.FLAGS["checker"](r, t) for r in test])
            lines.append(f"- **p_ok < t**, {rule}, t = {t}\n  - tuning: {F.fmt(m_tune)}\n  - test:   {F.fmt(m_test)}")
    text = "\n".join(lines) + "\n"
    C.write_text(out_path(a.run, a.variant, "md"), text)
    print(text)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["run", "eval"])
    ap.add_argument("--run", required=True, help="the notes run whose sentences to check (s3, v2)")
    ap.add_argument("--variant", default="prev", choices=sorted(VARIANTS))
    ap.add_argument("--models", help="only check sentences from these note models (comma list)")
    ap.add_argument("--limit", type=int, help="check at most N more sentences (smoke runs)")
    ap.add_argument("--threads", type=int, default=6)
    ap.add_argument("--ctx", type=int, default=4096)
    ap.add_argument("--port", type=int, default=8095)
    a = ap.parse_args()
    {"run": run_checks, "eval": evaluate}[a.cmd](a)
