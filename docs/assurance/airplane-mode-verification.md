# Airplane-mode / zero-egress verification

This document is part of the **assurance pack**. It lets an independent reviewer
(e.g. a law firm's IT / a security auditor) verify for themselves that the
Kindtech Transcribe application makes **no outbound network connections** — that
networking is absent **by construction**, not merely disabled by configuration.
It addresses PRD user story #24 (compliance officer wants independent
airplane-mode verification steps).

The verification rests on three independent, layered checks:

1. **Source/dependency evidence** — no networking crate is in the build. ✅
2. **Configuration evidence** — the runtime is locked down (CSP + capabilities). ✅
3. **Behavioural evidence** — the app works fully in airplane mode. ✅

All three are reproducible today from the open source tree.

---

## 1. Dependency-graph evidence (the gate) ✅ implemented

The strongest evidence: the compiled binary **cannot** make HTTP calls because
no HTTP client or networked Tauri plugin is linked into it.

Run the gate:

```sh
bash scripts/check-no-network.sh
```

Expected: it prints `PASS` and exits `0`. It runs `cargo tree -e normal` over
`app/src-tauri` (default **and** `--features hardened`), over `engine`, and over
`modelpack`, and fails if any of these crate names appear as a linked
dependency:

> `reqwest`, `hyper`, `h2`, `ureq`, `isahc`, `surf`, `attohttpc`, `curl`,
> `tauri-plugin-http`, `tauri-plugin-updater`, `tauri-plugin-aptabase`,
> `aptabase`.

To inspect the full graph manually:

```sh
cargo tree --manifest-path app/src-tauri/Cargo.toml -e normal | less
cargo tree --manifest-path app/src-tauri/Cargo.toml -e normal --features hardened | less
cargo tree --manifest-path engine/Cargo.toml -e normal | less
cargo tree --manifest-path modelpack/Cargo.toml -e normal | less
```

The same gate runs in CI (`.github/workflows/zero-egress.yml`, job `gate`) on
every push, so a network crate cannot be merged in without turning the build
red. The workflow is **live** and currently passing on both Linux (the gate +
frontend) and a real **Windows x64** runner (the MSVC static-link job), so the
zero-egress property is enforced on every change — and it is also provable
locally at any time via the `scripts/check-no-network.sh` script above.

**Crates deliberately absent** (the egress points stripped from the upstream
Vibe app — see `docs/plans/vibe-fork-plan.md` §4): the auto-updater
(`tauri-plugin-updater`), telemetry (`tauri-plugin-aptabase`), the generic HTTP
plugin / client (`tauri-plugin-http`, `reqwest`), URL/YouTube ingestion, and
cloud summaries. None are present. The app's `Cargo.toml` links **only**
`tauri` (with `protocol-asset`), `tauri-plugin-dialog`, `tauri-plugin-fs`, the
in-process `engine`, and `modelpack` — confirm by reading
`app/src-tauri/Cargo.toml`.

### Scope note (build-time vs runtime)

The `sherpa-onnx` speech engine statically links prebuilt C libraries. On a
**first developer build** its build script may download those libraries once
from GitHub (controllable / eliminable via `SHERPA_ONNX_LIB_DIR`, see
`engine/README.md`). This is a **build-time** developer fetch — it is **not** a
linked network crate and **not** something the shipped client binary does at
runtime. The shipped binary contains the libraries statically; it opens no
sockets. The gate therefore scopes to *linked runtime crates*. For a
**provably-offline build** (not just runtime), vendor the libs and set
`SHERPA_ONNX_LIB_DIR` so even the build performs no fetch (🚧 the CI
`windows-link` job is wired to do this once the vendored libs are cached).

---

## 2. Configuration evidence (defence in depth) ✅ implemented

Even though the binary has no networking code, the runtime is also locked down
in `app/src-tauri/tauri.conf.json`:

- **Content Security Policy** is set (not `null`):
  `default-src 'self'; connect-src 'none'; img-src 'self' asset: data:;
  media-src 'self' asset: blob:; script-src 'self'; style-src 'self'
  'unsafe-inline'`.
  - `connect-src 'none'` — the webview is forbidden from making any `fetch` /
    `XHR` / `WebSocket` connection.
  - No remote origins are permitted for scripts/styles/images/media — only
    app-local and the `asset:`/`data:`/`blob:` protocols.
- **Capabilities** (`app/src-tauri/capabilities/default.json`) grant only
  `core:default`, scoped `dialog` (open/save), and scoped `fs` (read/write under
  the user data + document/audio/desktop/download dirs). There is **no network
  capability** — Tauri v2 exposes none without the `http` plugin, which this app
  omits.

A reviewer can read both files directly; they are short and self-describing.

---

## 3. Behavioural evidence (airplane-mode smoke) ✅ runnable

1. Disconnect the machine from all networks (enable airplane mode / pull the
   cable / disable Wi-Fi).
2. Launch the installed application.
3. Open a local audio file (`.wav`, `.mp3`, `.m4a`, etc.) and run
   **Open audio → transcribe**.
4. Confirm a transcript is produced with no errors and no
   "offline"/connection failures, and that export to DOCX/JSON/text works.

Optionally, run a packet capture (e.g. Wireshark / `tcpdump`, or Little Snitch /
the Windows Defender Firewall outbound log / `netstat -b`) for the application's
process during a full transcription **and** export, and confirm **zero**
outbound packets are attributed to it. This is the empirical backstop on top of
the structural (gate) and configuration (CSP) evidence.

---

## 4. The hardened build variant ✅ scaffolded / 🚧 finalised

The shipped firm build is the **hardened variant**, driven by the `hardened`
Cargo feature (`app/src-tauri/Cargo.toml`). In PR2, **both** the default and
`hardened` builds are already network-free — the gate scans both. `hardened` is
the forward guard rail: it is reserved to compile **out** any future optional,
off-by-default networked model catalog (the non-firm "open SME" variant,
PRD #31), so that the firm's binary can never contain that surface even as the
codebase grows. Reviewers receiving the firm build should confirm it is built
with `--features hardened` and that the gate passes for that feature set (it
does, in CI's `gate` job and the local script).

---

## Summary

| Layer | Artifact | What it proves | Status |
| --- | --- | --- | --- |
| Dependency | `scripts/check-no-network.sh` + CI `gate` job | No HTTP client / networked plugin is linked (default + hardened + engine + modelpack). | ✅ |
| Config | `tauri.conf.json` CSP `connect-src 'none'` + capabilities | The webview/runtime is forbidden from connecting; no network capability granted. | ✅ |
| Behaviour | Airplane-mode smoke + packet capture | The running app emits no traffic. | ✅ runnable |
| Build | `hardened` Cargo feature + gate scans it | Optional networked surface is compiled out of the firm build. | ✅ scaffolded |

Together these establish zero egress **by construction** — the basis for the
vendor not acting as a UK GDPR data processor for the audio it transcribes (see
`non-processor-statement.md`).
