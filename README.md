# Folio

_An open-source product by [KindTech](https://github.com/KindTechUK)._

**Private, fully-local transcription and human review for organisations that
handle sensitive recordings.** Audio and transcripts are processed entirely on
the user's device and **never leave it** — there is no cloud, no telemetry, and
no network access of any kind.

Built for contexts where confidentiality is the whole point:

- **Legal** — privileged client interviews, dictation, and calls.
- **Charity / social sector** — recordings involving vulnerable individuals,
  under tight data-protection and retention obligations.
- Any UK-GDPR-sensitive setting where audio must not go to a third-party
  transcription service.

The reviewer edits a machine-generated **draft** — fast playback with a
word-synced karaoke cursor, sentence-level correction, and speaker relabelling —
so a human verifies every transcript before it is relied on.

## Privacy posture (the point of the product)

- **Zero outbound network by construction**, not by configuration: no HTTP
  client or networked plugin is linked, a strict CSP forbids the webview from
  opening any connection, and a CI gate fails the build if a networking crate
  reappears.
- **No telemetry, no crash reporting, no auto-updater.** Updates ship as a new
  signed installer pushed by IT.
- Because the vendor never receives any data, the vendor's position is that it
  is **not a data processor** under UK GDPR / DPA 2018.

The full reviewer-facing evidence — data-flow diagram, non-processor reasoning,
independently runnable airplane-mode verification, component/licence inventory,
data-at-rest notes, and a DPIA-support template — lives in
[`docs/assurance/`](docs/assurance/README.md).

## Documentation & downloads

- **Docs site:** <https://billlyzhaoyh.github.io/kindtech-transcribe/> — install
  guide, model packs, building from source, and the privacy/assurance pack.
- **Downloads:** signed model packs and desktop installers (macOS / Windows /
  Linux) are published to
  [GitHub Releases](https://github.com/billlyzhaoyh/kindtech-transcribe/releases).
- **Contributing & security:** see [CONTRIBUTING.md](CONTRIBUTING.md),
  [SECURITY.md](SECURITY.md), and the [Apache-2.0 licence](LICENSE).

## What it does

- **In-app decode** to 16 kHz mono via pure-Rust `symphonia` + `rubato` — no
  bundled ffmpeg, no sidecar.
- **In-process ASR on CPU** (Whisper `small.en`, int8) via the official k2-fsa
  `sherpa-onnx` crate. **No Python at runtime.**
- **Optional speaker diarization** (opt-in, default off — see
  [`engine/README.md`](engine/README.md)), with an optional speaker-count hint.
- **Review editor**: variable-speed playback, word-synced highlight + follow,
  click-to-seek, sentence/paragraph editing, inline speaker rename, undo/redo,
  find & replace.
- **Local Library**: every transcript is auto-saved on-device so past matters
  reopen; one-click **delete-matter** purges the transcript and source audio.
- **Exports**: DOCX / JSON / plain text, each stamped with a machine-draft
  disclaimer.
- **Signed model packs**: weights load only from a cryptographically verified
  (ed25519 + sha256, fail-closed) pack at transcription time.
- **BitLocker advisory** banner if the volume is not confirmed encrypted.

## Repository layout

| Path | What it is |
|------|------------|
| `app/` | The product: Tauri v2 + React desktop app (the shell, review editor, Library, Models views). |
| `core/` | Framework-independent TypeScript: transcript assembly, diarization mapping, the editor reducer, and the model-pack verification oracle (vitest). |
| `engine/` | Rust ASR + diarization engine over `sherpa-onnx` (CPU, no Python). |
| `modelpack/` | Rust model-pack signature/verification crate (mirrors the `core` oracle). |
| `tools/modelpack-sign/` | Vendor-side CLI to build and sign model packs. |
| `docs/assurance/` | The compliance / assurance pack. |
| `transcribe.py` (+ `pyproject.toml`) | **Legacy** macOS Python batch pipeline (see below). |

## Build & run the app (development)

Prerequisites: Node.js + npm, the Rust toolchain, and the `sherpa-onnx` native
libraries (set `SHERPA_ONNX_LIB_DIR` to point at them — see
[`engine/README.md`](engine/README.md)).

```bash
cd app
npm install
# point at your sherpa-onnx libs, then:
SHERPA_ONNX_LIB_DIR=/path/to/sherpa-onnx/lib npm run tauri dev
```

A debug-only `KINDTECH_MODELS_DIR` env var can point the transcribe path at a
raw models directory for dev/demo; release/hardened builds ignore it and load
only from verified signed packs.

### Tests & checks

```bash
cd core && npm run verify          # typecheck + 110 unit tests
cd app  && npm run build && npx vitest run   # tsc strict build + component tests
SHERPA_ONNX_LIB_DIR=... cargo test --manifest-path app/src-tauri/Cargo.toml
bash scripts/check-no-network.sh   # zero-egress denylist gate
```

## Status

Pre-release. The trust boundary, zero-egress enforcement, review editor, and
local Library are implemented and tested. **Roadmap**: a production vendor
signing key (fixtures use a committed TEST key), and Windows MSI packaging with
OV Authenticode code-signing for IT-managed (Intune/SCCM) deployment.

---

## Legacy: Python batch pipeline (`transcribe.py`)

The original macOS-only command-line pipeline predates the app and remains in
the repo for batch transcription on Apple Silicon. It is **separate** from the
Folio product above and uses a different stack (WhisperX +
`pyannote`, producing a Word document). It requires `uv`, `ffmpeg`, and — for
diarization — a Hugging Face token for the gated `pyannote` models.

```bash
uv sync
uv run transcribe.py episode.mp3 -o out/episode.docx --speakers 2
uv run transcribe.py episode.mp3 --no-diarize          # transcript only
uv run transcribe.py episode.mp3 --isolate-vocals      # strip music first (Demucs)
uv run transcribe.py --render-only out/episode.json -o out/episode.docx
```

Run `uv run transcribe.py --help` for the full flag list. Note: `--isolate-vocals`
recovers speech buried under music (Demucs `htdemucs`) and is the fix for
music-bed speech; do **not** tune `--vad-onset`/`--vad-offset` to chase it, as
that mis-chunks audio and can drop speech.
