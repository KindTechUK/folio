# Folio

_An open-source product by [KindTech](https://github.com/KindTechUK)._

**Private, on-device podcast & interview transcription.** A desktop app that turns audio
into a speaker-labelled, timestamped transcript — and **nothing ever leaves your
machine**. No cloud, no per-hour cost, no account.

```
SPEAKER_00  [00:00:12 -> 00:01:05]
Welcome back to the show. Today we're talking about...

SPEAKER_01  [00:01:05 -> 00:01:31]
Thanks for having me. So the big question is...
```

## Why it's different

- **Zero-egress by construction.** The shipped app and engine link **no networking
  crates** — enforced in CI by a denylist gate. Audio, transcripts, and telemetry stay
  local. See [Privacy & security](privacy-security.md).
- **CPU-only.** Runs on ordinary laptops (macOS, Windows, Linux) — no GPU required.
- **Signed model packs.** Models are distributed as signed packs and verified
  fail-closed before they are ever loaded. See [Model packs](model-packs.md).
- **Built-in editor.** Inline speaker renaming, undo/redo, find & replace, karaoke
  follow-along, and a persisted local transcript library. Export to `.docx`.

## Get started

1. **[Install](install.md)** the desktop app for your OS and import a model pack.
2. Open an audio file and transcribe — entirely offline.
3. Rename the positional speaker labels (`SPEAKER_00`, …) to real names in the editor.

Want to build it yourself or sign your own model packs? See
[Build from source](build-from-source.md) and [Model packs](model-packs.md).

## How it works

The app (Tauri v2, React 19 frontend) drives an in-process Rust engine built on
[`sherpa-onnx`](https://crates.io/crates/sherpa-onnx), CPU-only with no Python at runtime:

1. Audio is decoded to 16 kHz mono and chunked with **Silero VAD**.
2. **Whisper** (`small.en` / `medium.en`, int8) transcribes each chunk with word timings.
3. **pyannote segmentation-3.0 + 3D-Speaker CAM++** diarize and label speakers.
4. Consecutive same-speaker words are merged into turns and rendered in the editor.

## License

[Apache-2.0](https://github.com/billlyzhaoyh/kindtech-transcribe/blob/main/LICENSE).
