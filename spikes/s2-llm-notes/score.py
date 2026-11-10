#!/usr/bin/env python3
"""Score S2 notes.

  python score.py auto   --run R   automatic metrics per note -> results/<R>.scores.jsonl
  python score.py packets --run R  blind judge packets for synthetic transcripts -> results/judge/<R>/
  python score.py collect --run R  merge judge verdicts (results/judge/<R>/*.verdict.json) into scores

Automatic metrics (all transcripts):
  n_sentences, invalid_cite_rate (cited line doesn't exist), support (content-word overlap between a
  sentence and its cited lines, averaged), weak_rate (share of sentences with support < 0.25, a cheap
  "possibly unsupported" flag that S3 will refine).
Judge metrics (synthetic transcripts only; blind, via packets):
  fact_recall (present = 1, partial = 0.5), trap_violations, other_fabrications.
"""
import argparse
import hashlib
import json
import re

import common as C
from run import find_transcript

STOP = set("""a an the and or but if then so of to in on at by for with from as is are was were be been
being it its this that these those he she they we you i me my her his their our your them us him
not no yes do does did done have has had will would can could should may might must there here
what which who whom when where why how all any some very just also about into over under than too
um uh er erm like yeah okay ok oh well really right mean know think going get got one two""".split())


def content_words(text):
    out = set()
    for w in re.findall(r"[a-z0-9£]+", text.lower()):
        if len(w) < 3 or w in STOP:
            continue
        for suf in ("ing", "ed", "es", "s"):
            if w.endswith(suf) and len(w) - len(suf) >= 3:
                w = w[: -len(suf)]
                break
        out.add(w)
    return out


def note_sentences(note):
    for sec in (note or {}).get("sections", []):
        for s in sec.get("sentences", []):
            yield sec.get("title", ""), s


def load_note(run, model, tid, strategy):
    p = C.RESULTS / "notes" / run / model / f"{tid}.{strategy}.json"
    if not p.exists():
        return None
    d = json.loads(p.read_text())
    return d.get("note", d) if strategy == "mapreduce" else d


def notes_in_run(run):
    base = C.RESULTS / "notes" / run
    for p in sorted(base.glob("*/*.json")):
        model = p.parent.name
        tid, strategy = p.stem.rsplit(".", 1)
        yield model, tid, strategy


def auto(run):
    out = []
    for model, tid, strategy in notes_in_run(run):
        t = C.load_transcript(find_transcript(tid))
        by_num = {C.line_num(l["id"]): l["text"] for l in t["lines"]}
        note = load_note(run, model, tid, strategy)
        sents = list(note_sentences(note))
        n, invalid, total_cites, supports = len(sents), 0, 0, []
        for _, s in sents:
            cites = s.get("cites", [])
            total_cites += len(cites)
            invalid += sum(1 for c in cites if c not in by_num)
            sw = content_words(s.get("text", ""))
            cw = set().union(*(content_words(by_num[c]) for c in cites if c in by_num)) if cites else set()
            supports.append(len(sw & cw) / len(sw) if sw else 1.0)
        rec = {
            "run": run, "model": model, "transcript": tid, "strategy": strategy,
            "parsed": bool(note and "sections" in note), "n_sentences": n,
            "invalid_cite_rate": round(invalid / total_cites, 3) if total_cites else None,
            "support": round(sum(supports) / n, 3) if n else None,
            "weak_rate": round(sum(1 for x in supports if x < 0.25) / n, 3) if n else None,
        }
        out.append(rec)
        print(rec)
    (C.RESULTS / f"{run}.scores.jsonl").write_text("".join(json.dumps(r) + "\n" for r in out))


def packet_id(model, tid, strategy):
    return hashlib.sha1(f"{model}|{tid}|{strategy}".encode()).hexdigest()[:8]


def packets(run):
    jdir = C.RESULTS / "judge" / run
    jdir.mkdir(parents=True, exist_ok=True)
    key = {}
    for model, tid, strategy in notes_in_run(run):
        tpath = find_transcript(tid)
        t = C.load_transcript(tpath)
        if not t.get("facts"):
            continue
        pid = packet_id(model, tid, strategy)
        key[pid] = {"model": model, "transcript": tid, "strategy": strategy}
        note = load_note(run, model, tid, strategy)
        note_md = "\n".join(
            f"- [{title}] {s.get('text', '')}  (cites: {', '.join(map(str, s.get('cites', [])))})"
            for title, s in note_sentences(note)) or "(no parseable note)"
        facts = "\n".join(f"- {f['id']}: {f['fact']}" for f in t["facts"])
        traps = "\n".join(f"- {x['id']} ({x['kind']}): must NOT claim: {x['must_not_claim']}"
                          for x in t["traps"])
        (jdir / f"{pid}.md").write_text(
            f"# Judge packet {pid}\n\nTranscript file: {tpath}\n(line n in cites = id L000n)\n\n"
            f"## Note under review\n{note_md}\n\n## Facts checklist\n{facts}\n\n## Traps\n{traps}\n")
    (jdir / "_key.json").write_text(json.dumps(key, indent=1))
    print(f"{len(key)} packets in {jdir}")


def collect(run):
    jdir = C.RESULTS / "judge" / run
    key = json.loads((jdir / "_key.json").read_text())
    scores = [json.loads(l) for l in (C.RESULTS / f"{run}.scores.jsonl").read_text().splitlines()]
    idx = {(s["model"], s["transcript"], s["strategy"]): s for s in scores}
    for pid, k in key.items():
        vp = jdir / f"{pid}.verdict.json"
        if not vp.exists():
            continue
        v = json.loads(vp.read_text())
        fv = v["facts"].values()
        s = idx[(k["model"], k["transcript"], k["strategy"])]
        s["fact_recall"] = round(sum({"present": 1, "partial": 0.5}.get(x, 0) for x in fv) / len(fv), 3)
        s["trap_violations"] = sorted(t for t, x in v["traps"].items() if x == "violated")
        s["other_fabrications"] = len(v.get("other_fabrications", []))
    (C.RESULTS / f"{run}.scores.jsonl").write_text("".join(json.dumps(s) + "\n" for s in scores))
    print(f"merged verdicts into {run}.scores.jsonl")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["auto", "packets", "collect"])
    ap.add_argument("--run", required=True)
    a = ap.parse_args()
    {"auto": auto, "packets": packets, "collect": collect}[a.cmd](a.run)
