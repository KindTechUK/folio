# Airplane-mode / zero-egress verification

This document is part of the **assurance pack**. It lets an independent reviewer
(e.g. a law firm's IT / a security auditor) verify for themselves that the
Podcast Transcribe application makes **no outbound network connections** — that
networking is absent **by construction**, not merely disabled by configuration.

The verification rests on three independent, layered checks:

1. **Source/dependency evidence** — no networking crate is in the build.
2. **Configuration evidence** — the runtime is locked down (CSP + capabilities).
3. **Behavioural evidence** — the app works fully in airplane mode.

---

## 1. Dependency-graph evidence (the gate)

The strongest evidence: the compiled binary **cannot** make HTTP calls because
no HTTP client or networked Tauri plugin is linked into it.

Run the gate:

```sh
bash scripts/check-no-network.sh
```

Expected: it prints `PASS` and exits `0`. It runs `cargo tree -e normal` over
`app/src-tauri` (default **and** `--features hardened`) and over `engine`, and
fails if any of these crate names appear:

> `reqwest`, `hyper`, `h2`, `ureq`, `isahc`, `surf`, `attohttpc`, `curl`,
> `tauri-plugin-http`, `tauri-plugin-updater`, `tauri-plugin-aptabase`,
> `aptabase`.

To inspect the full graph manually:

```sh
cargo tree --manifest-path app/src-tauri/Cargo.toml -e normal | less
```

The same gate runs in CI (`.github/workflows/zero-egress.yml`, job `gate`), so a
network crate cannot be merged in without turning the build red.

**Crates deliberately absent** (the egress points stripped from the upstream
Vibe app — see `docs/plans/vibe-fork-plan.md` §4): the auto-updater
(`tauri-plugin-updater`), telemetry (`tauri-plugin-aptabase`), the generic HTTP
plugin / client (`tauri-plugin-http`, `reqwest`), URL/YouTube ingestion, and
cloud summaries. None are present in this build.

### Scope note (build-time vs runtime)

The `sherpa-onnx` speech engine statically links prebuilt C libraries. On a
**first developer build** its build script may download those libraries once
from GitHub (controllable / eliminable via `SHERPA_ONNX_LIB_DIR`, see
`engine/README.md`). This is a **build-time** developer fetch — it is **not** a
linked network crate and **not** something the shipped client binary does at
runtime. The shipped binary contains the libraries statically; it opens no
sockets. The gate therefore scopes to *linked runtime crates*.

---

## 2. Configuration evidence (defence in depth)

Even though the binary has no networking code, the runtime is also locked down
in `app/src-tauri/tauri.conf.json`:

- **Content Security Policy** is set (not `null`) with:
  - `connect-src 'none'` — the webview is forbidden from making any `fetch` /
    `XHR` / `WebSocket` connection.
  - `default-src 'self'`; scripts/styles/images/media restricted to app-local
    and the `asset:`/`data:`/`blob:` protocols only — no remote origins.
- **Capabilities** (`app/src-tauri/capabilities/default.json`) grant only
  `core:default`, scoped `dialog` (open/save), and scoped `fs` (read/write under
  the user data + document/audio dirs). There is **no network capability** —
  Tauri v2 exposes none without the `http` plugin, which this app omits.

---

## 3. Behavioural evidence (airplane-mode smoke)

1. Disconnect the machine from all networks (enable airplane mode / pull the
   cable / disable Wi-Fi).
2. Launch the installed application.
3. Open a local `.wav` file and run **Open audio → transcribe**.
4. Confirm a transcript is produced with no errors and no "offline"/connection
   failures.

Optionally, run a packet capture (e.g. Wireshark / `tcpdump`, or Little Snitch /
the Windows firewall outbound log) for the application's process during a full
transcription and confirm **zero outbound packets** are attributed to it.

---

## Summary

| Layer | Artifact | What it proves |
| --- | --- | --- |
| Dependency | `scripts/check-no-network.sh` + CI `gate` job | No HTTP client / networked plugin is linked. |
| Config | `tauri.conf.json` CSP `connect-src 'none'` + capabilities | The webview/runtime is forbidden from connecting. |
| Behaviour | Airplane-mode smoke + packet capture | The running app emits no traffic. |

Together these establish zero egress **by construction** — the basis for the
vendor not acting as a UK GDPR data processor for the audio it transcribes.
