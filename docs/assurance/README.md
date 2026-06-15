# Compliance / Assurance Pack

> Audience: a UK law firm's IT, DPO, and security reviewers clearing this
> product through procurement and a DPIA.
> Vendor: Sightline (the developer of *Podcast Transcribe*).
> Status of this pack: drafted 2026-06-12 against the live source tree. Every
> capability claim carries a status tag — **✅ implemented and verifiable
> today** or **🚧 planned / roadmap** — so a reviewer can tell what they can
> verify now from what is committed direction.

## Executive summary (the privacy posture in one paragraph)

*Podcast Transcribe* is a self-contained Windows desktop application that
transcribes and (optionally) diarizes sensitive audio **entirely on the
device**. It is engineered so that **audio, transcripts, and any derived
personal data never leave the laptop, and the vendor never receives any of it**.
Outbound networking is absent **by construction, not merely by configuration**:
the application links no HTTP client and no networked plugin, a strict Content
Security Policy forbids the webview from opening any connection, and a CI gate
fails the build if any networking crate reappears. Because the vendor never
receives personal data, the vendor's position is that it **does not act as a
data processor** for the firm under UK GDPR / the Data Protection Act 2018 —
which collapses the DPA, international-transfer, and DPIA burden the firm would
otherwise carry for a cloud transcriber. This pack provides the data-flow
description, the non-processor reasoning, independently runnable zero-egress
verification steps, the third-party component and licence inventory, the
data-at-rest dependency note, and a pre-filled DPIA-support template.

## What is verifiable today vs roadmap (headline)

| Claim | Status |
| --- | --- |
| Zero outbound network **by construction** + CI denylist gate | ✅ implemented |
| Strict CSP (`connect-src 'none'`) + network-free Tauri capabilities | ✅ implemented |
| Fully-local, CPU-only, Python-free ASR + diarization engine (sherpa-onnx, in-process) | ✅ implemented |
| In-app decode with no bundled ffmpeg / no sidecar HTTP | ✅ implemented |
| Signed model-pack verification (ed25519 + sha256, fail-closed) at the Rust trust boundary | ✅ implemented (against a **TEST** key) |
| Model weights loaded **ONLY from a verified signed pack** at transcription time (enforced in release/hardened builds; fail-closed) | ✅ implemented (against a **TEST** key) |
| Producer-side signing CLI to build signed packs (`tools/modelpack-sign`; reuses the verifier's canonicalization) | ✅ implemented |
| DOCX / JSON / text export, each carrying a machine-generated-draft disclaimer | ✅ implemented |
| Local transcript library — transcripts auto-saved on-device (BitLocker volume), zero-egress, purged by delete-matter | ✅ implemented |
| All bundled components / models permissive (MIT / Apache-2.0) | ✅ verified from manifests |
| Launch-time BitLocker-status check + warning (advisory banner) | ✅ implemented |
| One-click "delete this matter" (removes source audio + clears transcript) | ✅ implemented |
| OV Authenticode code signing of the installer + DLLs | 🚧 roadmap |
| MSI (WiX) packaging for Intune/SCCM + per-user no-admin NSIS | 🚧 roadmap (NSIS target only today) |
| Production vendor signing key (fixtures use a committed **TEST** key) | 🚧 roadmap |
| Temp/scratch-file wipe-on-completion | 🚧 roadmap (current engine decodes in-memory — see `data-at-rest.md`) |

## Contents

| File | What it covers |
| --- | --- |
| [`data-flow.md`](data-flow.md) | Where data goes (and doesn't); diagram; storage location; temp-file hygiene intent. |
| [`non-processor-statement.md`](non-processor-statement.md) | The UK GDPR / DPA 2018 reasoning that the vendor is not a data processor. Vendor assertion, to be confirmed by the firm's counsel. |
| [`airplane-mode-verification.md`](airplane-mode-verification.md) | Independently runnable steps to verify zero egress (CI gate, `cargo tree`, CSP, airplane-mode smoke, hardened build). |
| [`third-party-components.md`](third-party-components.md) | Bundled crates, npm deps, and ONNX models with version + licence + attribution. |
| [`data-at-rest.md`](data-at-rest.md) | BitLocker dependency + implemented launch-time encryption banner, temp-file hygiene, implemented one-click delete-matter, verified-pack model storage. |
| [`dpia-support.md`](dpia-support.md) | DPIA-support template: typical DPO questions, pre-filled with this product's answers. |

## How to read this pack as a reviewer

The strongest evidence is in `airplane-mode-verification.md`: it lets you
reproduce the zero-egress claim yourself from the source. The legal framing in
`non-processor-statement.md` is a **vendor assertion**, deliberately not
presented as legal advice — confirm it with your own counsel. Anything tagged
🚧 is committed direction, not a present capability; do not rely on it for a
go-live decision until the corresponding artifact ships.

## Source of truth

This pack is grounded in:
`docs/prd/local-private-transcription.md`,
`docs/plans/vibe-fork-plan.md`,
`scripts/check-no-network.sh`,
`.github/workflows/zero-egress.yml`,
`app/src-tauri/{Cargo.toml,tauri.conf.json,capabilities/default.json}`,
`engine/{Cargo.toml,README.md}`, `modelpack/`, `core/`, and
`fixtures/modelpack/`. The full source is open and auditable.
