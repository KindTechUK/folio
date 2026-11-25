#!/usr/bin/env python3
"""Run the S3 grid: model x transcript, sectioned strategy, on a CPU-only llama-server.

Copied from spikes/s2-llm-notes/run.py and cut down. S3 measures quality only, not speed (timings are
still recorded because llama-server returns them, but the machine is shared and they mean little).

For each model this starts `llama-server` in CPU-only mode (-ngl 0 -dev none, prompt cache disabled,
reasoning off), then for each transcript:
  ~10-min sections -> one "map" call per section (cited, section-tagged facts) -> the note is the facts
  grouped by template section (S2's "sectioned" strategy, S2's recommended note shape).
  With --strategies mapreduce the S2 reduce call also runs (its output is saved but unused by S3).

Changes from S2: transcript lines carry [mm:ss] start times (common.render_lines); map output cap 3072
(S2 recommended ~3k; S2 used 2048); defaults 8 threads, 8k context, port 8093; only the two S2
finalists; each call's raw output is kept in the JSONL; after each note the harness writes the span
shape {section, sentence, cites: [{start, end}]} next to it (<tid>.sectioned.spans.json).

Usage:
  python run.py --run s3 --models qwen3-4b,qwen3.5-4b --transcripts all
"""
import argparse
import json
import os
import pathlib
import socket
import subprocess
import time
import urllib.request

import common as C

MODEL_DIR = pathlib.Path.home() / "kindtech-transcribe/models/llm"
MODELS = {  # the two S2 finalists, Q4_0
    "qwen3-4b": "Qwen3-4B-Instruct-2507-Q4_0.gguf",
    "qwen3.5-4b": "Qwen3.5-4B-Q4_0.gguf",
}
MAP_MAX_TOKENS = 3072
REDUCE_MAX_TOKENS = 4096

# Tuning set (S2's transcripts) and test set (written for S3).
TUNING = ["synthetic_care_assessment", "synthetic_legal_interview"]
TEST = ["synthetic_mh_review", "synthetic_housing_assessment", "synthetic_tribunal_interview",
        "synthetic_planning_meeting"]


def find_transcript(name):
    # Accepts the file stem (synthetic_care_assessment) or the transcript id (synthetic-care-assessment).
    for n in {name, name.replace("-", "_")}:
        p = C.DATA / f"{n}.json"
        if p.exists():
            return p
    raise SystemExit(f"unknown transcript {name}")


def split_of(tid):
    return "tuning" if tid.replace("-", "_") in TUNING else "test"


class Server:
    # -fa off: on CPU, llama.cpp's flash-attention path collapses at long context (see S2).
    def __init__(self, model_key, threads, ctx, port, log_dir):
        self.url = f"http://127.0.0.1:{port}"
        gguf = MODEL_DIR / MODELS[model_key]
        if not gguf.exists():
            raise SystemExit(f"missing {gguf}")
        log_dir.mkdir(parents=True, exist_ok=True)
        self.log = log_dir / f"server-{model_key}-t{threads}.log"
        # Another llama-server on this port would answer /health before ours fails to bind, and its output
        # would be recorded under this model's name.
        with socket.socket() as probe:
            if probe.connect_ex(("127.0.0.1", port)) == 0:
                raise SystemExit(f"port {port} is already in use; stop that server or pass --port")
        cmd = ["llama-server", "-m", str(gguf), "-c", str(ctx), "-t", str(threads),
               "-tb", str(threads), "-ngl", "0", "-dev", "none", "-fa", "off", "-np", "1", "-cram", "0",
               "--reasoning", "off", "--no-webui", "--port", str(port), "--log-file", str(self.log)]
        self.proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
        t0 = time.time()
        while time.time() - t0 < 300:
            if self.proc.poll() is not None:
                raise SystemExit(f"llama-server exited early (port {port} busy?); see {self.log}")
            try:
                with urllib.request.urlopen(self.url + "/health", timeout=2) as r:
                    healthy = r.status == 200
                if healthy:
                    time.sleep(1)  # a child that lost the port race exits right after start-up
                    if self.proc.poll() is not None:
                        raise SystemExit(f"llama-server exited (port {port} busy?); see {self.log}")
                    break
            except Exception:
                time.sleep(1)
        else:
            self.stop()
            raise SystemExit(f"server for {model_key} did not become healthy; see {self.log}")
        self.load_s = round(time.time() - t0, 1)

    def chat(self, messages, schema, max_tokens, list_key="sections"):
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
        with urllib.request.urlopen(req, timeout=7200) as r:
            resp = json.loads(r.read())
        wall = time.time() - t0
        timings = resp.get("timings", {})
        choice = resp["choices"][0]
        content = choice["message"].get("content") or ""
        parsed, ok = C.salvage_json(content, list_key)
        return {
            "wall_s": round(wall, 2),
            "prompt_n": timings.get("prompt_n"),
            "predicted_n": timings.get("predicted_n"),
            "finish_reason": choice.get("finish_reason"),
            "json_ok": ok,
            "salvaged": bool(parsed) and not ok,
            "raw": content,
        }, parsed

    def stop(self):
        self.proc.terminate()
        try:
            self.proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.proc.kill()


def run_transcript(srv, model, tpath, strategies, out, notes_dir, meta):
    t = C.load_transcript(tpath)
    tpl = C.load_template(t["template"])
    lines = t["lines"]
    base = {**meta, "model": model, "transcript": t["id"], "split": split_of(t["id"]),
            "minutes": round(C.duration_min(lines), 1), "n_lines": len(lines)}

    def emit(rec):
        out.write(json.dumps(rec) + "\n")
        out.flush()
        print(f"  {model} {t['id']} {rec['stage']}: "
              + ", ".join(f"{k}={rec[k]}" for k in ("wall_s", "prompt_n", "predicted_n", "json_ok",
                                                     "salvaged", "finish_reason", "n_facts") if k in rec),
              flush=True)

    ndir = notes_dir / model
    ndir.mkdir(parents=True, exist_ok=True)

    secs = C.sections(lines)
    facts, ok = [], True
    for i, sec in enumerate(secs):
        m, parsed = srv.chat(C.map_messages(t, tpl, sec, i, len(secs)), C.facts_schema(tpl), MAP_MAX_TOKENS,
                             "facts")
        emit({**base, "strategy": "sectioned", "stage": f"map{i}",
              "section_lines": [sec[0]["id"], sec[-1]["id"]], **m})
        ok &= m["json_ok"]
        if parsed:
            facts.extend(parsed.get("facts", []))
    note = C.sectioned_note(tpl, facts)
    C.write_text(ndir / f"{t['id']}.sectioned.json", json.dumps(note, indent=1))
    C.write_text(ndir / f"{t['id']}.sectioned.spans.json", json.dumps(C.span_note(note, lines), indent=1))
    emit({**base, "strategy": "sectioned", "stage": "summary", "n_sections": len(secs), "n_facts": len(facts),
          "json_ok": ok})

    if "mapreduce" in strategies:  # S2's reduce on the same facts; saved, not scored in S3
        m, rnote = srv.chat(C.reduce_messages(t, tpl, facts), C.note_schema(tpl), REDUCE_MAX_TOKENS)
        emit({**base, "strategy": "mapreduce", "stage": "reduce", "n_facts": len(facts), **m})
        C.write_text(ndir / f"{t['id']}.mapreduce.json",
            json.dumps({"note": rnote if rnote else {"raw": m["raw"]}, "facts": facts}, indent=1))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", required=True, help="run tag; results/<run>.jsonl")
    ap.add_argument("--models", default="all")
    ap.add_argument("--transcripts", default="all", help="comma list of file stems, or all|tuning|test")
    ap.add_argument("--strategies", default="sectioned", help="sectioned (maps only) or mapreduce (+reduce)")
    ap.add_argument("--threads", type=int, default=8)
    ap.add_argument("--ctx", type=int, default=8192)
    ap.add_argument("--port", type=int, default=8093)
    a = ap.parse_args()

    models = list(MODELS) if a.models == "all" else a.models.split(",")
    names = {"all": TUNING + TEST, "tuning": TUNING, "test": TEST}.get(a.transcripts)
    tpaths = [find_transcript(n) for n in (names or a.transcripts.split(","))]
    strategies = a.strategies.split(",")
    C.RESULTS.mkdir(exist_ok=True)
    notes_dir = C.RESULTS / "notes" / a.run
    meta = {"run": a.run, "threads": a.threads, "ctx": a.ctx, "map_max_tokens": MAP_MAX_TOKENS}

    with open(C.RESULTS / f"{a.run}.jsonl", "a", encoding="utf-8") as out:
        for model in models:
            print(f"== {model} (t={a.threads}, ctx={a.ctx}, port={a.port})", flush=True)
            srv = Server(model, a.threads, a.ctx, a.port, C.RESULTS / "logs")
            try:
                # os.getloadavg() does not exist on Windows.
                load = [round(x, 1) for x in os.getloadavg()] if hasattr(os, "getloadavg") else None
                out.write(json.dumps({**meta, "model": model, "stage": "load", "load_s": srv.load_s,
                                      "loadavg": load}) + "\n")
                for tp in tpaths:
                    run_transcript(srv, model, tp, strategies, out, notes_dir, meta)
            finally:
                srv.stop()


if __name__ == "__main__":
    main()
