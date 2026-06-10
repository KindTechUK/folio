#!/usr/bin/env python3
"""Local podcast transcription with speaker diarization and timestamps.

Pipeline: ffmpeg decode -> WhisperX transcribe (large-v3) -> forced alignment
(word timestamps) -> pyannote diarization -> word-level speaker turns -> Word doc.

Speakers are emitted as SPEAKER_00 / SPEAKER_01 / ... for you to relabel afterward.

A JSON sidecar (next to the .docx) stores the full word-level result, so you can
re-render the document with different formatting via --render-only without
re-transcribing (which is the slow part).
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

import numpy as np
from docx import Document
from docx.shared import Pt, RGBColor
from dotenv import load_dotenv

SAMPLE_RATE = 16000


def log(msg: str) -> None:
    print(f"[transcribe] {msg}", file=sys.stderr, flush=True)


def decode(path: str, sr: int = SAMPLE_RATE, channels: int = 1) -> np.ndarray:
    """Decode any audio/video file to float32 PCM via the system ffmpeg.

    Returns a 1-D array for mono, or (channels, samples) for multi-channel.
    We bypass whisperx's built-in (torchcodec) decoder because it requires
    FFmpeg 4-7, while this machine ships FFmpeg 8.
    """
    cmd = [
        "ffmpeg", "-nostdin", "-threads", "0",
        "-i", path,
        "-f", "f32le", "-ac", str(channels), "-ar", str(sr), "-",
    ]
    try:
        proc = subprocess.run(cmd, capture_output=True, check=True)
    except FileNotFoundError:
        sys.exit("ffmpeg not found on PATH. Install it with: brew install ffmpeg")
    except subprocess.CalledProcessError as e:
        sys.exit(f"ffmpeg failed to decode {path!r}:\n{e.stderr.decode(errors='ignore')}")
    data = np.frombuffer(proc.stdout, np.float32)
    if channels > 1:
        data = data.reshape(-1, channels).T
    return data


def load_audio(path: str, sr: int = SAMPLE_RATE) -> np.ndarray:
    """Decode to mono float32 at the target sample rate (for transcription)."""
    return decode(path, sr=sr, channels=1)


def isolate_vocals(path: str) -> np.ndarray:
    """Separate the vocal stem with Demucs, returning mono float32 @ 16 kHz.

    Music source separation is the established preprocessing step for speech
    mixed under music (arXiv:2506.15514, arXiv:2008.12048): Whisper transcribes
    the clean vocal stem far better than the original mix.
    """
    import torch
    import torchaudio
    from demucs.apply import apply_model
    from demucs.pretrained import get_model

    model = get_model("htdemucs")
    model.eval()
    msr, mch = model.samplerate, model.audio_channels  # 44100, 2

    mix = torch.from_numpy(decode(path, sr=msr, channels=mch))  # (channels, samples)
    ref = mix.mean(0)
    mix_n = (mix - ref.mean()) / (ref.std() + 1e-8)
    with torch.no_grad():
        stems = apply_model(model, mix_n[None], device="cpu", split=True,
                            overlap=0.25, progress=False)[0]
    stems = stems * ref.std() + ref.mean()
    vocals = stems[model.sources.index("vocals")].mean(0)  # downmix to mono
    vocals16 = torchaudio.functional.resample(vocals, msr, SAMPLE_RATE)
    return vocals16.numpy().astype(np.float32)


def fmt_ts(seconds: float) -> str:
    """Seconds -> HH:MM:SS."""
    seconds = max(0, int(round(seconds)))
    h, rem = divmod(seconds, 3600)
    m, s = divmod(rem, 60)
    return f"{h:02d}:{m:02d}:{s:02d}"


def flatten_words(segments: list[dict]) -> list[dict]:
    """Flatten aligned segments into a single ordered list of words.

    Each word carries its own speaker (from diarization). Words missing a speaker
    or timestamp inherit from their segment / the previous word, so nothing is
    silently dropped. Segments without word-level data fall back to a single block.
    """
    words: list[dict] = []
    last_speaker = "UNKNOWN"
    last_end = 0.0
    for seg in segments:
        seg_speaker = seg.get("speaker")
        seg_words = seg.get("words") or []
        if not seg_words:
            text = (seg.get("text") or "").strip()
            if not text:
                continue
            spk = seg_speaker or last_speaker
            start = float(seg.get("start", last_end))
            end = float(seg.get("end", start))
            words.append({"text": text, "start": start, "end": end, "speaker": spk})
            last_speaker, last_end = spk, end
            continue
        for w in seg_words:
            tok = (w.get("word") or "").strip()
            if not tok:
                continue
            spk = w.get("speaker") or seg_speaker or last_speaker
            start = w.get("start")
            end = w.get("end")
            start = float(start) if start is not None else last_end
            end = float(end) if end is not None else start
            words.append({"text": tok, "start": start, "end": end, "speaker": spk})
            last_speaker, last_end = spk, end
    return words


_SENT_END = re.compile(r"[.!?]['\"”’\)\]]*$")


def split_sentences(words: list[dict]) -> list[list[dict]]:
    """Group an ordered word list into sentences using end punctuation."""
    sentences: list[list[dict]] = []
    cur: list[dict] = []
    for w in words:
        cur.append(w)
        if _SENT_END.search(w["text"]):
            sentences.append(cur)
            cur = []
    if cur:
        sentences.append(cur)
    return sentences


def build_turns(segments: list[dict]) -> list[dict]:
    """Group words into speaker turns at SENTENCE granularity.

    Per-word diarization labels are noisy (ASR word timestamps and diarization
    boundaries don't align to the word), so splitting per word chops sentences in
    half. Instead we use punctuation as anchors: each sentence is assigned ONE
    speaker by a duration-weighted majority vote of its words, then consecutive
    same-speaker sentences are merged into a turn. This is the standard
    whisper-diarization realignment approach and prevents mid-sentence speaker flips.
    """
    sentences = split_sentences(flatten_words(segments))
    sent_turns: list[dict] = []
    for sent in sentences:
        weights: dict[str, float] = {}
        for w in sent:
            weights[w["speaker"]] = weights.get(w["speaker"], 0.0) + max(w["end"] - w["start"], 1e-3)
        speaker = max(weights, key=weights.get)
        sent_turns.append({
            "speaker": speaker,
            "start": sent[0]["start"],
            "end": sent[-1]["end"],
            "text": " ".join(w["text"] for w in sent),
        })

    turns: list[dict] = []
    for s in sent_turns:
        if turns and turns[-1]["speaker"] == s["speaker"]:
            turns[-1]["text"] += " " + s["text"]
            turns[-1]["end"] = s["end"]
        else:
            turns.append(dict(s))
    for t in turns:
        # collapse whitespace and tidy spacing before punctuation
        t["text"] = re.sub(r"\s+([,.?!;:’'”\)])", r"\1", " ".join(t["text"].split()))
    return turns


def write_docx(turns: list[dict], out_path: Path, source_name: str, model_name: str) -> None:
    doc = Document()
    doc.add_heading(f"Transcript — {source_name}", level=0)
    speakers = sorted({t["speaker"] for t in turns})
    meta = doc.add_paragraph()
    run = meta.add_run(
        f"Model: {model_name}  ·  {len(turns)} turns  ·  {len(speakers)} speakers detected  ·  "
        "speaker labels are positional — relabel as needed"
    )
    run.italic = True
    run.font.size = Pt(9)
    run.font.color.rgb = RGBColor(0x80, 0x80, 0x80)

    for turn in turns:
        header = doc.add_paragraph()
        h = header.add_run(
            f"{turn['speaker']}  [{fmt_ts(turn['start'])} → {fmt_ts(turn['end'])}]"
        )
        h.bold = True
        h.font.size = Pt(11)
        body = doc.add_paragraph(turn["text"])
        body.paragraph_format.space_after = Pt(8)

    out_path.parent.mkdir(parents=True, exist_ok=True)
    doc.save(str(out_path))


def _json_default(o):
    if isinstance(o, np.floating):
        return float(o)
    if isinstance(o, np.integer):
        return int(o)
    if isinstance(o, np.ndarray):
        return o.tolist()
    return str(o)


def save_json(result: dict, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w") as f:
        json.dump({"segments": result.get("segments", [])}, f,
                  ensure_ascii=False, indent=2, default=_json_default)


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Transcribe an English podcast to a Word doc with speakers + timestamps.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("input", nargs="?",
                   help="Path to the audio file (mp3/wav/m4a/...). Omit when using --render-only.")
    p.add_argument("-o", "--output", help="Output .docx path. Default: out/<input>.docx")
    p.add_argument("--render-only", metavar="JSON",
                   help="Skip transcription; rebuild the .docx from a saved JSON sidecar.")
    p.add_argument("--model", default="large-v3",
                   help="Whisper model: large-v3, large-v2, distil-large-v3, medium, ...")
    p.add_argument("--language", default="en", help="Spoken language code.")
    p.add_argument("--device", default="cpu", choices=["cpu", "cuda"],
                   help="Compute device (CTranslate2 has no Metal backend, so cpu on Mac).")
    p.add_argument("--compute-type", default="int8",
                   help="faster-whisper compute type: int8, int8_float32, float32.")
    p.add_argument("--batch-size", type=int, default=8, help="Transcription batch size.")
    p.add_argument("--threads", type=int, default=8, help="CPU threads for transcription.")
    p.add_argument("--speakers", type=int, help="Exact number of speakers, if known.")
    p.add_argument("--min-speakers", type=int, help="Minimum speakers to detect.")
    p.add_argument("--max-speakers", type=int, help="Maximum speakers to detect.")
    # VAD thresholds. These are whisperx's defaults; testing showed that lowering
    # them mis-chunks the audio and can DROP speech (e.g. a caution voiced under
    # music) rather than recover it. Music-bed speech is recovered with
    # --isolate-vocals, not by VAD tuning. Left configurable for edge cases.
    p.add_argument("--vad-onset", type=float, default=0.500,
                   help="VAD onset threshold. whisperx default 0.500.")
    p.add_argument("--vad-offset", type=float, default=0.363,
                   help="VAD offset threshold. whisperx default 0.363.")
    p.add_argument("--isolate-vocals", action="store_true",
                   help="Run Demucs to strip music before transcribing. Use for speech "
                        "mixed under music (slower; downloads the htdemucs model once).")
    p.add_argument("--no-diarize", action="store_true",
                   help="Skip speaker diarization (transcript + timestamps only).")
    return p.parse_args()


def main() -> None:
    load_dotenv()
    args = parse_args()

    # --- Render-only: rebuild the doc from a saved JSON sidecar, no model needed. ---
    if args.render_only:
        json_path = Path(args.render_only).expanduser()
        if not json_path.exists():
            sys.exit(f"JSON sidecar not found: {json_path}")
        out_path = Path(args.output).expanduser() if args.output \
            else json_path.with_suffix(".docx")
        data = json.loads(json_path.read_text())
        turns = build_turns(data["segments"])
        log(f"Rendering {len(turns)} turns from {json_path.name} ...")
        write_docx(turns, out_path, json_path.stem, f"{args.model} (re-render)")
        log(f"Done -> {out_path}")
        return

    if not args.input:
        sys.exit("Provide an input audio file, or use --render-only JSON to rebuild a doc.")
    in_path = Path(args.input).expanduser()
    if not in_path.exists():
        sys.exit(f"Input file not found: {in_path}")
    out_path = Path(args.output).expanduser() if args.output \
        else Path("out") / (in_path.stem + ".docx")
    json_path = out_path.with_suffix(".json")

    # Import here so --help stays instant (whisperx import is heavy).
    import whisperx

    if args.isolate_vocals:
        log("Isolating vocals with Demucs (htdemucs) — this is the slow step ...")
        audio = isolate_vocals(str(in_path))
    else:
        log(f"Decoding {in_path.name} with ffmpeg ...")
        audio = load_audio(str(in_path))
    log(f"Audio: {len(audio) / SAMPLE_RATE / 60:.1f} min @ {SAMPLE_RATE} Hz")

    # On a clean (music-stripped) vocal stem we can keep more borderline speech
    # by raising no_speech_threshold; on a raw mix we keep whisperx's default.
    asr_options = {"no_speech_threshold": 0.85} if args.isolate_vocals else None

    log(f"Loading Whisper model '{args.model}' ({args.device}/{args.compute_type}) ...")
    model = whisperx.load_model(
        args.model, device=args.device, compute_type=args.compute_type,
        language=args.language, threads=args.threads,
        vad_options={"vad_onset": args.vad_onset, "vad_offset": args.vad_offset},
        asr_options=asr_options,
    )
    log(f"Transcribing (vad_onset={args.vad_onset}, vad_offset={args.vad_offset}) ...")
    result = model.transcribe(audio, batch_size=args.batch_size)

    log("Loading alignment model for word-level timestamps ...")
    align_model, metadata = whisperx.load_align_model(
        language_code=result.get("language", args.language), device=args.device
    )
    log("Aligning ...")
    result = whisperx.align(
        result["segments"], align_model, metadata, audio, args.device,
        return_char_alignments=False,
    )

    if args.no_diarize:
        log("Skipping diarization (--no-diarize).")
    else:
        token = os.getenv("HF_TOKEN")
        if not token:
            log("WARNING: HF_TOKEN not set — skipping diarization. "
                "See README for setup. Producing transcript without speakers.")
        else:
            log("Diarizing speakers (pyannote) ...")
            from whisperx.diarize import DiarizationPipeline
            diarizer = DiarizationPipeline(token=token, device=args.device)
            diarize_df = diarizer(
                audio,
                num_speakers=args.speakers,
                min_speakers=args.min_speakers,
                max_speakers=args.max_speakers,
            )
            result = whisperx.assign_word_speakers(diarize_df, result)

    log(f"Saving word-level result -> {json_path}")
    save_json(result, json_path)

    log("Building word-level speaker turns ...")
    turns = build_turns(result["segments"])
    log(f"{len(turns)} turns. Writing {out_path} ...")
    write_docx(turns, out_path, in_path.name, args.model)
    log(f"Done -> {out_path}  (re-render later with: --render-only {json_path})")


if __name__ == "__main__":
    main()
