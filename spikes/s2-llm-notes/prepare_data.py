#!/usr/bin/env python3
"""Build the S2 test transcripts.

AMI (CC-BY-4.0): streams the human transcripts of the `ihm` test split, without audio, and builds
fixed-length buckets from meetings in the same series:
  ami-15  ES2004a trimmed to 15 min
  ami-45  EN2002c trimmed to 45 min
  ami-60  ES2004b + ES2004c, trimmed to 60 min
  ami-90  ES2004b + ES2004c + ES2004d, trimmed to 90 min
Consecutive utterances from the same speaker less than 1.5 s apart are merged into one line, so the
line-number overhead stays realistic. AMI reference text is upper-case with no punctuation. We
lower-case it and capitalise the first letter of each line. That makes it harder to read than
Parakeet output, which has punctuation, and this is noted as a limitation in the write-up.

The synthetic transcripts in data/synthetic_*.json are only validated here.

Run with a Python that has `datasets` (e.g. ~/kindtech-transcribe/.venv-sherpa/bin/python).
Output goes to data/ami/*.json, which is gitignored and regenerable.
"""
import json
import sys

from common import DATA, line_num

BUCKETS = {
    "ami-15": (["ES2004a"], 15),
    "ami-45": (["EN2002c"], 45),
    "ami-60": (["ES2004b", "ES2004c"], 60),
    "ami-90": (["ES2004b", "ES2004c", "ES2004d"], 90),
}
MERGE_GAP = 1.5


def fetch_ami():
    from datasets import load_dataset

    ds = load_dataset(
        "edinburghcstr/ami", "ihm", split="test", streaming=True,
        columns=["meeting_id", "begin_time", "end_time", "speaker_id", "text"],
    )
    meetings = {}
    for ex in ds:
        meetings.setdefault(ex["meeting_id"], []).append(ex)
    for v in meetings.values():
        v.sort(key=lambda e: (e["begin_time"], e["end_time"]))
    return meetings


def tidy(text):
    t = text.lower().strip()
    return t[:1].upper() + t[1:]


def build_bucket(meetings, ids, minutes):
    utts, offset = [], 0.0
    for mid in ids:
        m = meetings[mid]
        base = m[0]["begin_time"]
        for e in m:
            utts.append({**e, "begin_time": e["begin_time"] - base + offset,
                         "end_time": e["end_time"] - base + offset})
        offset = utts[-1]["end_time"] + 5.0
    utts = [u for u in utts if u["begin_time"] < minutes * 60]

    # Speaker letters are assigned per bucket in order of first appearance.
    letters, lines = {}, []
    for u in utts:
        spk = letters.setdefault(u["speaker_id"], chr(ord("A") + len(letters)))
        if lines and lines[-1]["speaker"] == spk and u["begin_time"] - lines[-1]["end"] < MERGE_GAP:
            lines[-1]["text"] += " " + u["text"].lower()
            lines[-1]["end"] = max(lines[-1]["end"], u["end_time"])
            continue
        lines.append({"speaker": spk, "start": round(u["begin_time"], 2),
                      "end": round(u["end_time"], 2), "text": u["text"]})
    for i, l in enumerate(lines, 1):
        l["id"] = f"L{i:04d}"
        l["text"] = tidy(l["text"])
    return {
        "speakers": {v: f"Participant {v}" for v in letters.values()},
        "lines": [{k: l[k] for k in ("id", "start", "end", "speaker", "text")} for l in lines],
    }


def validate_synthetic(path):
    t = json.loads(path.read_text())
    ids = [l["id"] for l in t["lines"]]
    assert ids == [f"L{i:04d}" for i in range(1, len(ids) + 1)], f"{path.name}: non-sequential ids"
    for a, b in zip(t["lines"], t["lines"][1:]):
        assert a["end"] <= b["start"], f"{path.name}: overlap at {a['id']}"
    known = set(ids)
    for item in t.get("facts", []) + t.get("traps", []):
        for lid in item.get("lines", []):
            assert lid in known, f"{path.name}: {item['id']} cites unknown {lid}"
    words = sum(len(l["text"].split()) for l in t["lines"])
    dur = (t["lines"][-1]["end"] - t["lines"][0]["start"]) / 60
    print(f"ok {path.name}: {len(ids)} lines, {words} words, {dur:.1f} min, "
          f"{len(t.get('facts', []))} facts, {len(t.get('traps', []))} traps")


def main():
    for p in sorted(DATA.glob("synthetic_*.json")):
        validate_synthetic(p)
    if "--synthetic-only" in sys.argv:
        return
    out = DATA / "ami"
    out.mkdir(parents=True, exist_ok=True)
    meetings = fetch_ami()
    for name, (ids, minutes) in BUCKETS.items():
        b = build_bucket(meetings, ids, minutes)
        t = {"id": name, "title": f"AMI {' + '.join(ids)} ({minutes} min)",
             "template": "general_minutes", "source": "AMI Meeting Corpus (CC-BY-4.0)", **b}
        (out / f"{name}.json").write_text(json.dumps(t, indent=1))
        words = sum(len(l["text"].split()) for l in t["lines"])
        print(f"ok {name}: {len(t['lines'])} lines, {words} words, "
              f"{(t['lines'][-1]['end'] - t['lines'][0]['start']) / 60:.1f} min, "
              f"{len(t['speakers'])} speakers, last line L{line_num(t['lines'][-1]['id'])}")


if __name__ == "__main__":
    main()
