#!/usr/bin/env python3
"""Run the S2 grid: model x transcript x strategy on a CPU-only llama-server.

For each model this starts `llama-server` in CPU-only mode (-ngl 0 -dev none, prompt cache disabled,
reasoning off), then for each transcript runs:
  single     the whole transcript in one call
  mapreduce  ~10-min sections -> one "map" call per section (cited, section-tagged facts) -> one "reduce" call
             (the note). Timing is reported two ways:
               (b) after-stop = all maps + reduce
               (c) incremental = last map + reduce (earlier maps run during the recording)
  sectioned  (d) derived from the same map calls: the note is the section-tagged facts, with no
             reduce, so only the last map runs after stop.

One JSONL record is written per LLM call, plus one "summary" record per (model, transcript,
strategy). Notes go to results/notes/<run>/<model>/<transcript>.<strategy>.json.

Usage:
  python run.py --run smoke --models qwen3-4b --transcripts ami-15 --strategies single
  python run.py --run screen --models all --transcripts synthetic_care_assessment
"""
import argparse
import json
import os
import pathlib
import subprocess
import threading
import time
import urllib.request

import common as C

MODEL_DIR = pathlib.Path.home() / "kindtech-transcribe/models/llm"
# Q4_0 throughout: llama.cpp's CPU backend repacks Q4_0 weights for faster matmuls (ARM i8mm / x86
# AVX2), which gave ~2x prefill over Q4_K_M in llama-bench on the same model.
MODELS = {
    "qwen3-4b": "Qwen3-4B-Instruct-2507-Q4_0.gguf",
    "qwen3.5-4b": "Qwen3.5-4B-Q4_0.gguf",
    "smollm3-3b": "HuggingFaceTB_SmolLM3-3B-Q4_0.gguf",
    "phi4-mini": "microsoft_Phi-4-mini-instruct-Q4_0.gguf",
    "ministral3-3b": "mistralai_Ministral-3-3B-Instruct-2512-Q4_0.gguf",
    "gemma4-e4b": "gemma-4-E4B-it-Q4_0.gguf",
}


def find_transcript(name):
    for p in [C.DATA / f"{name}.json", C.DATA / "ami" / f"{name}.json"]:
        if p.exists():
            return p
    raise SystemExit(f"unknown transcript {name}")


class RssSampler:
    """Polls the server's resident set size, so we can record the peak during each call."""

    def __init__(self, pid):
        self.pid, self.peak, self._stop = pid, 0, threading.Event()
        threading.Thread(target=self._loop, daemon=True).start()

    def _loop(self):
        while not self._stop.is_set():
            try:
                out = subprocess.run(["ps", "-o", "rss=", "-p", str(self.pid)],
                                     capture_output=True, text=True).stdout.strip()
                if out:
                    self.peak = max(self.peak, int(out) // 1024)
            except Exception:
                pass
            time.sleep(0.25)

    def reset(self):
        p, self.peak = self.peak, 0
        return p

    def stop(self):
        self._stop.set()


class Server:
    # -fa off: on CPU, llama.cpp's flash-attention path collapses at long context (Qwen3-4B Q4_0,
    # 4 threads: pp512 @ 8k depth = 10 tok/s with FA vs 28 without); "auto" turned it on.
    def __init__(self, model_key, threads, ctx, port, log_dir):
        self.url = f"http://127.0.0.1:{port}"
        gguf = MODEL_DIR / MODELS[model_key]
        if not gguf.exists():
            raise SystemExit(f"missing {gguf}")
        log_dir.mkdir(parents=True, exist_ok=True)
        self.log = log_dir / f"server-{model_key}-t{threads}.log"
        cmd = ["llama-server", "-m", str(gguf), "-c", str(ctx), "-t", str(threads),
               "-tb", str(threads), "-ngl", "0", "-dev", "none", "-fa", "off", "-np", "1", "-cram", "0",
               "--reasoning", "off", "--no-webui", "--port", str(port), "--log-file", str(self.log)]
        self.proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
        t0 = time.time()
        while time.time() - t0 < 300:
            try:
                with urllib.request.urlopen(self.url + "/health", timeout=2) as r:
                    if r.status == 200:
                        break
            except Exception:
                time.sleep(1)
        else:
            self.stop()
            raise SystemExit(f"server for {model_key} did not become healthy; see {self.log}")
        self.load_s = round(time.time() - t0, 1)
        self.rss = RssSampler(self.proc.pid)

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
        self.rss.reset()
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
            "prompt_ms": timings.get("prompt_ms"),
            "predicted_n": timings.get("predicted_n"),
            "predicted_ms": timings.get("predicted_ms"),
            "prompt_tps": round(timings.get("prompt_per_second") or 0, 1),
            "gen_tps": round(timings.get("predicted_per_second") or 0, 1),
            "finish_reason": choice.get("finish_reason"),
            "peak_rss_mb": self.rss.peak,
            "json_ok": ok,
            "salvaged": bool(parsed) and not ok,
        }, parsed, content

    def stop(self):
        if hasattr(self, "rss"):
            self.rss.stop()
        self.proc.terminate()
        try:
            self.proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.proc.kill()


def run_transcript(srv, model, tpath, strategies, out, notes_dir, meta):
    t = C.load_transcript(tpath)
    tpl = C.load_template(t["template"])
    lines = t["lines"]
    base = {**meta, "model": model, "transcript": t["id"], "minutes": round(C.duration_min(lines), 1),
            "n_lines": len(lines)}

    def emit(rec):
        out.write(json.dumps(rec) + "\n")
        out.flush()
        tag = rec.get("stage", "")
        print(f"  {model} {t['id']} {rec['strategy']} {tag}: "
              + ", ".join(f"{k}={rec[k]}" for k in ("wall_s", "prompt_n", "prompt_tps", "gen_tps",
                                                     "peak_rss_mb", "json_ok", "finish_reason")
                          if k in rec), flush=True)

    ndir = notes_dir / model
    ndir.mkdir(parents=True, exist_ok=True)

    if "single" in strategies:
        m, note, raw = srv.chat(C.single_pass_messages(t, tpl, lines), C.note_schema(tpl), 4096)
        emit({**base, "strategy": "single", "stage": "single", **m})
        emit({**base, "strategy": "single", "stage": "summary", "after_stop_s": m["wall_s"],
              "peak_rss_mb": m["peak_rss_mb"], "json_ok": m["json_ok"], "wall_s": m["wall_s"]})
        (ndir / f"{t['id']}.single.json").write_text(json.dumps(note if note else {"raw": raw}, indent=1))

    if "mapreduce" in strategies:
        secs = C.sections(lines)
        facts, map_walls, peaks, ok = [], [], [], True
        for i, sec in enumerate(secs):
            m, parsed, _ = srv.chat(C.map_messages(t, tpl, sec, i, len(secs)), C.facts_schema(tpl), 2048, "facts")
            emit({**base, "strategy": "mapreduce", "stage": f"map{i}", **m})
            map_walls.append(m["wall_s"])
            peaks.append(m["peak_rss_mb"])
            ok &= m["json_ok"]
            if parsed:
                facts.extend(parsed.get("facts", []))
        m, note, raw = srv.chat(C.reduce_messages(t, tpl, facts), C.note_schema(tpl), 4096)
        emit({**base, "strategy": "mapreduce", "stage": "reduce", "n_facts": len(facts), **m})
        peaks.append(m["peak_rss_mb"])
        emit({**base, "strategy": "mapreduce", "stage": "summary", "n_sections": len(secs),
              "after_stop_s": round(sum(map_walls) + m["wall_s"], 1),
              "incremental_after_stop_s": round(map_walls[-1] + m["wall_s"], 1),
              "wall_s": round(sum(map_walls) + m["wall_s"], 1),
              "peak_rss_mb": max(peaks), "json_ok": ok and m["json_ok"]})
        # (d) sectioned: the note is the section-tagged facts, so nothing but the last map runs after stop.
        emit({**base, "strategy": "sectioned", "stage": "summary", "n_sections": len(secs),
              "after_stop_s": round(sum(map_walls), 1), "incremental_after_stop_s": map_walls[-1],
              "wall_s": round(sum(map_walls), 1), "peak_rss_mb": max(peaks[:-1]), "json_ok": ok})
        (ndir / f"{t['id']}.sectioned.json").write_text(json.dumps(C.sectioned_note(tpl, facts), indent=1))
        (ndir / f"{t['id']}.mapreduce.json").write_text(
            json.dumps({"note": note if note else {"raw": raw}, "facts": facts}, indent=1))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", required=True, help="run tag; results/<run>.jsonl")
    ap.add_argument("--models", default="all")
    ap.add_argument("--transcripts", required=True)
    ap.add_argument("--strategies", default="single,mapreduce")
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--ctx", type=int, default=32768)
    ap.add_argument("--port", type=int, default=8091)
    a = ap.parse_args()

    models = list(MODELS) if a.models == "all" else a.models.split(",")
    tpaths = [find_transcript(n) for n in a.transcripts.split(",")]
    strategies = a.strategies.split(",")
    C.RESULTS.mkdir(exist_ok=True)
    notes_dir = C.RESULTS / "notes" / a.run
    meta = {"run": a.run, "threads": a.threads, "ctx": a.ctx}

    with open(C.RESULTS / f"{a.run}.jsonl", "a") as out:
        for model in models:
            if not (MODEL_DIR / MODELS[model]).exists():
                print(f"skip {model}: not downloaded")
                continue
            print(f"== {model} (t={a.threads}, ctx={a.ctx})", flush=True)
            srv = Server(model, a.threads, a.ctx, a.port, C.RESULTS / "logs")
            out.write(json.dumps({**meta, "model": model, "stage": "load", "load_s": srv.load_s,
                                  "loadavg": [round(x, 1) for x in os.getloadavg()]}) + "\n")
            try:
                for tp in tpaths:
                    run_transcript(srv, model, tp, strategies, out, notes_dir, meta)
            finally:
                srv.stop()


if __name__ == "__main__":
    main()
