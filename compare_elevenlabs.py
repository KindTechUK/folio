#!/usr/bin/env python3
"""Transcribe via ElevenLabs Scribe and render with the SAME formatting as the
local pipeline, for an apples-to-apples comparison against transcribe.py output.

Reuses build_turns / write_docx / save_json from transcribe.py so the only
variable is the transcription+diarization engine.

Usage:
    # put ELEVENLABS_API_KEY=... in .env first
    uv run compare_elevenlabs.py INPUT_AUDIO [-o OUT.docx] [--model scribe_v2]
"""

import argparse
import os
import re
import sys
from pathlib import Path

import requests
from dotenv import load_dotenv

import transcribe as T

ENDPOINT = "https://api.elevenlabs.io/v1/speech-to-text"


def normalize_speaker(sid: str | None) -> str:
    """Map ElevenLabs 'speaker_0' -> 'SPEAKER_00' to match local output."""
    if not sid:
        return "UNKNOWN"
    m = re.match(r"speaker_(\d+)$", sid)
    return f"SPEAKER_{int(m.group(1)):02d}" if m else sid.upper()


def main() -> None:
    load_dotenv()
    p = argparse.ArgumentParser(description="ElevenLabs Scribe comparison transcription.")
    p.add_argument("input", help="Audio file to transcribe.")
    p.add_argument("-o", "--output", help="Output .docx. Default: out/<input>_elevenlabs.docx")
    p.add_argument("--model", default="scribe_v2", help="scribe_v2 or scribe_v1.")
    args = p.parse_args()

    key = os.getenv("ELEVENLABS_API_KEY")
    if not key:
        sys.exit("Set ELEVENLABS_API_KEY in .env (https://elevenlabs.io/app/settings/api-keys).")

    in_path = Path(args.input).expanduser()
    if not in_path.exists():
        sys.exit(f"Input file not found: {in_path}")
    out_path = Path(args.output).expanduser() if args.output \
        else Path("out") / (in_path.stem + "_elevenlabs.docx")

    print(f"[elevenlabs] Uploading {in_path.name} to {args.model} (diarize + word timestamps) ...",
          file=sys.stderr, flush=True)
    with in_path.open("rb") as f:
        resp = requests.post(
            ENDPOINT,
            headers={"xi-api-key": key},
            data={
                "model_id": args.model,
                "diarize": "true",
                "timestamps_granularity": "word",
                "language_code": "en",
            },
            files={"file": (in_path.name, f)},
            timeout=1800,
        )
    if resp.status_code != 200:
        sys.exit(f"ElevenLabs API error {resp.status_code}: {resp.text[:500]}")
    data = resp.json()

    words = [w for w in data.get("words", []) if w.get("type") == "word"]
    if not words:
        sys.exit("No word-level results returned; check the model/response.")
    seg_words = [{
        "word": w.get("text", ""),
        "start": w.get("start"),
        "end": w.get("end"),
        "speaker": normalize_speaker(w.get("speaker_id")),
    } for w in words]
    # Single pseudo-segment; build_turns re-splits into sentences/turns itself.
    segments = [{"words": seg_words, "text": data.get("text", ""),
                 "start": seg_words[0]["start"], "end": seg_words[-1]["end"]}]

    T.save_json({"segments": segments}, out_path.with_suffix(".json"))
    turns = T.build_turns(segments)
    T.write_docx(turns, out_path, in_path.name, f"ElevenLabs {args.model}")
    n_spk = len({t["speaker"] for t in turns})
    print(f"[elevenlabs] Done -> {out_path}  ({len(turns)} turns, {n_spk} speakers)",
          file=sys.stderr, flush=True)


if __name__ == "__main__":
    main()
