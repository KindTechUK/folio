# Privacy & security

Folio is built to a **zero-egress**, on-device threat model. The guarantee
is structural, not a policy promise: the shipped app and engine **cannot** make outbound
network calls because they link no networking code.

## The guarantees

- **No network at runtime.** The app and engine link **no networking crates**. This is
  enforced in CI by a network-crate denylist gate
  ([`scripts/check-no-network.sh`](https://github.com/KindTechUK/folio/blob/main/scripts/check-no-network.sh)),
  and the Tauri webview CSP sets `connect-src 'none'`. You can run the whole pipeline in
  airplane mode.
- **No telemetry.** Nothing is collected or phoned home.
- **CPU-only, local models.** Transcription and diarization run in-process on your
  machine from locally-imported [model packs](model-packs.md).
- **Fail-closed model trust.** Packs are verified (structure → sha256 → ed25519
  signature → engine-version gate) before they are ever loaded; unsigned, tampered, or
  untrusted packs are rejected and nothing is installed.

!!! note "Build-time vs runtime"
    The `sherpa-onnx` C library is fetched once at **build time** and is intentionally
    outside the network gate's scope. It is not runtime egress.

## Assurance pack

Detailed compliance documentation (intended for security/legal review) lives in the
repository and is mirrored here:

- [Assurance pack overview](assurance/README.md)
- [Data flow](assurance/data-flow.md)
- [Data at rest](assurance/data-at-rest.md)
- [Airplane-mode verification](assurance/airplane-mode-verification.md)
- [DPIA support](assurance/dpia-support.md)
- [Non-processor statement](assurance/non-processor-statement.md)
- [Third-party components](assurance/third-party-components.md)

## Reporting a vulnerability

Please report security issues **privately** via GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
(the **Security** tab → *Report a vulnerability*). See
[SECURITY.md](https://github.com/KindTechUK/folio/blob/main/SECURITY.md).
