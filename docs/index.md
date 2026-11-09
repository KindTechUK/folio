# Folio

_An open-source product by [KindTech](https://github.com/KindTechUK)._

**Private, fully-local transcription and human review for sensitive recordings.** A
desktop app that turns audio into a timestamped, editable transcript (optionally
speaker-labelled) — and **nothing ever leaves your machine**. No cloud, no per-hour cost,
no account.

## Why it's different

- **Zero-egress by construction.** The shipped app and engine link **no networking
  crates** — enforced in CI by a denylist gate. Audio, transcripts, and telemetry stay
  local. See [Privacy & security](privacy-security.md).
- **CPU-only.** Runs on ordinary laptops (macOS on Apple Silicon, Windows x64, Linux x64)
  — no GPU required.
- **Signed model packs.** Models are distributed as signed packs and verified
  fail-closed before they are ever loaded. See [Model packs](model-packs.md).
- **Built-in editor.** Sentence/paragraph editing, inline speaker renaming, undo/redo,
  find & replace, karaoke follow-along, and a persisted local transcript library. Export
  to DOCX, JSON, or plain text.

## Get started

1. **[Install](install.md)** the desktop app for your OS and import a model pack.
2. Open an audio file and transcribe — entirely offline.
3. Review and correct the draft in the editor. If you enabled *Detect and label
   speakers*, rename the positional labels (`SPEAKER_00`, …) to real names.

Want to build it yourself or sign your own model packs? See
[Build from source](build-from-source.md) and [Model packs](model-packs.md).

## How it works

The app (Tauri v2, React 19 frontend) drives an in-process Rust engine built on
[`sherpa-onnx`](https://crates.io/crates/sherpa-onnx), CPU-only with no Python at runtime:

1. Audio is decoded to 16 kHz mono and chunked with **Silero VAD**.
2. **Parakeet-TDT-0.6B-v2** (default) or **Whisper** `small.en` / `medium.en` (int8)
   transcribes each chunk with word timings.
3. Optionally (off by default), **pyannote segmentation-3.0 + 3D-Speaker CAM++** diarize
   and label speakers.
4. Words are grouped into paragraphs (or same-speaker turns) and rendered in the editor.

## License

[Apache-2.0](https://github.com/KindTechUK/folio/blob/main/LICENSE).
