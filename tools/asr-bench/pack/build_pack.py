# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pyarrow", "scipy", "soundfile", "fsspec", "aiohttp"]
# ///
"""Build an asr-bench pack: 16 kHz mono 16-bit WAVs plus manifest.jsonl.

Run once on a machine with network access; copy the pack folder to the machine under test.

    uv run --script tools/asr-bench/pack/build_pack.py ~/folio-bench-pack            # every set
    uv run --script tools/asr-bench/pack/build_pack.py ~/folio-bench-pack ami-sdm     # some sets

Short-clip sets use voice2text's sample (seed 20260929, 150 clips; its
utils/asr_grid/fetch_noisy.py), so scores compare with its grid:

- ami-sdm: AMI on the room microphone (edinburghcstr/ami, sdm, test, shard 0: meetings
  EN2002a-c), clips of 2-35 s. CC-BY-4.0.
- ami-ihm: the Open ASR Leaderboard's AMI-Cleaned (headset microphones, clips under 1 s removed;
  hf-audio/open-asr-leaderboard, ami_cleaned, all 16 test meetings). Only the 150 sampled clips
  are downloaded, through the datasets-server rows API.
- earnings22: earnings calls in short chunks with original transcripts
  (distil-whisper/earnings22, chunked, test, shard 0).

Long-form sets (one clip per recording; segmentation only matters on long audio):

- ami-sdm-long: whole meetings IS1009a, ES2004a and TS3003a (55 min) on the room microphone
  (diarizers-community/ami, sdm, test). References: the AMI manual annotations 1.6.2 word
  files, every speaker's words ordered by start time, punctuation tokens dropped (as in spike S1).
- earnings22-long: the leaderboard's Earnings22-Cleaned-AA, 6 whole calls (115 min) with
  corrected transcripts (ArtificialAnalysis/Earnings22-Cleaned-AA, Apache-2.0).

Downloads are cached in <pack>/.cache; delete it after building.
"""

from __future__ import annotations

import io
import json
import random
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path
from xml.etree import ElementTree

import numpy as np
import pyarrow.parquet as pq
import soundfile as sf
from scipy.signal import resample_poly

RATE, SAMPLE, SEED = 16000, 150, 20260929
HF = "https://huggingface.co/api/datasets/{}/parquet/{}/{}/{}.parquet"
ROWS = "https://datasets-server.huggingface.co/rows?dataset={}&config={}&split={}&offset={}&length=1"
AA = "https://huggingface.co/datasets/ArtificialAnalysis/Earnings22-Cleaned-AA/resolve/main/{}"
AMI_WORDS = "https://groups.inf.ed.ac.uk/ami/AMICorpusAnnotations/ami_public_manual_1.6.2.zip"
LONG_MEETINGS = ["IS1009a", "ES2004a", "TS3003a"]


def download(url: str, dest: Path) -> Path:
    if not dest.exists():
        print(f"  downloading {url}", file=sys.stderr)
        dest.parent.mkdir(parents=True, exist_ok=True)
        part = dest.with_name(dest.name + ".part")
        req = urllib.request.Request(url, headers={"User-Agent": "asr-bench-pack"})
        with urllib.request.urlopen(req, timeout=120) as r, open(part, "wb") as f:
            while chunk := r.read(1 << 20):
                f.write(chunk)
        part.replace(dest)
    return dest


def fetch(url: str, timeout: int = 120) -> bytes:
    """GET with retries: the datasets-server rate-limits (429) bursts of requests."""
    for attempt in range(8):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "asr-bench-pack"})
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return r.read()
        except urllib.error.HTTPError as e:
            if e.code not in (429, 500, 502, 503, 504) or attempt == 7:
                raise
            wait = float(e.headers.get("Retry-After") or 0) or 5 * 2**attempt
        except (urllib.error.URLError, TimeoutError):
            if attempt == 7:
                raise
            wait = 5 * 2**attempt
        print(f"  retrying in {wait:.0f} s: {url[:90]}", file=sys.stderr)
        time.sleep(wait)
    raise AssertionError("unreachable")


def to16k(data_bytes: bytes) -> np.ndarray:
    data, rate = sf.read(io.BytesIO(data_bytes), dtype="float32", always_2d=True)
    data = data.mean(axis=1)
    return resample_poly(data, RATE, rate).astype(np.float32) if rate != RATE else data


class Pack:
    def __init__(self, root: Path):
        self.root = root
        self.cache = root / ".cache"
        self.manifest = root / "manifest.jsonl"
        self.rows: dict[str, dict] = {}
        if self.manifest.exists():
            for line in self.manifest.read_text().splitlines():
                if line.strip():
                    r = json.loads(line)
                    self.rows[r["id"]] = r

    def has_set(self, name: str) -> bool:
        return any(r["set"] == name for r in self.rows.values())

    def add(self, set_name: str, cid: str, data: np.ndarray, text: str, recording: str) -> None:
        rel = f"audio/{set_name}/{cid}.wav"
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        part = path.with_name(path.name + ".part")
        sf.write(str(part), np.clip(data, -1, 1), RATE, subtype="PCM_16", format="WAV")
        part.replace(path)
        self.rows[f"{set_name}/{cid}"] = {
            "id": f"{set_name}/{cid}", "set": set_name, "audio": rel,
            "reference": " ".join(text.split()), "recording": recording,
        }

    def save(self) -> None:
        part = self.manifest.with_name("manifest.jsonl.part")
        part.write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in self.rows.values()))
        part.replace(self.manifest)


def short_from_shard(pack: Pack, name: str, dataset: str, config: str, text_col: str, rec_col: str | None) -> None:
    """voice2text's fetch(): every 2-35 s clip with text in shard 0, shuffled, first 150."""
    shard = download(HF.format(dataset, config, "test", 0), pack.cache / f"{name}-0.parquet")
    table = pq.read_table(shard)
    cols = table.column_names
    text_col = text_col if text_col in cols else next(c for c in cols if c in ("text", "transcription", "sentence"))
    id_col = next((c for c in ("id", "audio_id", "segment_id", "file_id") if c in cols), None)
    rows = table.to_pylist()
    ids = [str(r[id_col]) for r in rows] if id_col else []
    unique_ids = len(set(ids)) == len(ids) > 0
    candidates = []
    for i, row in enumerate(rows):
        data = to16k(row["audio"]["bytes"])
        seconds = len(data) / RATE
        text = (row[text_col] or "").strip()
        if 2 <= seconds <= 35 and text and not text.startswith("<"):
            cid = str(row[id_col]) if unique_ids else f"{name}-{i:05d}"
            rec = str(row[rec_col]) if rec_col and rec_col in cols else ""
            candidates.append((cid.replace("/", "_"), data, text, rec))
    random.Random(SEED).shuffle(candidates)
    for cid, data, text, rec in sorted(candidates[:SAMPLE], key=lambda c: c[0]):
        pack.add(name, cid, data, text, rec)
    print(f"{name}: {min(SAMPLE, len(candidates))} of {len(candidates)} eligible clips", file=sys.stderr)


def ami_ihm(pack: Pack) -> None:
    """voice2text's sample of the leaderboard's AMI-Cleaned: ids sorted, shuffled, first 150."""
    import fsspec

    fs = fsspec.filesystem("https")
    ids: list[str] = []
    texts: dict[str, str] = {}
    for shard in range(3):
        url = HF.format("hf-audio/open-asr-leaderboard", "ami_cleaned", "test", shard)
        with fs.open(url, block_size=1 << 20) as f:
            t = pq.read_table(f, columns=["id", "text"])
        for i, x in zip(t.column("id").to_pylist(), t.column("text").to_pylist()):
            ids.append(i)
            texts[i] = x
    order = {cid: k for k, cid in enumerate(ids)}  # row offset across shards
    picked = sorted(ids)
    random.Random(SEED).shuffle(picked)
    for k, cid in enumerate(sorted(picked[:SAMPLE])):
        if f"ami-ihm/{cid}" in pack.rows:
            continue  # resuming after an interruption
        url = ROWS.format(urllib.parse.quote("hf-audio/open-asr-leaderboard"), "ami_cleaned", "test", order[cid])
        row = json.loads(fetch(url))["rows"][0]["row"]
        if row["id"] != cid:
            raise SystemExit(f"rows API offset {order[cid]} gave {row['id']}, expected {cid}")
        data = to16k(fetch(row["audio"][0]["src"]))
        meeting = cid.split("_")[1] if cid.startswith("AMI_") else ""
        pack.add("ami-ihm", cid, data, texts[cid], meeting)
        if k % 10 == 9:
            pack.save()
        time.sleep(1.0)  # stay under the rows API's rate limit
    print(f"ami-ihm: {SAMPLE} of {len(ids)} clips", file=sys.stderr)


def ami_words(pack: Pack) -> dict[str, str]:
    """Reference text per meeting from the AMI manual word annotations."""
    z = zipfile.ZipFile(download(AMI_WORDS, pack.cache / "ami_public_manual_1.6.2.zip"))
    refs = {}
    for meeting in LONG_MEETINGS:
        words = []
        for name in z.namelist():
            if re.fullmatch(rf"words/{meeting}\.[A-Z]\.words\.xml", name):
                root = ElementTree.fromstring(z.read(name))
                for w in root.iter("w"):
                    if w.get("punc") == "true" or not (w.text or "").strip():
                        continue
                    words.append((float(w.get("starttime", "nan")), w.text.strip()))
        words.sort(key=lambda x: x[0])
        refs[meeting] = " ".join(t for _, t in words)
    return refs


def ami_sdm_long(pack: Pack) -> None:
    import fsspec

    refs = ami_words(pack)
    fs = fsspec.filesystem("https")
    found = set()
    for shard in (0, 1):
        url = HF.format("diarizers-community/ami", "sdm", "test", shard)
        with fs.open(url, block_size=1 << 22) as f:
            pf = pq.ParquetFile(f)
            for rg in range(pf.num_row_groups):
                paths = pf.read_row_group(rg, columns=["audio.path"]).column(0).to_pylist()
                wanted = [p["path"].split(".")[0] if isinstance(p, dict) else str(p).split(".")[0] for p in paths]
                if not set(wanted) & set(LONG_MEETINGS):
                    continue
                t = pf.read_row_group(rg, columns=["audio"]).column(0).to_pylist()
                for audio in t:
                    meeting = audio["path"].split(".")[0]
                    if meeting in LONG_MEETINGS:
                        pack.add("ami-sdm-long", meeting, to16k(audio["bytes"]), refs[meeting], meeting)
                        found.add(meeting)
    missing = set(LONG_MEETINGS) - found
    if missing:
        raise SystemExit(f"ami-sdm-long: meetings not found: {missing}")
    print(f"ami-sdm-long: {len(found)} meetings", file=sys.stderr)


def earnings22_long(pack: Pack) -> None:
    meta = download(AA.format("earnings22_cleaned_aa_v1.jsonl"), pack.cache / "earnings22_cleaned_aa_v1.jsonl")
    calls = [json.loads(line) for line in meta.read_text().splitlines() if line.strip()]
    for c in calls:
        audio_name = c.get("file_name") or c.get("audio") or c.get("audio_path")
        if not audio_name:
            raise SystemExit(f"earnings22-long: no audio field in {sorted(c)}")
        audio_name = audio_name if audio_name.startswith("audio/") else f"audio/{audio_name}"
        mp3 = download(AA.format(audio_name), pack.cache / audio_name)
        pack.add("earnings22-long", str(c["id"]), to16k(mp3.read_bytes()), c["transcript"], str(c["id"]))
    print(f"earnings22-long: {len(calls)} calls", file=sys.stderr)


SETS = {
    "ami-sdm": lambda p: short_from_shard(p, "ami-sdm", "edinburghcstr/ami", "sdm", "text", "meeting_id"),
    "ami-ihm": ami_ihm,
    "earnings22": lambda p: short_from_shard(p, "earnings22", "distil-whisper/earnings22", "chunked", "transcription", "file_id"),
    "ami-sdm-long": ami_sdm_long,
    "earnings22-long": earnings22_long,
}

if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    pack = Pack(Path(sys.argv[1]).expanduser())
    pack.root.mkdir(parents=True, exist_ok=True)
    for name in sys.argv[2:] or list(SETS):
        if name not in SETS:
            raise SystemExit(f"unknown set {name}; one of {list(SETS)}")
        if pack.has_set(name) and name != "ami-ihm":
            print(f"{name}: already in the pack", file=sys.stderr)
            continue
        SETS[name](pack)
        pack.save()
    print(pack.root)
