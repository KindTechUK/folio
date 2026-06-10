# podcast-transcribe

Local, private English-podcast transcription on Apple Silicon. Produces a **Word
document** where every speaker turn is a block with start/end timestamps:

```
SPEAKER_00  [00:00:12 -> 00:01:05]
Welcome back to the show. Today we're talking about...

SPEAKER_01  [00:01:05 -> 00:01:31]
Thanks for having me. So the big question is...
```

Built on **WhisperX** (`whisper-large-v3` for transcription + forced alignment for
accurate word timestamps + **pyannote** for speaker diarization). Everything runs
locally -- no audio leaves your machine, no per-hour cost.

Speaker labels are **positional** (`SPEAKER_00`, `SPEAKER_01`, ...). You relabel them
to real names afterward (see [Relabeling speakers](#relabeling-speakers)).

## Requirements

- macOS (Apple Silicon recommended) with [`uv`](https://docs.astral.sh/uv/) and `ffmpeg`
  (`brew install ffmpeg`).
- A free Hugging Face account + token (for diarization only).

## Setup

```bash
cd ~/podcast-transcribe
uv sync                 # creates a Python 3.11 venv and installs everything
```

The first transcription run downloads ~3-4 GB of models (cached afterward under
`~/.cache/huggingface` and `~/.cache/torch`).

### Speaker diarization setup (one-time)

Diarization uses a gated pyannote model, so you must authorize it once:

1. Create a token (read scope) at <https://hf.co/settings/tokens>.
2. Accept the model terms (click *Agree* on each page while logged in):
   - <https://hf.co/pyannote/speaker-diarization-community-1>
   - <https://hf.co/pyannote/segmentation-3.0>
3. Copy the token into a `.env` file:
   ```bash
   cp .env.example .env
   # edit .env and paste your hf_... token
   ```

> If `HF_TOKEN` is missing or the terms aren't accepted, the pipeline still runs and
> produces a timestamped transcript -- just without speaker labels.

## Usage

```bash
# Basic (auto-detect number of speakers)
uv run transcribe.py episode.mp3

# Specify output path and exact speaker count (improves diarization)
uv run transcribe.py episode.mp3 -o out/episode.docx --speakers 2

# Bound the speaker count instead of fixing it
uv run transcribe.py episode.mp3 --min-speakers 2 --max-speakers 4

# Faster, slightly less accurate model
uv run transcribe.py episode.mp3 --model distil-large-v3

# Transcript + timestamps only, no diarization
uv run transcribe.py episode.mp3 --no-diarize

# Recover speech mixed under music (strips music with Demucs first)
uv run transcribe.py episode.mp3 --isolate-vocals

# Re-render the doc from a saved JSON sidecar (no re-transcription)
uv run transcribe.py --render-only out/episode.json -o out/episode.docx
```

Output defaults to `out/<input-filename>.docx`, alongside a `.json` sidecar holding
the full word-level result (used by `--render-only`).

### Useful flags

| Flag | Default | Notes |
|------|---------|-------|
| `--model` | `large-v3` | `large-v2` (sometimes fewer hallucinations), `distil-large-v3` (~2-3x faster) |
| `--language` | `en` | Forcing the language is faster and more accurate than auto-detect |
| `--speakers N` | -- | Exact speaker count, when known |
| `--min-speakers` / `--max-speakers` | -- | Bounds when the count is uncertain |
| `--batch-size` | `8` | Lower if you hit memory pressure |
| `--compute-type` | `int8` | `float32` for max precision (slower, more RAM) |
| `--no-diarize` | off | Skip speaker tagging |
| `--isolate-vocals` | off | Strip music with Demucs before transcribing (see below) |
| `--vad-onset` / `--vad-offset` | `0.500` / `0.363` | whisperx defaults; lowering them mis-chunks audio and can *drop* speech — leave as-is |
| `--render-only JSON` | -- | Rebuild the `.docx` from a saved sidecar, no re-transcription |

### Speech mixed under music (`--isolate-vocals`)

Whisper drops or garbles speech that's mixed **under music** (dramatic voiceovers,
intros/outros, archive clips). VAD tuning can't fix this — the music has to be removed
first. `--isolate-vocals` runs [Demucs](https://github.com/facebookresearch/demucs)
(`htdemucs`) to extract the vocal stem, then transcribes that. This reliably recovers
music-bed speech (e.g. a voiced-over police caution) and is **neutral on clean speech**.

Trade-offs: it's the slow step (Demucs runs on CPU here and roughly doubles total
runtime on a full episode) and downloads the htdemucs model once (~80 MB). Leave it
**off** for ordinary clean-audio podcasts; turn it **on** for documentary-style audio
with heavy music beds. Note: even with clean vocal stems, Whisper still struggles with
heavily dramatized/low-fidelity passages, so recovered text may need light hand-editing.

## Performance notes

- On an M4 Pro, `large-v3` transcription runs roughly at real time (a 60-min episode
  takes on the order of an hour for transcription+alignment+diarization; the first run
  is slower due to model downloads).
- Transcription runs on **CPU**: the faster-whisper backend (CTranslate2) has no Apple
  Metal backend. Diarization also runs on CPU here for reliability.
- For a quick check, trim a clip first:
  ```bash
  ffmpeg -i episode.mp3 -t 120 sample.mp3
  uv run transcribe.py sample.mp3 --speakers 2
  ```

## Relabeling speakers

Open the `.docx` in Word/Pages/Google Docs and use Find & Replace:
`SPEAKER_00` -> `Alice`, `SPEAKER_01` -> `Bob`, etc. Listen to the first line of each
speaker's earliest turn to identify who's who.

## How it works

1. `ffmpeg` decodes the audio to 16 kHz mono (bypasses whisperx's torchcodec decoder,
   which is incompatible with FFmpeg 8).
2. WhisperX transcribes with `large-v3` (VAD-segmented to reduce hallucination).
3. A wav2vec2 alignment model produces precise word-level timestamps.
4. pyannote diarizes the audio and labels each word with a speaker.
5. Consecutive same-speaker segments are merged into turns and written to `.docx`.

## Troubleshooting

- **Diarization auth error / 401 / gated repo**: token missing, or you haven't accepted
  the model terms -- redo [diarization setup](#speaker-diarization-setup-one-time).
- **`ffmpeg not found`**: `brew install ffmpeg`.
- **Out of memory**: lower `--batch-size`, or use `--model distil-large-v3`.
