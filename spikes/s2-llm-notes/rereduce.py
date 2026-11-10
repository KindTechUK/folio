#!/usr/bin/env python3
"""Re-run only the reduce step from saved map facts (no re-mapping), using reduce v2.

  python rereduce.py --from screen --run screen-r2 --models qwen3-4b,gemma4-e4b

Writes results/<run>.jsonl records with strategy "mapreduce-v2". Its after-stop times reuse the
original map timings from results/<from>.jsonl: (b) = all maps + new reduce, (c) = last map + new reduce.
"""
import argparse
import json

import common as C
from run import Server, find_transcript


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--from", dest="src", required=True)
    ap.add_argument("--run", required=True)
    ap.add_argument("--models", required=True)
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--ctx", type=int, default=32768)
    ap.add_argument("--port", type=int, default=8091)
    a = ap.parse_args()
    src = [json.loads(l) for l in (C.RESULTS / f"{a.src}.jsonl").read_text().splitlines()]
    meta = {"run": a.run, "threads": a.threads, "ctx": a.ctx}
    with open(C.RESULTS / f"{a.run}.jsonl", "a") as out:
        for model in a.models.split(","):
            srv = Server(model, a.threads, a.ctx, a.port, C.RESULTS / "logs")
            try:
                for p in sorted((C.RESULTS / "notes" / a.src / model).glob("*.mapreduce.json")):
                    tid = p.name.split(".")[0]
                    t = C.load_transcript(find_transcript(tid))
                    tpl = C.load_template(t["template"])
                    facts = json.loads(p.read_text())["facts"]
                    maps = [r["wall_s"] for r in src if r.get("model") == model and r.get("transcript") == tid
                            and r.get("stage", "").startswith("map")]
                    m, note, raw = srv.chat(C.reduce_messages_v2(t, tpl, facts), C.note_schema_v2(tpl), 6144)
                    base = {**meta, "model": model, "transcript": tid, "strategy": "mapreduce-v2",
                            "minutes": round(C.duration_min(t["lines"]), 1), "n_lines": len(t["lines"])}
                    out.write(json.dumps({**base, "stage": "reduce", "n_facts": len(facts), **m}) + "\n")
                    out.write(json.dumps({**base, "stage": "summary", "after_stop_s": round(sum(maps) + m["wall_s"], 1),
                                          "incremental_after_stop_s": round(maps[-1] + m["wall_s"], 1),
                                          "wall_s": m["wall_s"], "peak_rss_mb": m["peak_rss_mb"],
                                          "json_ok": m["json_ok"]}) + "\n")
                    out.flush()
                    d = C.RESULTS / "notes" / a.run / model
                    d.mkdir(parents=True, exist_ok=True)
                    (d / f"{tid}.mapreduce-v2.json").write_text(
                        json.dumps({"note": note if note else {"raw": raw}, "facts": facts}, indent=1))
                    print(f"{model} {tid} reduce-v2: wall_s={m['wall_s']} prompt_n={m['prompt_n']} "
                          f"gen={m['predicted_n']} json_ok={m['json_ok']} finish={m['finish_reason']}", flush=True)
            finally:
                srv.stop()


if __name__ == "__main__":
    main()
