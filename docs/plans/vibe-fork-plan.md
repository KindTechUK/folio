# Vibe Fork Plan — privacy-first, CPU-only, zero-egress Windows transcription+diarization

> Status: plan (paper investigation, no code cloned/modified) · Date: 2026-06-10
> Step #2 in `docs/prd/local-private-transcription.md`. Builds on `docs/spikes/sherpa-onnx-spike.md`.
> Target base: **Vibe** (github.com/thewh1teagle/vibe, MIT). All repo facts below were verified
> against the live `main` branch in June 2026 via raw file fetch; where a fact could not be
> confirmed from source it is flagged **[UNVERIFIED]**.

---

## 0. Executive summary

- **Vibe is MIT, Tauri v2, React 19 + Vite + react-router, no formal state library.** Good news: the v1→v2 migration tax the PRD worried about **does not exist** — Vibe is already on Tauri 2.
- **Vibe no longer runs whisper in-process.** As of v3.x it spawns a **separate `sona` sidecar binary** (`thewh1teagle/sona`, a Go + whisper.cpp local server) over an **OpenAI-compatible HTTP/NDJSON API**, plus a `sona-diarize` companion. The Rust backend (`desktop/src-tauri/src/sona.rs`, `cmd/transcribe.rs`) is just an HTTP client + process supervisor. This is the single most important structural fact and it reshapes the whole plan.
- **sherpa-rs (the binding the PRD/spike assumed) was archived/deprecated on 2026-06-06.** Use the **official `sherpa-onnx` crate (k2-fsa, v1.13.2, Apache-2.0)** which exposes `OfflineSpeakerDiarization`. Notably, sherpa-rs's author *is* Vibe's author (thewh1teagle) — the deprecation is deliberate and recent.
- **Recommendation: harvest, don't hard-fork.** Build a fresh Tauri v2 app and lift Vibe's proven pieces (sidecar supervisor pattern, model-download UX, installer config, ffmpeg decode, i18n). Reasoning below.
- **Top risks:** (1) the `sona` sidecar's diarization engine and word-timestamp behaviour are not documented in sona's public docs — must be confirmed by reading sona source or replaced with our own sherpa-onnx diarizer; (2) the no-word-timestamps reality of the current `transcribe.rs` path; (3) bundling/static-linking the sherpa-onnx native libs for a *provably offline* build (the crate's build script downloads prebuilt libs unless `SHERPA_ONNX_LIB_DIR` is set).

---

## 1. Repo snapshot (verified)

| Fact | Value | Source (live `main`, June 2026) |
|---|---|---|
| License | **MIT**, © thewh1teagle 2024 | `LICENSE` |
| Desktop shell | **Tauri v2** | `desktop/src-tauri/tauri.conf.json` references `gen/schemas/desktop-schema.json` (v2 shape); `Cargo.toml` `tauri = "2"`, `tauri-build = "2"` |
| App version | 3.0.19 | `tauri.conf.json` |
| Frontend framework | **React 19.2** (`react`, `react-dom` ^19.2.4) | `desktop/package.json` |
| Build tool | **Vite 7** | `desktop/package.json` (devDeps) |
| Router | `react-router-dom` ^7.13 | `desktop/package.json` |
| i18n | `i18next` / `react-i18next` (+ `i18next-resources-to-backend`) | `desktop/package.json`; locales bundled via `tauri.conf.json` `resources: ["locales"]` |
| State management | **None declared** — no Zustand/Redux/Jotai. State is React-local + Tauri `plugin-store` for persistence | `desktop/package.json` (absence) + `@tauri-apps/plugin-store` present |
| Rust workspace | Root `Cargo.toml` members = `["desktop/src-tauri"]` (single member). Shared deps: `eyre`, `serde_json` | `Cargo.toml` |
| Package manager | pnpm (`beforeBuildCommand: "pnpm build"`) | `tauri.conf.json` |

### Rust crate layout (`desktop/src-tauri/src/`)
Confirmed files: `main.rs`, `cli.rs`, `config.rs`, `sona.rs`, `analytics.rs`, `ffmpeg.rs`, `setup.rs`, `logging.rs`, `error.rs`, `custom_protocol.rs`, `diagnostics.rs`, `dock.rs`, `cleaner.rs`, `transcript.rs`, and a `cmd/` directory.
`cmd/` submodules: `app`, `audio`, `download`, `files`, `permissions`, `sona_cmd`, `transcribe`, `ui`, `ytdlp`.

### Tauri plugins in use (all v2)
`window-state`, `fs`, `updater`, `dialog`, `process`, `os`, `deep-link` (scheme `vibe://`), `store`, `single-instance`, `clipboard-manager`, `global-shortcut`, `notification`, `keepawake` (optional), **`http` (with `unsafe-headers`)**, `opener`, and **`aptabase`** (analytics, from a custom git branch). (Source: `desktop/src-tauri/Cargo.toml` + `desktop/package.json` `@tauri-apps/plugin-*`.)

### tauri.conf.json notables
- Bundle targets: `nsis, deb, rpm, dmg, app` — **note: no `msi`/`wix` target today.** Windows ships **NSIS only**.
- Updater: enabled, Minisign pubkey, endpoint `https://github.com/thewh1teagle/vibe/releases/latest/download/latest.json`.
- `app.security.csp = null` (no CSP set).
- Asset protocol scope is wide-open: `"allow": ["**"]`.
- External binaries bundled: `binaries/sona`, `binaries/sona-diarize`.

---

## 2. ASR + diarization path today

**ASR is out-of-process.** `cmd/transcribe.rs` exposes the `transcribe(TranscribeOptions) -> Transcript` Tauri command. It does **not** call whisper directly; it obtains an HTTP client + base URL from `SonaState` and calls `SonaProcess::transcribe_stream()` (in `sona.rs`), which **spawns the `sona` sidecar** (`Command::new(binary_path).spawn()`) and streams **NDJSON** events back.

`sona.rs` event enum (verified):
```rust
pub enum SonaEvent {
    Segment { start: f64, end: f64, text: String, speaker: Option<i32> },
    Result  { text: String },
    Progress { progress: i32 },
    Error   { code: Option<String>, message: String },
}
```
`TranscribeOptions` (verified, abridged): `path, lang, n_threads, word_timestamps, diarize_model, vad_model, stable_timestamps, temperature, beam_size, …`.
Final `Segment` surfaced to the frontend (verified): `{ start: i64, stop: i64, text: String, speaker: Option<String> }` — timestamps in **centiseconds**, **segment-level only; no word-level timestamps are surfaced** through this path even though `word_timestamps` is an option.

**`sona` itself** = `thewh1teagle/sona`, "a local transcription runner built on top of **whisper.cpp**", Go, exposing an OpenAI-compatible HTTP API (`/docs`, `/openapi.json`), Windows x86_64 supported (Vulkan GPU optional; CPU path **[UNVERIFIED but implied]**). Models pulled from Hugging Face in its quick-start (e.g. `ggml-base.bin`).

**Diarization today:** driven by the `diarize_model` option passed through to sona / the `sona-diarize` bundled binary. **sona's public README/BUILDING docs do not state the diarization engine** (sherpa-onnx vs pyannote vs whisper tinydiarize) — **[UNVERIFIED]**. The output is just `speaker: Option<i32/String>` per segment. There is **no in-Rust pyannote** to "rip out"; diarization is the sidecar's concern.

### Implications for our engine work
- **Keep whisper.cpp for ASR** — but we get to choose *how*. Two options:
  - **(A) Reuse the sona sidecar** as-is (Go server + HTTP). Pro: zero engine work, matches Vibe. Con: a second runtime to bundle/sign/audit; HTTP localhost surface to lock down; its diarization engine is opaque.
  - **(B) Replace the sidecar with the official `sherpa-onnx` Rust crate in-process** (ASR *and* diarization), which is exactly what the spike already validated end-to-end. Pro: one binary, no localhost HTTP, no Go, all-ONNX/MIT-Apache, diarization engine is ours and known (pyannote-seg-3.0 ONNX + 3D-Speaker embedding). Con: more Rust engine code; must vendor the native sherpa libs.
- **The spike already proved (B)'s output flows through our `core` `buildTurns` unchanged.** Given the zero-egress, auditable, single-binary requirements, **(B) is the recommended direction.** The sidecar pattern is still worth *harvesting* as the fallback shape if in-process linking proves painful on the Windows target.

---

## 3. Diarization: what to remove / replace

- **Remove:** the dependence on the opaque `sona-diarize` bundled binary and the `diarize_model`-as-HTTP-param flow.
- **Replace with:** the official **`sherpa-onnx` crate's `OfflineSpeakerDiarization`** (k2-fsa, v1.13.2, Apache-2.0) — segmentation = converted pyannote/segmentation-3.0 (MIT), embedding = 3D-Speaker CAM++ (Apache-2.0), clustering with a tunable threshold. This is the exact stack the spike measured (RTF ~0.05–0.06 on CPU).
- **Do NOT use `sherpa-rs`** — archived/deprecated 2026-06-06; upstream now ships the official binding.
- **Speaker-count guard (spike condition #1):** default to a conservative cluster threshold and surface an optional "number of speakers" hint in the UI (PRD user story #32). Auto-count over-segmented (12 vs 3) in the spike; the firm's 1–2-speaker reality plus the human relabel backstop mitigates, but the multi-party path needs the hint.
- **Word timestamps:** neither Vibe's surfaced path nor released Whisper-ONNX decoders give true per-word times (spike finding). Use the PRD-sanctioned **spread-VAD-chunk-span-across-words** model. Our `core/src/editor.ts` already re-spreads spans on edit, so this is consistent.

---

## 4. Network egress to strip (zero-egress is the product)

Every confirmed egress point, with the mechanism and how to remove it. The hardened build must have networking **provably absent** (airplane-mode clean, verifiable by absence of the crates/plugins, not just config flags).

| # | Egress | Mechanism (verified) | How to strip |
|---|---|---|---|
| 1 | **Auto-updater** | `tauri-plugin-updater` (Rust) + `@tauri-apps/plugin-updater` (JS); endpoint `.../releases/latest/download/latest.json` in `tauri.conf.json` | Remove both plugin deps; delete updater block from `tauri.conf.json`; remove updater UI. PRD user story #22 = no updater. IT pushes a new MSI. |
| 2 | **Telemetry / analytics** | `tauri-plugin-aptabase` (custom git fork); `analytics.rs` sends `app_started`, `cli_started`, `sona_spawn_failed`; endpoint from compile-time `APTABASE_BASE_URL`/`APTABASE_APP_KEY` env (`option_env!`); has runtime opt-out via `analytics_enabled` store key | **Delete the crate dep and `analytics.rs` entirely** (don't rely on the opt-out — the dep must be absent for the audit). Remove all `track_event` call sites. |
| 3 | **Model download (first run)** | `cmd/download.rs`: `download_model(app, url, path)` using `reqwest::Client` streaming; **URL is caller-supplied from the frontend** model list (no hardcoded URL in this file) | Replace with offline model-pack import (file-picker / watch-folder) verified by our `modelpack` module. Remove `download_model` command + the frontend model-catalog screen in the hardened build. (Keep an *optional, off-by-default* catalog only in the open-SME build variant — PRD user story #31.) |
| 4 | **YouTube / URL ingestion** | `cmd/ytdlp.rs`: bundled `yt-dlp` binary (`download_audio` command) **+ a live GitHub version-check egress** `https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest` (`get_latest_ytdlp_version`) | Remove the whole `ytdlp` module, both commands, and the bundled yt-dlp binary. PRD strips URL ingestion outright. |
| 5 | **`tauri-plugin-http` (`unsafe-headers`)** | General-purpose HTTP from Rust + JS (`@tauri-apps/plugin-http`); also `reqwest 0.13` direct in Cargo.toml | Remove `tauri-plugin-http`, `@tauri-apps/plugin-http`, and `reqwest` once 1/3/4 are gone. If the sona-sidecar option (2A) is kept, localhost HTTP remains — another reason to prefer the in-process sherpa-onnx engine (2B) so the HTTP stack can be deleted entirely. |
| 6 | **AI summaries / cloud features** | Vibe markets "AI summaries"; mechanism **[UNVERIFIED]** — likely an external LLM endpoint via `plugin-http`. | Audit the frontend for any summary/LLM call; remove the feature and its endpoint. PRD: no cloud features. |
| 7 | **`deep-link` (`vibe://`)** | Not egress per se, but an external-invocation surface | Optional: remove `tauri-plugin-deep-link` + scheme to shrink attack surface; not required for zero-egress. |

**Provable-absence build strategy:** drive the hardened variant with a **Cargo feature `hardened`** (and a parallel Vite build flag) that compiles out updater/http/reqwest/aptabase/ytdlp. CI should `cargo tree` the hardened binary and **fail the build if `reqwest`, `tauri-plugin-http`, `tauri-plugin-updater`, or any aptabase crate appear** in the dependency graph. Also set `tauri.conf.json` capabilities to grant **no** network permissions and a strict CSP (currently `null`). Ship the dependency graph + an airplane-mode test transcript in the assurance pack (PRD "Assurance").

---

## 5. Model handling

**Today:** models are fetched on first run via `cmd/download.rs` with a frontend-supplied URL (HF/GitHub); stored under the app local-data dir. `config.rs` only defines log/store filenames and a "Vibe" documents subfolder — **no hardcoded model URLs live there** (they come from the frontend catalog).

**Target (PRD):**
- Default models **bundled in the installer** so first launch is airplane-mode-clean. Add them to `tauri.conf.json` `bundle.resources` (alongside the existing `locales`) — or, if installer size matters, ship a default pack into the per-user `models/` dir at install time.
- New models arrive as **signed, versioned model packs** dropped into a `models/` folder; the app **verifies signature + sha256 + `engine_min_version` before load and refuses unsigned/tampered packs**. This is exactly what **`core/src/modelpack.ts` (`verifyModelPack` / `validateManifest`)** does (ed25519 over the canonical manifest, constant-time hash compare, fail-closed).

**Changes:**
- Add a Rust `models.rs` (replacing `cmd/download.rs` in the hardened build) that enumerates `models/*` packs, reads each manifest + weights, and calls into the frontend's `verifyModelPack` (or a Rust mirror) before handing a path to the engine. The vendor public key is embedded in the build.
- Engine version map fed to `verifyModelPack({ engineVersions })` must report the actual bundled whisper.cpp / sherpa-onnx versions.
- Wire a "watch folder / file-picker import" UI (reusing Vibe's `plugin-dialog` + `plugin-fs`) in place of the network catalog.

---

## 6. Installer / signing

**Today:** NSIS only on Windows (no MSI/WiX target in `tauri.conf.json`). Known Vibe issue: NSIS `$PLUGINSDIR` DLLs were unsigned and AV-flagged (tauri issue #11673) — a cautionary tale for our signing setup.

**Target (PRD user stories #17–#19, #22):**
- **Add the `msi` (WiX v3) bundle target** for Intune/SCCM. Tauri v2 supports building both MSI (WiX v3) and NSIS from the same config (`bundle.targets`).
- **Per-user no-admin fallback:** NSIS `installMode: "currentUser"` installing to `%LOCALAPPDATA%` (PRD #18). Ship MSI (per-machine, IT-pushed) **and** a per-user NSIS as the two distribution artifacts.
- **OV Authenticode signing** via Tauri v2's Windows signing (`signCommand` / cert thumbprint). Ensure **all** bundled DLLs/sidecars get signed (the #11673 trap). Prerequisite (PRD): a registered company entity to hold the OV cert.
- **Remove the updater signature/artifacts** (Minisign) since there is no updater.

---

## 7. Editor UI — graft vs build

**Today:** Vibe renders segment-level transcripts (`{start, stop, text, speaker}`, centisecond times) in React; it does not surface word-level timestamps through `transcribe.rs`, so it has **no word-karaoke highlighting** to inherit.

**Our assets:**
- `karaoke_editor.html` — the interaction prototype (word highlight, click-to-seek, sentence double-click edit, speaker relabel).
- `core/src/editor.ts` — the **headless reducer already ported from that prototype**: `loadTranscript`, `editSentence` (re-spreads span across re-tokenized words), `renameSpeaker` (global, positional), `reassignSentenceSpeaker`, `exportJSON`, `exportText`.

**Plan: build the editor view fresh as a React component** (Vibe's transcript view is too segment-coarse to retrofit), but **drive 100% of its logic from `core/src/editor.ts`** — the React layer is just rendering + event wiring around the reducer. Reuse Vibe's playback/audio plumbing (`cmd/audio`, `ffmpeg.rs` decode) and `plugin-fs`/`plugin-dialog` for open/export. Keep Vibe's i18n.

---

## 8. Where the existing `core/` TS modules plug in

The `core` package is framework-independent (Node `crypto` for modelpack; pure transforms otherwise) and exports a clean API (`core/src/index.ts`). Seams in the new frontend/backend:

| `core` export | Plugged in at | Role |
|---|---|---|
| `buildTurns(segments)` (`transcript.ts`) | Right after the engine returns the `{segments:[{start,end,text,speaker,words?}]}` sidecar shape (the contract the spike produced) → before render/export | Duration-weighted majority per sentence; merges consecutive same-speaker turns. The spike confirmed sherpa-onnx output flows through this unchanged (18 turns, 3 speakers). |
| `flattenWords`, `splitSentences` | Internal to `buildTurns` and `loadTranscript` | Sentence segmentation on `.?!` |
| `assignSpeakers(words, speakerSegments)` (`diarization.ts`) | In the engine adapter, mapping diarization spans onto ASR words by overlap **before** building turns | Pure mapping logic (not the ML); used when we merge our sherpa-onnx diarization output with ASR words |
| `loadTranscript / editSentence / renameSpeaker / reassignSentenceSpeaker / exportJSON / exportText` (`editor.ts`) | The new React editor view — every edit action dispatches into the reducer; render reads `EditorState` | The whole review-editor behaviour, already unit-tested |
| `verifyModelPack / validateManifest / compareSemver` (`modelpack.ts`) | The model-load path (`models.rs` ↔ frontend), gating every pack before the engine touches it | Fail-closed signature + hash + engine-version verification |

**Packaging note:** `modelpack.ts` imports `node:crypto`. In a Tauri webview there is no Node. Either (a) run verification in a small Node-free WebCrypto port of `modelpack` on the frontend, or (b) **do the verify in Rust** (ed25519 via `ed25519-dalek`, sha256 via `sha2`) mirroring the documented canonicalization contract (sorted-key JSON, `signature` excluded) and keep the TS version as the spec + test oracle. **Recommend (b)** — verification belongs at the trust boundary in the Rust backend, with `core/test` as the cross-checking conformance suite.

---

## 9. Fork vs harvest — recommendation

**Recommendation: HARVEST into a fresh Tauri v2 app.** (Confidence: medium-high.)

Why not a wholesale hard-fork:
- Vibe carries a lot of surface we must delete (updater, aptabase, ytdlp, http, AI summaries, deep-link, multi-OS bundle targets). A fork starts with a large *removal* burden and the risk of missing an egress path during audit — the opposite of what a "provably zero-egress" claim needs.
- Vibe's current engine is a **Go + whisper.cpp HTTP sidecar (`sona`)** plus an opaque `sona-diarize` binary. Our validated direction is **in-process `sherpa-onnx` (Rust)** for both ASR and diarization. Keeping sona means bundling/signing/auditing a second runtime and a localhost HTTP surface; replacing it means the sidecar code is mostly throwaway anyway.

Why not start 100% from scratch:
- Vibe is MIT and **already Tauri v2** — no migration tax. Its sidecar-supervisor pattern, model-download UX, ffmpeg decode (`ffmpeg.rs`), audio capture (`cmd/audio`, forked `cpal`), i18n, `cleaner.rs` (temp wipe — maps to PRD's "wipe scratch on completion"), and NSIS/MSI bundler config are genuinely reusable.

So: **scaffold a new Tauri v2 + React + Vite app; copy in the harvested modules listed in §10 file-by-file with attribution; drive logic from `core/`.** This keeps the auditable surface minimal-by-construction rather than minimal-by-deletion.

---

## 10. Ordered change map

**Engine layer**
1. Scaffold fresh Tauri v2 + React 19 + Vite app (mirror Vibe's `desktop/` layout for familiarity).
2. Add `sherpa-onnx` crate (k2-fsa, ≥1.13.2). Build an `engine` Rust module with an adapter trait (PRD #30) over: ASR (whisper.cpp via sherpa-onnx) + `OfflineSpeakerDiarization`. Vendor native libs via `SHERPA_ONNX_LIB_DIR` (do **not** rely on the build-script download — see risks).
3. Engine emits the contract `{segments:[{start,end,text,speaker,words:[{word,start,end,speaker}]}]}` (spread VAD span across words). Feed through `core` `assignSpeakers` → `buildTurns`.
4. (Fallback only) keep Vibe's `sona.rs` sidecar supervisor harvested but disabled, in case in-process linking on the Windows target proves blocking.

**Network stripping (hardened build)**
5. Do NOT port `analytics.rs`, `cmd/ytdlp.rs`, the updater config, or the network model catalog.
6. Omit `tauri-plugin-updater`, `tauri-plugin-http`, `tauri-plugin-aptabase`, `tauri-plugin-deep-link`, `reqwest`. Add Cargo feature `hardened` and CI `cargo tree` gate that fails if any reappear.
7. Set strict CSP (replace `null`) and network-free capabilities in `tauri.conf.json`.

**Model loading**
8. Add `models.rs` + frontend import UI (file-picker/watch-folder via `plugin-fs`/`plugin-dialog`); no network fetch in hardened build.
9. Implement verification at the trust boundary (Rust mirror of `modelpack`, conformance-tested against `core/src/modelpack.ts`); embed vendor ed25519 pubkey; bundle default packs via `bundle.resources`.

**Editor**
10. Build the React review-editor view; wire all behaviour to `core/src/editor.ts`. Reuse Vibe audio/ffmpeg plumbing and i18n. Add the "machine-generated draft" disclaimer in UI + every export header (PRD liability).

**Installer / signing**
11. Add `msi` (WiX v3) target + per-user NSIS (`currentUser`). OV Authenticode sign app **and all DLLs/sidecars** (avoid Vibe issue #11673). Remove Minisign/updater artifacts.
12. Add BitLocker-status check on launch (PRD #15) and `cleaner.rs`-style scratch wipe to the app dir (PRD).

---

## 11. Risks & unknowns

- **[UNVERIFIED] sona's diarization engine & word-timestamps.** sona's public docs don't state whether `sona-diarize` is sherpa-onnx/pyannote/tinydiarize, and `transcribe.rs` surfaces only segment-level times. *Resolution:* we're replacing it with our own sherpa-onnx diarizer, so this becomes moot — but confirm by reading sona source before deciding to keep any of it.
- **sherpa-onnx native-lib bundling for offline builds.** The crate's build script **downloads prebuilt libs from GitHub** unless `SHERPA_ONNX_LIB_DIR` is set. For a provably-offline *build* (not just runtime), vendor the libs and pin `SHERPA_ONNX_LIB_DIR`. Validate Windows x86_64 CPU static/shared linking early.
- **sherpa-rs deprecation (2026-06-06).** Don't follow stale PRD/spike references to `sherpa-rs`; use the official `sherpa-onnx` crate. Migration is small (different API surface) but real.
- **Speaker-count over-segmentation** (spike): ship the speaker-count hint + conservative threshold; don't promise blind auto-count for multi-party audio.
- **CPU throughput on real firm laptops** (spike): default to small.en; re-measure on target hardware before promising turnaround.
- **MSI vs NSIS for Intune:** MSI (WiX v3) is the Intune-native path; confirm WiX v3 builds cleanly under Tauri v2 on the CI signer, and that per-user NSIS satisfies the no-admin story.
- **Harvest licence hygiene:** Vibe is MIT — preserve its copyright notice in any harvested file; sona is whisper.cpp-based (MIT) but confirm sona's own LICENSE before lifting any Go code. (We are not planning to lift sona.)
- **`modelpack` runtime location:** decide TS-on-frontend (WebCrypto port) vs Rust-at-boundary (recommended) early — it affects where the trust boundary sits.

---

## 12. Recommended first 3 PRs

1. **PR-1 "Engine spike → adapter": in-process sherpa-onnx on Windows CPU.** New Tauri v2 scaffold + `sherpa-onnx` crate wired behind the engine-adapter trait; ASR + `OfflineSpeakerDiarization` producing the `core` contract shape; vendored native libs (`SHERPA_ONNX_LIB_DIR`); a smoke test that runs the spike clip through `assignSpeakers`+`buildTurns`. *De-risks the single biggest unknown (native linking + offline build) and validates the harvest-over-fork bet.*
2. **PR-2 "Hardened, zero-egress shell":** scaffold with updater/http/reqwest/aptabase/ytdlp **absent**; `hardened` Cargo feature; CI `cargo tree` gate failing on any network crate; strict CSP + network-free capabilities; airplane-mode test artifact for the assurance pack. *Locks in the core commercial differentiator before features accrete.*
3. **PR-3 "Model packs + review editor":** Rust-side `verifyModelPack` mirror (conformance-tested vs `core/src/modelpack.ts`) + offline import UI + bundled default pack; the React review-editor view driven entirely by `core/src/editor.ts`, with the draft-disclaimer in UI and exports. *Delivers the two user-facing pillars (trusted offline model updates + the karaoke editor differentiator).*

---

## Sources

- Vibe repo (live `main`, June 2026): `LICENSE`, `Cargo.toml`, `desktop/package.json`, `desktop/src-tauri/Cargo.toml`, `desktop/src-tauri/tauri.conf.json`, `desktop/src-tauri/src/{sona.rs, analytics.rs, config.rs}`, `desktop/src-tauri/src/cmd/{mod.rs, transcribe.rs, download.rs, ytdlp.rs}` — https://github.com/thewh1teagle/vibe
- sona (Go + whisper.cpp local server): https://github.com/thewh1teagle/sona
- sherpa-rs (archived/deprecated 2026-06-06): https://github.com/thewh1teagle/sherpa-rs
- Official sherpa-onnx Rust crate (k2-fsa, v1.13.2, Apache-2.0, `OfflineSpeakerDiarization`): https://docs.rs/sherpa-onnx/latest/sherpa_onnx/ ; https://k2-fsa.github.io/sherpa/onnx/speaker-diarization/rust.html
- Tauri v2 Windows installer / signing / updater docs: https://v2.tauri.app/distribute/windows-installer/ ; https://v2.tauri.app/distribute/sign/windows/ ; https://v2.tauri.app/plugin/updater/
- Tauri NSIS unsigned-DLL AV issue: https://github.com/tauri-apps/tauri/issues/11673
- Local context: `docs/prd/local-private-transcription.md`, `docs/spikes/sherpa-onnx-spike.md`, `core/src/{index,transcript,diarization,editor,modelpack}.ts`, `karaoke_editor.html`
