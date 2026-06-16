# Security Policy

## Design posture

kindtech-transcribe is built to a **zero-egress**, on-device threat model:

- The shipped app and engine link **no networking crates** — enforced in CI by
  `scripts/check-no-network.sh` (the network-crate denylist gate). Audio, transcripts,
  and telemetry never leave the user's machine.
- Model packs are loaded **fail-closed**: a pack is only trusted after structural
  validation, a SHA-256 integrity check, an **ed25519** signature check against a vendor
  public key embedded at build time, and an engine-version gate. Unsigned, tampered, or
  untrusted packs are rejected and nothing is installed.

The compliance and assurance documentation (data-flow, data-at-rest, airplane-mode
verification, DPIA support, non-processor statement, third-party components) lives under
[`docs/assurance/`](docs/assurance/README.md).

## Reporting a vulnerability

Please report security issues **privately** rather than opening a public issue:

- Use GitHub's [private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
  ("Report a vulnerability" under the repository's **Security** tab), or
- Email the maintainers (see the repository profile).

Include reproduction steps and the affected version/commit. We aim to acknowledge
reports within a few business days. Please give us a reasonable window to release a fix
before any public disclosure.

## Scope notes

- The standalone Python CLI (`transcribe.py`) is a legacy/research tool and downloads
  models from Hugging Face on first run; it is **not** part of the zero-egress desktop
  release and is out of scope for the on-device guarantee.
- The `sherpa-onnx` prebuilt C library is fetched at **build time** only; this is not
  runtime egress and is intentionally outside the network gate's scope (see
  `scripts/check-no-network.sh`).
