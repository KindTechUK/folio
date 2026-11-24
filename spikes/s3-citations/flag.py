#!/usr/bin/env python3
"""The cheap unsupported-sentence flag: score it against the blind judges' labels.

  python flag.py nli  --run R    NLI scores for every note sentence -> results/R.nli.jsonl
                                 (needs `transformers torch sentencepiece`; downloads the model once)
  python flag.py eval --run R    after `score.py collect`: tune thresholds on the tuning set, report on the test set

Flags (a sentence is flagged as possibly unsupported):
  lexical   support < t, where support is score.py's content-word overlap with the valid cited lines
  nli       P(entailment) < t from cross-encoder/nli-deberta-v3-small,
            premise = the valid cited lines joined ("<speaker label>: <text>" each), hypothesis = the sentence
  combined  lexical < t1 OR nli < t2
A sentence with no valid citation scores 0 on both, so every flag flags it.

Positive class = judge label in {unsupported_fabrication, contradiction, over_inference}.
Tuning set = the two S2 transcripts; test set = the four S3 transcripts (run.TUNING / run.TEST).
Thresholds are chosen on the tuning set two ways: best F1, and "catch-all" (the highest precision that leaves no
fabrication or contradiction unflagged). Both are then applied unchanged to the test set.
"""
import argparse
import json

import common as C
import score as S
from run import find_transcript

NLI_MODEL = "cross-encoder/nli-deberta-v3-small"
GRID = [0.01, 0.02, 0.03] + [round(x * 0.05, 2) for x in range(1, 21)]  # NLI probabilities cluster near 0 and 1
SEVERE = {"unsupported_fabrication", "contradiction"}


def nli(run):
    import torch
    import transformers
    from transformers import AutoModelForSequenceClassification, AutoTokenizer

    tok = AutoTokenizer.from_pretrained(NLI_MODEL)
    model = AutoModelForSequenceClassification.from_pretrained(NLI_MODEL).eval()
    labels = {i: l.lower() for i, l in model.config.id2label.items()}
    sents = S.load_jsonl(C.RESULTS / f"{run}.sentences.jsonl")
    tcache = {}
    pairs, out = [], []
    for s in sents:
        if s["transcript"] not in tcache:
            t = C.load_transcript(find_transcript(s["transcript"]))
            tcache[s["transcript"]] = ({C.line_num(l["id"]): l for l in t["lines"]}, t["speakers"])
        by_num, spk = tcache[s["transcript"]]
        valid = [c for c in s["cites"] if c in by_num]
        premise = " ".join(f"{spk.get(by_num[c]['speaker'], by_num[c]['speaker'])}: {by_num[c]['text']}"
                           for c in valid)
        rec = {"model": s["model"], "transcript": s["transcript"], "sid": s["sid"], "premise_chars": len(premise)}
        out.append(rec)
        if premise:
            pairs.append((rec, premise, s["text"]))
        else:
            rec.update({"p_entailment": 0.0, "p_neutral": None, "p_contradiction": None, "no_premise": True})
    with torch.no_grad():
        for i in range(0, len(pairs), 16):
            batch = pairs[i:i + 16]
            enc = tok([p for _, p, _ in batch], [h for _, _, h in batch], truncation="only_first",
                      max_length=512, padding=True, return_tensors="pt")
            probs = torch.softmax(model(**enc).logits, dim=-1).tolist()
            for (rec, _, _), pr in zip(batch, probs):
                for j, p in enumerate(pr):
                    rec[f"p_{labels[j]}"] = round(p, 4)
            print(f"  {min(i + 16, len(pairs))}/{len(pairs)}", end="\r", flush=True)
    meta = {"nli_model": NLI_MODEL, "transformers": transformers.__version__, "torch": torch.__version__}
    (C.RESULTS / f"{run}.nli.jsonl").write_text("".join(json.dumps({**r, **meta}) + "\n" for r in out))
    print(f"\n{len(out)} sentences ({len(pairs)} with a premise) -> {run}.nli.jsonl")


def metrics(rows, flagged):
    tp = sum(1 for r, f in zip(rows, flagged) if f and r["pos"])
    fp = sum(1 for r, f in zip(rows, flagged) if f and not r["pos"])
    fn = sum(1 for r, f in zip(rows, flagged) if not f and r["pos"])
    p = tp / (tp + fp) if tp + fp else 0.0
    rcl = tp / (tp + fn) if tp + fn else 0.0
    return {"flagged": tp + fp, "tp": tp, "fp": fp, "fn": fn, "precision": round(p, 3), "recall": round(rcl, 3),
            "f1": round(2 * p * rcl / (p + rcl), 3) if p + rcl else 0.0,
            "unflagged_severe": sum(1 for r, f in zip(rows, flagged) if not f and r["label"] in SEVERE)}


FLAGS = {
    "lexical": lambda r, t: r["support"] < t,
    "nli": lambda r, t: r["p_entailment"] < t,
    "combined": lambda r, t: r["support"] < t[0] or r["p_entailment"] < t[1],
}


def grid(name):
    return [(a, b) for a in GRID for b in GRID] if name == "combined" else GRID


def choose(rows, name):
    scored = [(t, metrics(rows, [FLAGS[name](r, t) for r in rows])) for t in grid(name)]
    best_f1 = max(scored, key=lambda x: (x[1]["f1"], x[1]["recall"]))
    catch = [x for x in scored if x[1]["unflagged_severe"] == 0]
    best_catch = max(catch, key=lambda x: (x[1]["precision"], x[1]["f1"])) if catch else None
    return best_f1, best_catch


def fmt(m):
    return (f"P {m['precision']:.2f}  R {m['recall']:.2f}  F1 {m['f1']:.2f}  flagged {m['flagged']}  "
            f"(tp {m['tp']}, fp {m['fp']}, fn {m['fn']})  unflagged fab/contra {m['unflagged_severe']}")


def evaluate(run):
    verdicts, _, missing = S.load_verdicts(run)
    if missing:
        print(f"warning: {len(missing)} packets have no verdict yet")
    sents = {(s["model"], s["transcript"], s["sid"]): s for s in S.load_jsonl(C.RESULTS / f"{run}.sentences.jsonl")}
    nli_s = {(s["model"], s["transcript"], s["sid"]): s for s in S.load_jsonl(C.RESULTS / f"{run}.nli.jsonl")}
    rows = []
    for v in verdicts:
        k = (v["model"], v["transcript"], v["sid"])
        if k not in sents or k not in nli_s:
            print(f"  skip {k}: no automatic or NLI score")
            continue
        rows.append({**v, "support": sents[k]["support"], "p_entailment": nli_s[k]["p_entailment"],
                     "pos": v["label"] in S.POSITIVE})
    tune = [r for r in rows if r["split"] == "tuning"]
    test = [r for r in rows if r["split"] == "test"]
    print(f"judged sentences: tuning {len(tune)} ({sum(r['pos'] for r in tune)} positive), "
          f"test {len(test)} ({sum(r['pos'] for r in test)} positive)\n")

    print("## Citation validity (judge: cited lines alone support the sentence)\n")
    print("| set | model | sentences | cites_support yes | yes or partly |\n|---|---|---:|---:|---:|")
    for name, part in (("tuning", tune), ("test", test)):
        for model in sorted({r["model"] for r in part}) + ["all"]:
            rs = [r for r in part if model in ("all", r["model"])]
            if rs:
                y = sum(r["cites_support"] == "yes" for r in rs) / len(rs)
                yp = sum(r["cites_support"] in ("yes", "partly") for r in rs) / len(rs)
                print(f"| {name} | {model} | {len(rs)} | {y:.3f} | {yp:.3f} |")

    print("\n## Labels\n")
    for name, part in (("tuning", tune), ("test", test)):
        counts = {}
        for r in part:
            counts[r["label"]] = counts.get(r["label"], 0) + 1
        print(f"- {name}: {counts}")

    if not tune or not test:
        print("\nNeed judged sentences in both the tuning and the test set to evaluate the flag.")
        return
    print("\n## Flag: thresholds chosen on the tuning set, applied to the test set\n")
    for name in FLAGS:
        best_f1, catch = choose(tune, name)
        for rule, pick in (("best F1", best_f1), ("catch-all", catch)):
            if pick is None:
                print(f"- **{name}**, {rule}: no threshold on the grid leaves zero unflagged on the tuning set")
                continue
            t, m_tune = pick
            m_test = metrics(test, [FLAGS[name](r, t) for r in test])
            print(f"- **{name}**, {rule}, t = {t}\n  - tuning: {fmt(m_tune)}\n  - test:   {fmt(m_test)}")
    print("\n## Sweep on the test set (single-signal flags)\n")
    print("| t | lexical P | lexical R | lexical unflagged fab/contra | NLI P | NLI R | NLI unflagged fab/contra |")
    print("|---:|---:|---:|---:|---:|---:|---:|")
    for t in GRID:
        a = metrics(test, [FLAGS["lexical"](r, t) for r in test])
        b = metrics(test, [FLAGS["nli"](r, t) for r in test])
        print(f"| {t} | {a['precision']:.2f} | {a['recall']:.2f} | {a['unflagged_severe']} | "
              f"{b['precision']:.2f} | {b['recall']:.2f} | {b['unflagged_severe']} |")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["nli", "eval"])
    ap.add_argument("--run", required=True)
    a = ap.parse_args()
    {"nli": nli, "eval": evaluate}[a.cmd](a.run)
