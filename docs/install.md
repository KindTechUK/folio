# Install

## 1. Download the app

Grab the installer for your OS from the
[**Releases**](https://github.com/KindTechUK/folio/releases) page:

| OS | Asset |
|----|-------|
| macOS (Apple Silicon) | `Folio_<version>_aarch64.dmg` |
| Windows x64 | `Folio_<version>_x64-setup.exe` (NSIS) |
| Linux x64 | `Folio_<version>_amd64.AppImage` or `Folio_<version>_amd64.deb` |

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

| Pack | Use it for | Download |
|------|-----------|----------|
| `asr-parakeet-tdt-0.6b-v2` | Transcription — **default, recommended** (English, fastest on CPU) | ~480 MB |
| `asr-whisper-small.en` | Transcription (Whisper alternative) | ~230 MB |
| `asr-whisper-medium.en` | Transcription (Whisper, slower, more accurate than small.en) | ~585 MB |
| `diarization-pyannote-campplus` | Speaker labels (optional) | ~33 MB |

The Parakeet pack is published from v0.2.0 onwards. The app uses Parakeet when it is
installed, otherwise `small.en`, then `medium.en`.

Then:

1. Unzip the pack(s) — each becomes a folder containing `manifest.json` + `weights.bin`.
2. Open the app's **Models** screen and **Import** each folder.
3. The app verifies the pack (signature + checksum + engine-version gate) and shows
   ✓ **imported** — or a rejection reason if it fails. Nothing is installed unless it
   verifies.

You need **one ASR pack** (Parakeet recommended). Add the diarization pack only if you
will use *Detect and label speakers*. Verify downloads against `SHA256SUMS` from the
release:

```bash
sha256sum -c --ignore-missing SHA256SUMS            # Linux / Windows (Git Bash)
shasum -a 256 -c --ignore-missing SHA256SUMS        # macOS
```

## 3. Transcribe

Open an audio file and transcribe. To label speakers, tick *Detect and label speakers*
(optionally giving the expected number of speakers). Everything runs
locally on CPU — you can do it in airplane mode. See
[Privacy & security](privacy-security.md) for the guarantees, or
[Model packs](model-packs.md) to build your own.
