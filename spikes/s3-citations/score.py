#!/usr/bin/env python3
"""Score S3 notes (sentence level).

  python score.py auto    --run R  automatic metrics -> results/R.sentences.jsonl (one row per note sentence)
                                   and results/R.scores.jsonl (one row per note)
  python score.py packets --run R  blind sentence-level judge packets -> results/judge/R/
  python score.py collect --run R  merge results/judge/R/*.verdict.json -> results/R.verdicts.jsonl and
                                   judge metrics added to results/R.scores.jsonl

Automatic metrics per sentence:
  cites, invalid_cites (line doesn't exist), out_of_part_cites (line exists but was not in the transcript part
  the model was given for that map call, so it cannot be a real citation), support (lexical: share of the
  sentence's content words found in its valid cited lines; S2's measure, 0 when nothing valid is cited).
"""
import argparse
import hashlib
import json
import math
import re

import common as C
from run import find_transcript, split_of

STOP = set("""a an the and or but if then so of to in on at by for with from as is are was were be been
being it its this that these those he she they we you i me my her his their our your them us him
not no yes do does did done have has had will would can could should may might must there here
what which who whom when where why how all any some very just also about into over under than too
um uh er erm like yeah okay ok oh well really right mean know think going get got one two""".split())

STRATEGY = "sectioned"
PART_MAX = 120  # sentences per judge packet before a note is split into parts
POSITIVE = {"unsupported_fabrication", "contradiction", "over_inference"}


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


def lexical_support(text, cited_texts):
    sw = content_words(text)
    cw = set().union(*(content_words(x) for x in cited_texts)) if cited_texts else set()
    return round(len(sw & cw) / len(sw), 3) if sw else 1.0


def load_jsonl(p):
    return [json.loads(l) for l in p.read_text().splitlines() if l.strip()] if p.exists() else []


def notes_in_run(run):
    for p in sorted((C.RESULTS / "notes" / run).glob(f"*/*.{STRATEGY}.json")):
        yield p.parent.name, p.name.removesuffix(f".{STRATEGY}.json"), p


def map_records(run):
    """(model, transcript) -> that note's map-call records from the run JSONL, in call order."""
    out = {}
    for r in load_jsonl(C.RESULTS / f"{run}.jsonl"):
        if r.get("strategy") == STRATEGY and r.get("stage", "").startswith("map"):
            out.setdefault((r["model"], r["transcript"]), []).append(r)
    return out


def sentence_parts(tpl, recs, sents):
    """sid -> (first, last) line number of the transcript part whose map call produced the sentence.

    Rebuilt by tagging each map call's facts and re-running the sectioned grouping; empty if the rebuilt
    note doesn't match the saved one."""
    facts = []
    for r in recs:
        parsed, _ = C.salvage_json(r.get("raw", ""), "facts")
        span = (C.line_num(r["section_lines"][0]), C.line_num(r["section_lines"][1]))
        facts += [{**f, "_span": span} for f in (parsed or {}).get("facts", [])]
    ordered = [f for sec in tpl["sections"] for f in facts if f.get("section") == sec["title"]]
    if [f.get("text") for f in ordered] != [s["text"] for s in sents]:
        return {}
    return {s["sid"]: f["_span"] for s, f in zip(sents, ordered)}


def auto(run):
    maps = map_records(run)
    sent_rows, note_rows = [], []
    for model, tid, path in notes_in_run(run):
        t = C.load_transcript(find_transcript(tid))
        tpl = C.load_template(t["template"])
        by_num = {C.line_num(l["id"]): l for l in t["lines"]}
        note = json.loads(path.read_text())
        sents = C.note_sentence_list(note)

        part_of = sentence_parts(tpl, maps.get((model, t["id"]), []), sents)
        for s in sents:
            valid = [c for c in s["cites"] if c in by_num]
            lo, hi = part_of.get(s["sid"], (None, None))
            sent_rows.append({
                "run": run, "model": model, "transcript": t["id"], "split": split_of(t["id"]),
                "sid": s["sid"], "section": s["section"], "text": s["text"], "cites": s["cites"],
                "invalid_cites": [c for c in s["cites"] if c not in by_num],
                "out_of_part_cites": [c for c in valid if lo is not None and not lo <= c <= hi],
                "part_lines": [lo, hi] if lo is not None else None,
                "support": lexical_support(s["text"], [by_num[c]["text"] for c in valid]),
            })
        rows = [r for r in sent_rows if r["model"] == model and r["transcript"] == t["id"]]
        n = len(rows)
        n_cites = sum(len(r["cites"]) for r in rows)
        sup = sorted(r["support"] for r in rows)
        note_rows.append({
            "run": run, "model": model, "transcript": t["id"], "split": split_of(t["id"]),
            "n_sentences": n, "n_cites": n_cites,
            "invalid_cites": sum(len(r["invalid_cites"]) for r in rows),
            "invalid_cite_rate": round(sum(len(r["invalid_cites"]) for r in rows) / n_cites, 4) if n_cites else None,
            "out_of_part_cites": sum(len(r["out_of_part_cites"]) for r in rows),
            "parts_mapped": bool(part_of),
            "sent_with_cite": sum(1 for r in rows if r["cites"]),
            "sent_with_valid_cite": sum(1 for r in rows if len(r["invalid_cites"]) < len(r["cites"])),
            "support_mean": round(sum(sup) / n, 3) if n else None,
            "support_median": sup[n // 2] if n else None,
            "weak_rate": round(sum(1 for x in sup if x < 0.25) / n, 3) if n else None,
        })
        print({k: v for k, v in note_rows[-1].items() if k != "run"})
    (C.RESULTS / f"{run}.sentences.jsonl").write_text("".join(json.dumps(r) + "\n" for r in sent_rows))
    (C.RESULTS / f"{run}.scores.jsonl").write_text("".join(json.dumps(r) + "\n" for r in note_rows))


# ---- blind packets ----

def note_hash(model, tid):
    return hashlib.sha1(f"s3|{model}|{tid}|{STRATEGY}".encode()).hexdigest()[:8]


def transcript_txt(t):
    head = f"# {t['title']}\n# id: {t['id']}\n# Speakers: " + "; ".join(f"{k} = {v}" for k, v in t["speakers"].items())
    return head + "\n\n" + C.render_lines(t["lines"]) + "\n"


def packets(run):
    jdir = C.RESULTS / "judge" / run
    (jdir / "transcripts").mkdir(parents=True, exist_ok=True)
    key, sizes = {}, []
    for model, tid, path in notes_in_run(run):
        tpath = find_transcript(tid)
        t = C.load_transcript(tpath)
        tpl = C.load_template(t["template"])
        txt = jdir / "transcripts" / f"{t['id']}.txt"
        txt.write_text(transcript_txt(t))
        by_num = {C.line_num(l["id"]): l for l in t["lines"]}
        sents = C.note_sentence_list(json.loads(path.read_text()))
        n_parts = max(1, math.ceil(len(sents) / PART_MAX))
        size = math.ceil(len(sents) / n_parts) if sents else 0
        h = note_hash(model, tid)
        for i in range(n_parts):
            pid = h if n_parts == 1 else f"{h}-{i + 1}"
            chunk = sents[i * size:(i + 1) * size]
            key[pid] = {"model": model, "transcript": t["id"], "strategy": STRATEGY, "part": i + 1,
                        "of": n_parts, "sids": [s["sid"] for s in chunk], "judges_facts": i == 0}
            sizes.append((pid, t["id"], len(chunk), i == 0))
            blocks = []
            for s in chunk:
                quoted = [f"    {C.render_line(by_num[c])}" if c in by_num else f"    L{c}: (no such line)"
                          for c in s["cites"]] or ["    (no citations)"]
                blocks.append(f"### {s['sid']} · {s['section']}\n{s['text']}\n\nCited lines:\n" + "\n".join(quoted))
            part_note = f" (part {i + 1} of {n_parts}: sentences {chunk[0]['sid']}–{chunk[-1]['sid']})" if n_parts > 1 else ""
            md = [f"# Judge packet {pid}{part_note}\n",
                  f"Full transcript (read it all): `{txt}`  ",
                  f"Same transcript as JSON: `{tpath}`\n",
                  f"Note type: {tpl['name']}. Speakers: " + "; ".join(f"{k} = {v}" for k, v in t["speakers"].items()),
                  "\nLine references: `L52` in a note's cites = line `L52` in the transcript file "
                  "(id `L0052` in the JSON). Times are [mm:ss] from the start.\n",
                  f"## Sentences to judge ({len(chunk)})\n", "\n\n".join(blocks) or "(the note has no sentences)"]
            if i == 0:
                if n_parts > 1:
                    md += ["\n\n## Whole note, for the facts checklist only (do not label these here)\n",
                           "\n".join(f"- {s['sid']} [{s['section']}] {s['text']}" for s in sents)]
                md += ["\n\n## Facts checklist (judge each: present | partial | absent)\n",
                       "\n".join(f"- {f['id']}: {f['fact']}" for f in t["facts"])]
            else:
                md += ["\n\n## Facts checklist\nNot judged in this part (part 1 judges the facts). "
                       "Write `\"facts\": {}`."]
            md += ["\n\n## Traps (a sentence that makes one of these claims, even implicitly, hits the trap)\n",
                   "\n".join(f"- {x['id']} ({x['kind']}): must NOT claim: {x['must_not_claim']}" for x in t["traps"]),
                   f"\n\nWrite your verdict to `{jdir / (pid + '.verdict.json')}` following "
                   f"`{C.HERE / 'judge_instructions_s3.md'}`.\n"]
            (jdir / f"{pid}.md").write_text("\n".join(md))
    (jdir / "_key.json").write_text(json.dumps(key, indent=1))
    print(f"{len(key)} packets in {jdir}")
    for pid, tid, n, facts in sorted(sizes, key=lambda x: (x[1], x[0])):
        print(f"  {pid}  {tid:32s} {n:4d} sentences{'  + facts' if facts else ''}")


# ---- verdicts ----

def load_verdicts(run):
    """One row per judged sentence: {model, transcript, split, sid, label, cites_support, trap, why}."""
    jdir = C.RESULTS / "judge" / run
    key = json.loads((jdir / "_key.json").read_text())
    rows, facts, missing = [], {}, []
    for pid, k in key.items():
        vp = jdir / f"{pid}.verdict.json"
        if not vp.exists():
            missing.append(pid)
            continue
        v = json.loads(vp.read_text())
        for sid in k["sids"]:
            sv = v.get("sentences", {}).get(sid)
            if sv is None:
                print(f"  {pid}: no verdict for {sid}")
                continue
            rows.append({"model": k["model"], "transcript": k["transcript"], "split": split_of(k["transcript"]),
                         "sid": sid, "label": sv.get("label"), "cites_support": sv.get("cites_support"),
                         "trap": sv.get("trap"), "why": sv.get("why", ""), "packet": pid})
        if k["judges_facts"]:
            facts[(k["model"], k["transcript"])] = v.get("facts", {})
    return rows, facts, missing


def collect(run):
    rows, facts, missing = load_verdicts(run)
    if missing:
        print(f"{len(missing)} packets have no verdict yet: {', '.join(missing)}")
    (C.RESULTS / f"{run}.verdicts.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows))
    scores = load_jsonl(C.RESULTS / f"{run}.scores.jsonl")
    for s in scores:
        k = (s["model"], s["transcript"])
        vr = [r for r in rows if (r["model"], r["transcript"]) == k]
        if not vr:
            continue
        n = len(vr)
        s["judged"] = n
        for lab in ["supported"] + sorted(POSITIVE):
            s[lab] = sum(1 for r in vr if r["label"] == lab)
        s["cites_yes"] = round(sum(1 for r in vr if r["cites_support"] == "yes") / n, 3)
        s["cites_yes_or_partly"] = round(sum(1 for r in vr if r["cites_support"] in ("yes", "partly")) / n, 3)
        s["traps_hit"] = sorted({r["trap"] for r in vr if r["trap"]})
        fv = facts.get(k)
        if fv:
            t = C.load_transcript(find_transcript(s["transcript"]))
            omit = {f["id"] for f in t["facts"] if f.get("omission")}
            val = {"present": 1, "partial": 0.5}
            s["fact_recall"] = round(sum(val.get(x, 0) for x in fv.values()) / len(fv), 3)
            if omit:
                s["omission_recall"] = round(sum(val.get(fv.get(f), 0) for f in omit) / len(omit), 3)
    (C.RESULTS / f"{run}.scores.jsonl").write_text("".join(json.dumps(s) + "\n" for s in scores))
    print(f"{len(rows)} sentence verdicts -> {run}.verdicts.jsonl; judge metrics merged into {run}.scores.jsonl")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["auto", "packets", "collect"])
    ap.add_argument("--run", required=True)
    a = ap.parse_args()
    {"auto": auto, "packets": packets, "collect": collect}[a.cmd](a.run)
