#!/usr/bin/env python3
"""Turn YouTube rolling auto-caption VTT into a clean, de-duplicated transcript.

YouTube's auto-caption VTT repeats each line several times as words roll in, and
interleaves inline <timestamp><c> tags. This collapses that into one line per
finalized caption cue, prefixed with [MM:SS], so the result reads like a transcript.

Usage: uv run clean_vtt.py downloads/mcp/talk.en.vtt -o out/mcp_talk_captions.txt
"""

import argparse
import re
from pathlib import Path

TS = re.compile(r"^(\d{2}):(\d{2}):(\d{2})\.\d{3} -->")
TAG = re.compile(r"<[^>]+>")


def parse(vtt: str):
    lines, last = [], None
    cur_start = None
    for block in vtt.split("\n"):
        m = TS.match(block)
        if m:
            h, mm, s = int(m.group(1)), int(m.group(2)), int(m.group(3))
            cur_start = h * 3600 + mm * 60 + s
            continue
        text = TAG.sub("", block).strip()
        if not text or text == "&nbsp;":
            continue
        # Skip the rolling partial lines: keep a cue only when its text changes.
        if text == last or cur_start is None:
            continue
        lines.append((cur_start, text))
        last = text
    return lines


def main():
    p = argparse.ArgumentParser()
    p.add_argument("vtt")
    p.add_argument("-o", "--output", required=True)
    args = p.parse_args()

    raw = Path(args.vtt).read_text(encoding="utf-8", errors="replace")
    cues = parse(raw)

    # Stitch into ~20s timestamped paragraphs for readability.
    out, buf, blk_start, last_stamp = [], [], None, -999
    for start, text in cues:
        if blk_start is None:
            blk_start = start
        buf.append(text)
        if start - last_stamp >= 20:
            last_stamp = start
        if start - blk_start >= 30:
            mm, ss = divmod(blk_start, 60)
            out.append(f"[{mm:02d}:{ss:02d}] " + " ".join(buf))
            buf, blk_start = [], None
    if buf:
        mm, ss = divmod(blk_start or 0, 60)
        out.append(f"[{mm:02d}:{ss:02d}] " + " ".join(buf))

    Path(args.output).write_text("\n\n".join(out) + "\n", encoding="utf-8")
    print(f"{len(cues)} cues -> {len(out)} paragraphs -> {args.output}")


if __name__ == "__main__":
    main()
