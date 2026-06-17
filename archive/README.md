# archive/ — retired Python pipeline

These files are **no longer part of Folio** (the Tauri desktop app + Rust engine).
They are the original macOS-only Python batch pipeline and a few helper scripts that
predate the app, kept here for reference and reproducibility.

| File | What it was |
|------|-------------|
| `transcribe.py` | WhisperX + pyannote CLI → speaker-labelled `.docx` (the original product). |
| `clean_vtt.py` | Tidy a `.vtt` caption file into plain text. |
| `compare_elevenlabs.py` | Compare the local pipeline against ElevenLabs Scribe output. |
| `karaoke_editor.html` | Standalone interaction prototype for the review editor (superseded by `app/`). |
| `pyproject.toml`, `uv.lock`, `.python-version` | Python project metadata for the above. |
| `.env.example` | Hugging Face / ElevenLabs token template for the Python scripts. |

## Running the legacy pipeline (if you still need it)

Everything stays self-contained in this directory:

```bash
cd archive
cp .env.example .env          # add your hf_... token for diarization
uv sync
uv run transcribe.py episode.mp3 -o out/episode.docx --speakers 2
```

> Unlike Folio, this pipeline **downloads models from Hugging Face on first run** and
> is **not** covered by the zero-egress guarantee. Use Folio (the app) for private,
> on-device transcription.
