# Data at rest

> Part of the assurance pack. Covers how stored data is protected, temp-file
> hygiene, and disposal. Addresses PRD user stories #14 (delete matter) and #15
> (BitLocker warning).

## Encryption model: rely on the firm's BitLocker (documented dependency)

The product **does not implement application-level encryption** of stored
files. This is a deliberate design decision (PRD): the app relies on the firm's
**BitLocker full-disk encryption** on the managed Windows laptop, avoiding the
key-management liability and recovery-burden of a bespoke crypto layer. All
data the app touches — input audio (read in place), in-memory working data, and
any exports the user saves — therefore sits on the **BitLocker-encrypted
volume** the firm already operates and controls.

**This is a dependency, not a feature the vendor provides.** For the data-at-rest
protection to hold, the firm must ensure BitLocker is enabled on the laptops
the app runs on. The non-processor / on-device posture (see
`non-processor-statement.md`) is independent of BitLocker; BitLocker is what
protects the locally-stored data from device theft/loss.

### Launch-time BitLocker-status check — ✅ implemented (advisory)

The PRD specifies that the app checks BitLocker status on launch and **warns
the fee-earner if encryption is off**, so privileged data is not stored
unprotected (user story #15). **Implemented:** on launch the app calls the
`volume_encryption_status` command, which on Windows derives the drive and parses
`manage-bde -status`; unless the volume is confirmed encrypted it shows a
dismissible amber banner advising the matter be stored/run on an encrypted
volume (`app/src-tauri/src/system.rs`, `app/src/App.tsx`). The check **fails
soft** — if `manage-bde` cannot be run it reports `unknown` (and still warns),
never erroring the app; on non-Windows dev hosts it reports the Windows-only
`unknown` variant.

This is an **advisory** prompt, not an enforcement gate — the firm should still
enforce BitLocker through its device-management policy (Intune/SCCM) as the
primary control; the in-app banner is a backstop, not a substitute.

## Where data is stored

| Data | Location | Notes |
| --- | --- | --- |
| Input audio | Wherever the user keeps it | Read in place; not copied to a vendor-controlled location. |
| Working/decoded audio | **In memory** during a run | Current engine decodes to in-memory `f32` samples (`engine/src/decode.rs`); see temp-file note below. |
| **Saved transcripts (library)** | Per-user app-data dir, on the BitLocker volume | ✅ Each transcript is **auto-saved** as a local JSON file (`app_data/library/<id>.json`, `app/src-tauri/src/library.rs`) so past matters reappear on reopen. It stores the transcript text/edits + metadata and a **reference (path) to the source audio** — the audio itself is NOT copied into the library (one copy on disk). Purged by one-click delete-matter. Never leaves the device. |
| Models / model packs | Verified, signed packs only | ✅ The transcribe path loads weights ONLY from a cryptographically verified signed pack, extracted into a per-user content-addressed cache (`app_data/verified-models/`). Release/hardened builds always enforce this; a debug-only `KINDTECH_MODELS_DIR` override exists for the dev/demo. See `non-processor-statement.md` / `third-party-components.md`. |
| Exports (DOCX/JSON/text) | A local folder the user chooses | Expected to be on the BitLocker volume; carries the draft disclaimer. |

`fs` access is **scoped** by the Tauri capability set to the user data,
document, audio, desktop, and download directories
(`app/src-tauri/capabilities/default.json`) — the app cannot read/write
arbitrary system paths.

## Temp-file hygiene

**Intent (PRD):** working/temp files (audio decode scratch, model scratch) must
live on the encrypted volume in the app's **own** directory — never an arbitrary
system temp location — and be **wiped on completion**.

**Honest current state:** the implemented engine **decodes audio in memory**
(Symphonia → Rubato → `f32` samples) and does **not** write a separate decode
scratch file to disk (no `tempfile`/`temp_dir`/scratch writes appear in
`engine/src/`). So today there is **no decode temp file to leak**, which is a
stronger position than a wipe-on-completion routine. However:

- A formal, app-owned scratch directory with a guaranteed wipe-on-completion
  routine (the Vibe `cleaner.rs`-style sweep referenced in the fork plan) is
  **🚧 roadmap**, for any future code path that does spill to disk (e.g. very
  long inputs, or if a future engine path requires on-disk intermediates).
- Reviewers should treat "no temp files written today" as the verifiable fact,
  and "explicit app-owned scratch + wipe" as committed direction — not claim the
  wipe routine exists.

## Disposal: one-click "delete this matter" — ✅ implemented

The PRD specifies a one-click action to remove the audio and transcript for a
matter when the fee-earner is done (user story #14). **Implemented:** a "Delete
matter" button (shown once a transcript is loaded) calls the backend, which
resolves the saved source path, restricts it to approved user-media folders,
rejects escaping symlinks, and displays the canonical path in a native
confirmation dialog before deletion. It then **purges the saved library entry**
for that matter (`delete_session`) and clears the in-memory transcript
(`app/src-tauri/src/system.rs`, `library.rs`, `app/src/App.tsx`). The deletion
helper is idempotent (an already-gone file counts as deleted) and reports any
genuine failure rather than crashing.

**Honest scope:** the on-disk footprint of a matter is the **source audio** (the
user's own file, referenced not copied) plus the **saved transcript JSON** in the
library — delete-matter removes both. The engine writes no temp/scratch file
(in-memory decode, above) and there is no vendor-side copy. Exports the user
separately saved elsewhere are deleted by the user through the OS. A matter can
also be deleted individually from the Library view. Because the vendor never
receives the data, there is no vendor-side deletion to request or verify.

## Summary for the reviewer

- ✅ No app-level crypto by design; relies on firm BitLocker (documented
  dependency).
- ✅ `fs` access is capability-scoped, not system-wide.
- ✅ No decode temp/scratch file is written today (in-memory decode).
- ✅ Exports carry the machine-draft disclaimer.
- ✅ Transcripts are auto-saved to a local on-device library (app-data dir, on
  the BitLocker volume); the source audio is referenced by path, not copied.
- ✅ Launch-time BitLocker-status banner (advisory) and one-click delete-matter
  (removes the source audio + purges the saved library entry + clears the
  transcript) are implemented.
- ✅ Models load ONLY from cryptographically verified signed packs at
  transcription time (enforced in release/hardened builds).
- 🚧 An explicit app-owned scratch dir with wipe-on-completion remains roadmap
  for any future code path that spills intermediates to disk (none does today).
