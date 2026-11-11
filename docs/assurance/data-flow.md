# Data-flow description

> Part of the assurance pack. Gives a DPO the data-flow diagram and the
> "data never leaves the device" statement.

## Summary

All processing of audio and all derived data (transcript, speaker labels,
exports) happens **on the fee-earner's device**. **Nothing is transmitted to
the vendor or to any third party.** There is no telemetry, no crash reporting,
no analytics, no update ping, and no cloud feature. The application makes
**zero outbound network connections** — see `airplane-mode-verification.md` for
how to confirm this independently.

## The data flow, end to end

```mermaid
flowchart LR
    A["Audio file<br/>(local disk, on the<br/>BitLocker volume)"]
      --> B["In-app decode<br/>Symphonia + Rubato<br/>→ 16 kHz mono f32<br/>(in memory, no ffmpeg)"]
    M["Microphone<br/>(Record page)"]
      --> R["Recorder: 16 kHz mono<br/>WAV in app data<br/>+ live text per utterance"]
    R --> C
    B --> C["ASR (Parakeet-TDT-0.6B-v2 or Whisper, int8)<br/>via sherpa-onnx, in-process, CPU"]
    C --> D{"Diarization?<br/>(opt-in, default OFF)"}
    D -- "yes" --> E["Speaker diarization<br/>pyannote-seg-3.0 + CAM++<br/>via sherpa-onnx, in-process"]
    D -- "no" --> F
    E --> F["Transcript JSON contract<br/>{segments:[{start,end,text,speaker,words[]}]}"]
    F --> G["Review editor<br/>(karaoke highlight,<br/>sentence edit, relabel)"]
    G --> H["Exports: DOCX / JSON / text<br/>each stamped with the<br/>machine-draft disclaimer"]
    H --> I["Saved locally<br/>(DOCX: save dialog;<br/>JSON/text: webview download)"]

    subgraph DEVICE["The fee-earner's laptop — the trust boundary"]
        A
        M
        R
        B
        C
        D
        E
        F
        G
        H
        I
    end

    X(["Vendor / cloud / any<br/>network endpoint"])
    DEVICE -. "NO CONNECTION — networking absent by construction" .-x X
```

Equivalent ASCII view:

```
 audio file                                                      exports
 (local, on        decode        ASR        (opt) diarize       (DOCX/JSON/
  BitLocker)  -->  in-memory --> Parakeet/ --> sherpa-onnx    --> text, with
      |            Symphonia     Whisper     pyannote+CAM++       draft notice)
      |            + Rubato      int8 CPU    in-process               |
      |                                                              v
      |                          all in-process, CPU-only      saved locally
      |                          on THIS device                on the encrypted
      |                                                         volume
      +------------------------------------------------------------+
                                |
        =========================================================
        ||  TRUST BOUNDARY = the device. Nothing crosses it.   ||
        ||  No HTTP client linked. CSP connect-src 'none'.     ||
        =========================================================
                                |  (X)  no connection — by construction
                                v
                    vendor / cloud / any endpoint
```

## Stage-by-stage

1. **Audio input.** The user opens a local audio file via the OS file picker
   (`tauri-plugin-dialog`, scoped). The file is read from local disk only.
   ✅ implemented.
2. **Decode.** The file is decoded to 16 kHz mono in-process with the
   pure-Rust `symphonia` (demux/decode) + `rubato` (resample) crates — **no
   bundled ffmpeg, no sidecar, no network**. Decoded samples are held in
   memory (`engine/src/decode.rs`). ✅ implemented.
3. **ASR.** NVIDIA Parakeet-TDT-0.6B-v2 (int8, the default) or Whisper
   `small.en` / `medium.en` (int8) runs in-process on CPU via the official
   k2-fsa `sherpa-onnx` crate, chunked by Silero VAD (`engine/src/sherpa.rs`).
   ✅ implemented.
4. **Diarization (opt-in, default OFF).** When the user opts in, speaker
   diarization runs in-process (pyannote-segmentation-3.0 ONNX + 3D-Speaker
   CAM++ embedding via `sherpa-onnx`). The default flow produces plain
   pause-based paragraphs with no speaker labels. ✅ implemented.
5. **Transcript assembly + review.** The engine emits the JSON transcript
   contract; the review editor renders it for karaoke playback, sentence-level
   correction, and speaker relabelling. ✅ implemented (`core/`, `app/src`).
6. **Export.** The user exports DOCX, JSON, or plain text. DOCX is written to
   a path the user picks in a native save dialog; JSON and text are handed to
   the webview as a browser-style download, so where they land (typically the
   Downloads folder, or a prompt) depends on the OS webview. Each export
   carries the machine-generated-draft disclaimer
   (`app/src/draft.ts`, `app/src/docx.ts`). DOCX is built in-process by the
   `docx` npm package — no network. ✅ implemented.

At **no** stage is any byte sent off the device.

## Where the data lives

- **Input audio**: wherever the user already keeps it (read in place).
- **Recordings made in Folio** (Record page): one WAV per recording in
  `app_data/library/recordings/`, on the BitLocker volume, written as the
  conversation happens. The live transcript is produced in memory, utterance
  by utterance, by the same in-process engine. See `data-at-rest.md`.
- **Working data**: held in memory during a transcription run. The current
  engine decodes audio in memory and does not write a separate scratch/temp
  file to disk (`engine/src/decode.rs`); see `data-at-rest.md` for the honest
  status of the "wipe temp files on completion" intent.
- **Saved transcripts**: each transcript is auto-saved to a local library
  (`app_data/library/`, on the BitLocker volume) so past transcripts reopen — it
  stores the transcript/edits + a path reference to the source audio (the audio
  is not copied). It never leaves the device; one-click "Delete transcript and audio" purges it.
  See `data-at-rest.md`.
- **Models**: at transcription time, weights are loaded **ONLY from a
  cryptographically verified signed pack** (ed25519 + sha256, fail-closed),
  extracted into a per-user content-addressed cache. Release builds
  always enforce this; a debug-only `KINDTECH_MODELS_DIR` override exists for the
  dev/demo. See `app/src-tauri/src/commands.rs`, `models.rs`. 🚧 Installer
  bundling of a default signed pack remains roadmap (packs are imported once
  via the Models screen).
- **Exports**: DOCX to the path chosen in the save dialog; JSON/text to the
  webview's download location (typically the user's Downloads folder). All are
  expected to be on the firm's **BitLocker-encrypted** volume (see
  `data-at-rest.md`).

All of these are on the firm's own managed, encrypted laptop. The
vendor has no copy and no access.

## Categories of data involved

The audio can contain **special-category and privileged personal data** (client
interviews, dictation, meetings). The transcript and speaker labels are derived
personal data. Because none of it leaves the device, the recipient set for this
processing is **only the firm itself** — there is no third-party recipient and
no international transfer. See `non-processor-statement.md`.
