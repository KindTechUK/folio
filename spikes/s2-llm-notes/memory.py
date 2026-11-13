#!/usr/bin/env python3
"""Peak memory of one section-note (map) call, at a given context size.

  python memory.py qwen3.5-4b 8192   -> appends a row to results/memory.jsonl

run.py's peak_rss_mb comes from `ps` RSS, which on macOS counts memory-mapped weights and moves with
memory compression, so it is not a reliable peak (see s1-live-lag.md). This instead loads the weights
into ordinary memory (--load-mode none, so they count) and reads macOS `footprint`'s
phys_footprint_peak after the call. macOS only.
"""
import json, subprocess, sys, time, urllib.request
import common as C
from run import MODEL_DIR, MODELS, find_transcript
model, ctx, port = sys.argv[1], int(sys.argv[2]), 8097
t = C.load_transcript(find_transcript("synthetic_care_assessment")); tpl = C.load_template(t["template"])
secs = C.sections(t["lines"])
p = subprocess.Popen(["llama-server", "-m", str(MODEL_DIR / MODELS[model]), "-c", str(ctx), "-t", "4", "-tb", "4",
    "-ngl", "0", "-dev", "none", "-fa", "off", "-np", "1", "-cram", "0", "--reasoning", "off", "--no-webui",
    "--load-mode", "none", "--port", str(port)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
def fp():
    """Current and peak physical footprint in GB. Exits rather than return a reading it didn't get."""
    res = subprocess.run(["footprint", "-f", "bytes", "-p", str(p.pid)], capture_output=True, text=True)
    vals = {}
    for l in res.stdout.splitlines():
        l = l.strip()
        if l.startswith("phys_footprint_peak:"): vals["peak"] = int(l.split()[1]) / 2**30
        elif l.startswith("phys_footprint:"): vals["now"] = int(l.split()[1]) / 2**30
    if res.returncode != 0 or not vals.get("now") or not vals.get("peak"):
        raise SystemExit(f"footprint gave no reading (exit {res.returncode}): {res.stderr.strip()[:200]}")
    return vals
try:
    for _ in range(300):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2); break
        except Exception: time.sleep(1)
    idle = fp()
    body = {"messages": C.map_messages(t, tpl, secs[0], 0, len(secs)), "temperature": 0.2, "seed": 42, "max_tokens": 2048,
            "cache_prompt": False, "response_format": {"type": "json_schema", "json_schema": {"name": "out", "schema": C.facts_schema(tpl)}}}
    r = json.loads(urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",
        data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}), timeout=3600).read())
    after = fp()
    row = json.dumps({"model": model, "ctx": ctx, "idle_gb": round(idle["now"], 2),
        "peak_gb": round(after["peak"], 2), "prompt_n": r["timings"]["prompt_n"]})
    print(row)
    with open(C.RESULTS / "memory.jsonl", "a") as f:
        f.write(row + "\n")
finally:
    p.terminate(); p.wait()
