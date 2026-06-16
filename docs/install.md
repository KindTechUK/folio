# Install

## 1. Download the app

Grab the installer for your OS from the
[**Releases**](https://github.com/billlyzhaoyh/kindtech-transcribe/releases) page:

| OS | Asset |
|----|-------|
| macOS | `Folio_<version>_*.dmg` |
| Windows | `Folio_<version>_*-setup.exe` (NSIS) |
| Linux | `*.AppImage` or `*.deb` |

### First-launch notes (unsigned builds)

The releases are currently **not code-signed/notarized**, so the OS will warn on first
launch. This is expected.

=== "macOS"

    Gatekeeper blocks unsigned apps. Either right-click the app → **Open** → **Open**, or
    clear the quarantine attribute:

    ```bash
    xattr -dr com.apple.quarantine "/Applications/Folio.app"
    ```

=== "Windows"

    SmartScreen shows "Windows protected your PC". Click **More info → Run anyway**.

=== "Linux"

    Make the AppImage executable: `chmod +x *.AppImage`, then run it. The `.deb`
    installs with `sudo apt install ./*.deb`.

## 2. Import a model pack

The app ships **without** model weights — you import a signed pack once. From the same
release, download the packs you need (each is a `.zip` of `manifest.json` + `weights.bin`):

| Pack | Use it for |
|------|-----------|
| `asr-whisper-small.en` | Transcription (faster, ~370 MB) |
| `asr-whisper-medium.en` | Transcription (more accurate, ~930 MB) |
| `diarization-pyannote-campplus` | Speaker labels (~35 MB) |

Then:

1. Unzip the pack(s) — each becomes a folder containing `manifest.json` + `weights.bin`.
2. Open the app's **Models** screen and **Import** each folder.
3. The app verifies the pack (signature + checksum + engine-version gate) and shows
   ✓ **imported** — or a rejection reason if it fails. Nothing is installed unless it
   verifies.

You typically want **one ASR pack + the diarization pack**. Verify downloads against
`SHA256SUMS` from the release:

```bash
sha256sum -c SHA256SUMS
```

## 3. Transcribe

Open an audio file, optionally set the speaker count, and transcribe. Everything runs
locally on CPU — you can do it in airplane mode. See
[Privacy & security](privacy-security.md) for the guarantees, or
[Model packs](model-packs.md) to build your own.
