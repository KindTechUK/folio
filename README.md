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
  installer pushed by IT (installer code-signing is on the roadmap).
- Because the vendor never receives any data, the vendor's position is that it
  is **not a data processor** under UK GDPR / DPA 2018.

The full reviewer-facing evidence — data-flow diagram, non-processor reasoning,
independently runnable airplane-mode verification, component/licence inventory,
data-at-rest notes, and a DPIA-support template — lives in
[`docs/assurance/`](docs/assurance/README.md).

## Documentation & downloads

- **Docs site:** <https://kindtechuk.github.io/folio/> — install
  guide, model packs, building from source, and the privacy/assurance pack.
- **Downloads:** signed model packs and (not yet code-signed) desktop
  installers — macOS Apple Silicon `.dmg`, Windows x64 `-setup.exe`, Linux x64
  `.AppImage` / `.deb` — are published to
  [GitHub Releases](https://github.com/KindTechUK/folio/releases).
- **Contributing & security:** see [CONTRIBUTING.md](CONTRIBUTING.md),
  [SECURITY.md](SECURITY.md), and the [Apache-2.0 licence](LICENSE).

## What it does

- **In-app decode** to 16 kHz mono via pure-Rust `symphonia` + `rubato` — no
  bundled ffmpeg, no sidecar.
- **In-process ASR on CPU** via the official k2-fsa `sherpa-onnx` crate:
  NVIDIA **Parakeet-TDT-0.6B-v2** (int8, English, native word timestamps) by
  default, or Whisper `small.en` / `medium.en` (int8) when only those packs are
  installed. **No Python at runtime.**
- **Record in the app**: capture a conversation from the microphone and watch
  the transcript build as people speak (a few seconds behind), with pause and
  discard. The recording is saved as a WAV on the device, written so a crash
  keeps all but the last few seconds, and opens in the editor when you stop.
- **Optional speaker diarization** (opt-in, default off — see
  [`engine/README.md`](engine/README.md)), with an optional speaker-count hint.
- **Review editor**: variable-speed playback with 5-second skip, word-synced
  highlight + follow, click-to-seek, sentence/paragraph editing, inline speaker
  rename, undo/redo, find & replace.
- **Transcripts**: every transcript is auto-saved on-device so past transcripts
  reopen; one-click **Delete transcript and audio** purges the transcript and
  source audio.
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
| `docs/` | The docs site source, including the compliance / assurance pack (`docs/assurance/`). |
| `scripts/` | Dev/CI helpers: the zero-egress gate, model-pack release build, manual e2e harness. |
| `fixtures/` | Test-only model-pack fixtures and keys for the conformance tests. |

## Build & run the app (development)

Prerequisites: Node.js 22+ and npm, the Rust toolchain, and Tauri's platform
dependencies. The first build downloads the prebuilt `sherpa-onnx` C library;
set `SHERPA_ONNX_LIB_DIR` only for an offline/vendored build (see
[`engine/README.md`](engine/README.md)).

```bash
cd app
npm ci
npm run tauri dev
```

A debug-only `KINDTECH_MODELS_DIR` env var can point the transcribe path at a
raw models directory for dev/demo; release builds ignore it and load
only from verified signed packs.

### Tests & checks

```bash
(cd core && npm ci && npm run verify)                    # typecheck + unit tests
(cd app && npm ci && npm run build && npx vitest run)    # tsc build + component tests
cargo test --manifest-path app/src-tauri/Cargo.toml      # app backend (needs app/dist from the line above)
bash scripts/check-no-network.sh                         # zero-egress denylist gate
```

See [Build from source](docs/build-from-source.md) for every component.

## Status

Pre-release. The trust boundary, zero-egress enforcement, review editor, and
local Library are implemented and tested. Release packs are signed with a
CI-held vendor key, and release builds refuse to compile against the committed
test key. **Roadmap**: Windows MSI packaging with OV Authenticode code-signing
for IT-managed (Intune/SCCM) deployment.
