#!/usr/bin/env python3
"""Run the S4 grid: model x prompt x vignette, sectioned strategy, on a CPU-only llama-server.

Adapted from spikes/s2-llm-notes/run.py. The only strategy is S2's "sectioned": one map call per
~10-minute section (every S4 vignette is 5-8 minutes, so one call), and the note is the section-tagged
facts grouped under the template's headings. There is no reduce call. Differences from S2:
  - the system prompt is chosen per run (`--prompts neutral,statutory`, see common.PROMPTS);
  - the map output cap is 3072 tokens (S2: 2048);
  - the raw model output is kept in the JSONL record;
  - finished (model, prompt, vignette) notes are skipped unless --force.

Outputs:
  results/runs.jsonl                                   one record per map call
  results/notes/<model>/<prompt>/<vignette>.json       {"note": sectioned note, "facts": [...]}
  results/logs/server-<model>-t<threads>.log           llama-server log

Usage:
  python3 run.py --models qwen3-4b,qwen3.5-4b --prompts neutral,statutory --vignettes all
"""
import argparse
import json
import os
import pathlib
import socket
import subprocess
import time
import urllib.request
import uuid

import common as C

MODEL_DIR = pathlib.Path.home() / "kindtech-transcribe/models/llm"
MODELS = {
    "qwen3-4b": "Qwen3-4B-Instruct-2507-Q4_0.gguf",
    "qwen3.5-4b": "Qwen3.5-4B-Q4_0.gguf",
}
MAP_MAX_TOKENS = 3072


class Server:
    # Same flags as S2: CPU only, flash attention off, one slot, prompt cache off, reasoning off.
    def __init__(self, model_key, threads, ctx, port, log_dir):
        self.url = f"http://127.0.0.1:{port}"
        gguf = MODEL_DIR / MODELS[model_key]
        if not gguf.exists():
            raise SystemExit(f"missing {gguf}")
        # A server already on this port would answer /health and every request below, and its output
        # would be recorded under this model's name.
        with socket.socket() as s:
            if s.connect_ex(("127.0.0.1", port)) == 0:
                raise SystemExit(f"port {port} is already in use; stop that server or pass --port")
        log_dir.mkdir(parents=True, exist_ok=True)
        self.log = log_dir / f"server-{model_key}-t{threads}.log"
        cmd = ["llama-server", "-m", str(gguf), "-c", str(ctx), "-t", str(threads),
               "-tb", str(threads), "-ngl", "0", "-dev", "none", "-fa", "off", "-np", "1", "-cram", "0",
               "--reasoning", "off", "--no-webui", "--port", str(port), "--log-file", str(self.log)]
        self.proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
        t0 = time.time()
        while time.time() - t0 < 300:
            if self.proc.poll() is not None:
                raise SystemExit(f"llama-server exited; see {self.log}")
            try:
                with urllib.request.urlopen(self.url + "/health", timeout=2) as r:
                    if r.status == 200 and self.proc.poll() is None:
                        break
            except Exception:
                time.sleep(1)
        else:
            self.stop()
            raise SystemExit(f"server for {model_key} did not become healthy; see {self.log}")
        self.load_s = round(time.time() - t0, 1)

    def chat(self, messages, schema, max_tokens, list_key):
        body = {
            "messages": messages,
            "temperature": 0.2,
            "seed": 42,
            "max_tokens": max_tokens,
            "cache_prompt": False,
            "response_format": {"type": "json_schema", "json_schema": {"name": "out", "schema": schema}},
        }
        req = urllib.request.Request(self.url + "/v1/chat/completions",
                                     data=json.dumps(body).encode(),
                                     headers={"Content-Type": "application/json"})
        t0 = time.time()
        with urllib.request.urlopen(req, timeout=3600) as r:
            resp = json.loads(r.read())
        timings = resp.get("timings", {})
        choice = resp["choices"][0]
        content = choice["message"].get("content") or ""
        parsed, ok = C.salvage_json(content, list_key)
        return {
            "wall_s": round(time.time() - t0, 2),
            "prompt_n": timings.get("prompt_n"),
            "predicted_n": timings.get("predicted_n"),
            "finish_reason": choice.get("finish_reason"),
            "json_ok": ok,
            "salvaged": bool(parsed) and not ok,
        }, parsed, content

    def stop(self):
        self.proc.terminate()
        try:
            self.proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.proc.kill()


def note_path(model, prompt, vid):
    return C.RESULTS / "notes" / model / prompt / f"{vid}.json"


def run_one(srv, model, prompt, vpath, out):
    """One note. Its map calls in runs.jsonl and the saved note share a run_id, so score.py pairs a note
    with the calls that made it even when a rerun was interrupted between the two writes."""
    run_id = uuid.uuid4().hex[:12]
    t = C.load_transcript(vpath)
    tpl = C.load_template(t["template"])
    secs = C.sections(t["lines"])
    facts = []
    for i, sec in enumerate(secs):
        msgs = C.map_messages(t, tpl, sec, i, len(secs), C.PROMPTS[prompt])
        m, parsed, raw = srv.chat(msgs, C.facts_schema(tpl), MAP_MAX_TOKENS, "facts")
        rec = {"run_id": run_id, "model": model, "prompt": prompt, "vignette": t["id"], "stage": f"map{i}",
               "n_sections": len(secs), **m, "n_facts": len((parsed or {}).get("facts", [])), "raw": raw}
        out.write(json.dumps(rec) + "\n")
        out.flush()
        print(f"  {model} {prompt} {t['id']} map{i}: wall_s={m['wall_s']} prompt_n={m['prompt_n']} "
              f"predicted_n={m['predicted_n']} json_ok={m['json_ok']} finish={m['finish_reason']} "
              f"facts={rec['n_facts']}", flush=True)
        if parsed:
            facts.extend(parsed.get("facts", []))
    p = note_path(model, prompt, t["id"])
    p.parent.mkdir(parents=True, exist_ok=True)
    C.write_text(p, json.dumps({"run_id": run_id, "note": C.sectioned_note(tpl, facts), "facts": facts}, indent=1,
                            ensure_ascii=False))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", default="qwen3-4b,qwen3.5-4b")
    ap.add_argument("--prompts", default="neutral,statutory")
    ap.add_argument("--vignettes", default="all")
    ap.add_argument("--threads", type=int, default=8)
    ap.add_argument("--ctx", type=int, default=8192)
    ap.add_argument("--port", type=int, default=8094)
    ap.add_argument("--force", action="store_true", help="re-run notes that already exist")
    a = ap.parse_args()

    vpaths = C.vignette_paths() if a.vignettes == "all" else [C.find_vignette(v) for v in a.vignettes.split(",")]
    prompts = a.prompts.split(",")
    C.RESULTS.mkdir(exist_ok=True)
    with open(C.RESULTS / "runs.jsonl", "a", encoding="utf-8") as out:
        for model in a.models.split(","):
            todo = [(p, v) for p in prompts for v in vpaths
                    if a.force or not note_path(model, p, v.stem).exists()]
            if not todo:
                print(f"== {model}: nothing to do")
                continue
            print(f"== {model} (t={a.threads}, ctx={a.ctx}, port={a.port}): {len(todo)} notes", flush=True)
            srv = Server(model, a.threads, a.ctx, a.port, C.RESULTS / "logs")
            try:
                # os.getloadavg does not exist on Windows.
                load = [round(x, 1) for x in os.getloadavg()] if hasattr(os, "getloadavg") else None
                out.write(json.dumps({"model": model, "stage": "load", "load_s": srv.load_s, "threads": a.threads,
                                      "ctx": a.ctx, "loadavg": load}) + "\n")
                for prompt, vp in todo:
                    run_one(srv, model, prompt, vp, out)
            finally:
                srv.stop()


if __name__ == "__main__":
    main()
