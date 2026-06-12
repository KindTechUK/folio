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

### Launch-time BitLocker-status check — 🚧 roadmap

The PRD specifies that the app checks BitLocker status on launch and **warns
the fee-earner if encryption is off**, so privileged data is not stored
unprotected (user story #15). **This check is not yet implemented in the
current code** — it is committed roadmap (see `docs/plans/vibe-fork-plan.md`
§10, step 12). Until it ships, the firm should enforce BitLocker through its
existing device-management policy (Intune/SCCM) rather than relying on an in-app
prompt.

## Where data is stored

| Data | Location | Notes |
| --- | --- | --- |
| Input audio | Wherever the user keeps it | Read in place; not copied to a vendor-controlled location. |
| Working/decoded audio | **In memory** during a run | Current engine decodes to in-memory `f32` samples (`engine/src/decode.rs`); see temp-file note below. |
| Models / model packs | Local models dir | Dev: repo `models/` via `PODCAST_MODELS_DIR`. 🚧 Production: installer-bundled default pack + per-user data dir for signed packs. |
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

## Disposal: one-click "delete this matter" — 🚧 roadmap

The PRD specifies a one-click action to remove the audio and transcript for a
matter when the fee-earner is done (user story #14). **This is not yet
implemented.** Until it ships, disposal is manual: the user deletes the input
audio and any exported files through the OS, and (because there is no
vendor-side copy and no automatic temp spill) nothing else persists. Because the
vendor never receives the data, there is no vendor-side deletion to request or
verify.

## Summary for the reviewer

- ✅ No app-level crypto by design; relies on firm BitLocker (documented
  dependency).
- ✅ `fs` access is capability-scoped, not system-wide.
- ✅ No decode temp/scratch file is written today (in-memory decode).
- ✅ Exports carry the machine-draft disclaimer.
- 🚧 Launch-time BitLocker check, an explicit app-owned scratch dir with
  wipe-on-completion, and one-click delete-matter are roadmap, not present.
