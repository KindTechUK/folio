# DPIA-support template

> Part of the assurance pack. Gives a DPO material to complete a DPIA / risk
> file.
>
> **This is a template of vendor-provided answers to the questions a firm DPO
> typically asks. It supports — it does not replace — the firm's own DPIA. The
> firm is the controller and owns the assessment. Answers are confirmed by the
> rest of this pack; status tags (✅ / 🚧) flag what is verifiable today.**

## A. Processing description

| DPO question | Vendor-supplied answer |
| --- | --- |
| What does the processing do? | Transcribes (and optionally diarizes) audio files into a reviewable, editable text transcript, entirely on the user's device. |
| What personal data is involved? | Audio of individuals (potentially special-category and legally privileged), and the derived transcript and speaker labels. |
| Whose data? | Data subjects recorded in the firm's audio (clients, witnesses, staff). |
| Is the data special-category / privileged? | Likely yes. Treat accordingly. The product is designed for exactly this sensitivity. |
| Volume / scale? | Per-matter, single-user, on one laptop at a time. No central aggregation, no multi-tenant store. |

## B. Necessity, proportionality, lawful basis

| DPO question | Vendor-supplied answer |
| --- | --- |
| What is the lawful basis? | **The firm determines this** (typically performance of the retainer / legitimate interests / legal claims). The vendor does not set or rely on a lawful basis — it never processes the data. |
| Is processing necessary / proportionate? | The on-device design is the *minimising* option vs cloud transcription: no data leaves the device, so the processing footprint is the smallest available for the purpose. |
| Could less data achieve the aim? | Diarization is **opt-in (default off)**; the default flow produces plain paragraphs with no speaker inference, minimising derived data unless the user opts in. ✅ |

## C. Recipients, transfers, processors

| DPO question | Vendor-supplied answer |
| --- | --- |
| Who receives the data? | **Only the firm.** No third-party recipient. The vendor receives nothing. ✅ |
| Is the vendor a processor? | The vendor asserts **no** — it never receives the data. See `non-processor-statement.md` (confirm with your counsel). |
| Is an Art. 28 (DPA) contract needed with the vendor? | On the non-processor basis, no DPA is required for this data. (Normal software-supply/support terms still apply.) |
| Any international transfer? | **None.** No data leaves the device, so no Chapter V / SCC / IDTA analysis is engaged for this processing. ✅ |
| Any sub-processors? | None — there is no processing by the vendor to sub-contract. |

## D. Data flows & security

| DPO question | Vendor-supplied answer |
| --- | --- |
| Where does the data flow? | Local disk → in-memory decode → on-device ASR/(opt) diarization → local exports. Nothing crosses the device boundary. See `data-flow.md`. ✅ |
| Does the app connect to the internet? | No — zero outbound connections, enforced by construction (no HTTP client/plugin linked; CSP `connect-src 'none'`; CI denylist gate). Independently verifiable — see `airplane-mode-verification.md`. ✅ |
| Telemetry / analytics / crash reporting? | **None.** ✅ |
| Auto-updater / update pings? | **None** — IT pushes a new signed installer. ✅ (no updater linked) |
| How is data protected at rest? | Relies on the firm's **BitLocker** full-disk encryption (documented dependency). No app-level crypto by design. See `data-at-rest.md`. |
| Are model/software updates trustworthy? | Models are **signed, versioned packs**; the app verifies ed25519 signature + sha256 + engine-version and **refuses unsigned/tampered/too-new packs (fail-closed)**. ✅ implemented (release packs are signed with a CI-held vendor key; release builds refuse the committed test key). 🚧 Installer code-signing is roadmap. |
| Is the software auditable? | Yes — the source is open; this pack points to the exact files. ✅ |

## E. Data subject rights & retention

| DPO question | Vendor-supplied answer |
| --- | --- |
| Who controls retention? | **The firm / fee-earner.** Transcripts are auto-saved to a local on-device library (app-data dir) until the user deletes them; exports persist where the user saves them. The vendor holds nothing. |
| How is data deleted? | One-click "Delete transcript and audio" removes the source audio and the saved library entry; a saved transcript can also be removed from the Transcripts view. Exports saved elsewhere are deleted via the OS. No vendor copy, no automatic temp spill. ✅ |
| Can a subject-access / erasure request be met? | Yes, entirely within the firm — all copies are the firm's own local files. The vendor has nothing to search or erase. |
| Automated decision-making / profiling? | None. Output is a **machine-generated draft** explicitly requiring human verification; the review/edit step is the expected control. The disclaimer is stamped on every export (`app/src/draft.ts`). ✅ |

## F. Risks & mitigations

| Risk | Mitigation | Status |
| --- | --- | --- |
| Audio/transcript leaks to a third party or vendor | Zero egress by construction; CI gate; strict CSP; no telemetry. | ✅ |
| Device theft exposes stored data | Firm BitLocker full-disk encryption + in-app advisory BitLocker-off warning banner on launch. | ✅ (firm-operated; in-app banner implemented) |
| Tampered/untrusted model injected | Fail-closed signed model-pack verification at the Rust trust boundary; the transcribe path loads weights ONLY from a verified pack (enforced in release builds). | ✅ |
| Persisted transcript exposed after a matter ends | Transcripts auto-save to a local on-device library (BitLocker volume, never leaves device); one-click "Delete transcript and audio" purges the saved entry + source audio, and a saved transcript (not the audio) can be removed from the Transcripts view. | ✅ |
| Over-reliance on an inaccurate transcript | Machine-draft disclaimer in UI + every export; human review is a first-class step; speaker labels off by default. | ✅ |
| Speaker mislabelling (diarization over-segmentation) | Diarization opt-in + optional speaker-count hint + human relabel backstop. | ✅ |
| Residual data left in temp files | In-memory decode writes no scratch file today. | ✅ (today) — 🚧 explicit app-owned scratch + wipe routine |
| Tampered/unauthentic installer | OV Authenticode code signing of installer + DLLs. | 🚧 roadmap |

## G. Outstanding items for the firm's DPIA file

- Confirm the non-processor assertion with the firm's counsel
  (`non-processor-statement.md`).
- Confirm BitLocker is enforced on target laptops via the firm's MDM (the
  in-app banner is advisory only — see `data-at-rest.md` for its limitations).
- Record which build variant is deployed (the **hardened, zero-egress** build —
  the non-processor assertion is scoped to it).
- Note the roadmap items (🚧) above and obtain a delivery date from the vendor
  if any are go-live prerequisites for the firm.
