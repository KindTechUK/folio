# Vendor non-processor statement (UK GDPR / DPA 2018)

> **This is a vendor assertion to support the firm's own assessment. It is NOT
> legal advice and must be confirmed by the firm's own counsel / DPO before
> being relied upon.** It is written to give your DPO the factual and legal
> hooks to reach their own conclusion quickly.

## The assertion

The vendor (Sightline) asserts that, in respect of the audio the firm
transcribes with *Podcast Transcribe*, **the vendor is not a data processor**
and **not a data controller** under the UK GDPR and the Data Protection Act
2018, because **the vendor never receives, accesses, stores, or otherwise
processes any of that personal data**.

## Why — the factual basis

"Processor" under UK GDPR Art. 4(8) means a party that **processes personal
data on behalf of the controller**. "Processing" (Art. 4(2)) requires an
operation performed *on* personal data by that party. The vendor performs no
such operation:

- The software runs **entirely on the firm's device**. All decode, ASR, and
  diarization are **in-process and on-device** (`engine/`), CPU-only, with no
  server component.
- The application makes **no outbound network connection of any kind** — no
  telemetry, analytics, crash reporting, update ping, model download, or cloud
  feature. This is enforced **by construction**: no HTTP client or networked
  plugin is linked, a strict CSP forbids the webview from connecting, and a CI
  gate fails the build if a networking crate reappears. See
  `airplane-mode-verification.md` for independent verification. ✅ implemented.
- The vendor therefore **receives nothing**: no audio, no transcript, no
  metadata, no usage data. There is no data flow from the device to the vendor
  to characterise.

A party that cannot and does not receive the data is not, by reference to that
data, a processor or controller. The vendor's role is confined to **supplying
software** (and, separately, support/customisation) — the classic position of
a software *supplier*, not a *data processor*.

## Consequences for the firm (subject to counsel's confirmation)

If the firm's own counsel agrees with the above, the following follow:

1. **No Art. 28 processor contract (DPA) is required** between the firm and the
   vendor in respect of this data, because the vendor is not a processor of it.
   (The firm will still have its usual software-supply / support contract.)
2. **No international-transfer analysis (Chapter V / SCCs / IDTA)** is required
   for this processing, because no personal data is transferred to the vendor
   or anywhere off the device.
3. **The DPIA is materially simplified.** The residual processing is the
   firm's own on-device handling of audio it already controls; there is no new
   third-party recipient and no new transfer to assess. See `dpia-support.md`.
4. **The controller remains the firm.** The firm determines the purposes and
   means (which audio to transcribe, retention, who sees the output). The
   vendor does not influence those purposes. Lawful basis for the underlying
   processing is the firm's to determine and document.

## Important scoping caveats (read these before relying on the assertion)

- **The assertion is specific to the on-device, zero-egress product as built
  and verified.** It does **not** cover any optional, off-by-default model
  catalog intended for the non-firm "open SME" segment (PRD #31), which would
  fetch from a vendor server. The firm's build is the **hardened, zero-egress
  variant** with networking compiled out; the non-processor assertion is scoped
  to that variant. Confirm with the vendor which build you are receiving.
- **Support diagnostics are local-only and user-initiated** (PRD #27): the user
  exports diagnostics to a local file they can inspect before choosing to send
  it. If the firm later chooses to send a diagnostic file (or any audio) to the
  vendor for support, **that specific, deliberate transfer is outside this
  assertion** and would need its own handling. The product does not do this
  automatically.
- **Code-signing and packaging (OV Authenticode, MSI) are 🚧 roadmap.** They do
  not affect the data-flow / non-processor analysis (they concern integrity and
  deployment, not egress), but a reviewer should note their status.
- This statement concerns **data protection roles only**. It is not a
  warranty of accuracy of the transcripts (which are positioned as
  machine-generated drafts requiring human verification — see the disclaimer in
  every export) nor a cap or statement of contractual liability.

## What the firm should do with this

Hand this document, together with `data-flow.md` and
`airplane-mode-verification.md`, to your DPO and counsel. The verification pack
lets your security team confirm the factual premise (zero egress) for
themselves; your counsel can then confirm the legal conclusion under your own
risk appetite.
