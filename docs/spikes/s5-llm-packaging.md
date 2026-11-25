# Spike S5: packaging, licensing and zero-egress for the local LLM

> Status: **GO-WITH-CONDITIONS** (Mac measured; CI green on Linux with no network, macOS and Windows, where the
> binary links and starts both runtimes without a model) · Recorded 2026-11-24, review fixes 2026-11-25.
> Plan: [`next-phase-spikes.md`](next-phase-spikes.md#s5-packaging-licensing-and-zero-egress).
> Harness: `spikes/s5-llama/` (one binary linking llama.cpp and the engine's sherpa-onnx).

## Question

Can llama.cpp be linked in-process, built from vendored sources, and shipped with its weights as a signed
model pack, without adding a network crate or a licence we can't redistribute?

## Verdict

Yes. llama.cpp and sherpa-onnx link into one static, CPU-only binary; the build needs no network; the
zero-egress gate stays green; a signed 2.4 GB Qwen3-4B pack verifies, resolves and loads in flat memory, and
fails closed after a one-byte change; and both candidate weights are plain Apache-2.0.

| Pass criterion | Result | |
|---|---|---|
| Gate green | `scripts/check-no-network.sh` passes, now also scanning `spikes/s5-llama` on all targets | **Pass** |
| Reproducible static build, no build-time download | Mac: clean build with outbound network denied by the OS sandbox; two clean builds gave bit-identical binaries. Linux (CI): builds inside a network namespace with no network. Windows (CI, real MSVC): links once condition 2's CMake setting is in, and the self-check runs | **Pass** |
| Signed pack verifies and loads | `task: "llm"` pack signed, verified by the app's LLM resolver (given an engine map with llama.cpp, as P2's build will have), GGUF loaded and generating; one flipped byte rejected | **Pass** |
| Weights licence cleared | Qwen3-4B-Instruct-2507 and Qwen3.5-4B: Apache-2.0, ungated, no use policy, no NOTICE | **Pass** |

**Conditions:**
1. **Turn off llama.cpp's `common` library properly before P2 ships.** `llama-cpp-2` depends on
   `llama-cpp-sys-2` *with its default features*, so the sys crate's `common` feature is on whatever our
   `Cargo.toml` says. That compiles llama.cpp's `common` library, which includes its model downloader and the
   vendored `cpp-httplib` client; if CMake finds OpenSSL (Homebrew on a Mac, `libssl-dev` on CI runners), the
   client is built with HTTPS. None of it reaches our binary, because nothing references it (checked below),
   and `cargo tree` can't see any of it. The spike sets `CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=TRUE` so HTTPS
   is never built. For the product, patch `llama-cpp-2` (a `[patch.crates-io]` copy whose `llama-cpp-sys-2`
   dependency has `default-features = false`), or send that one-line fix upstream, and add a binary-level
   check to the gate (below).
2. **Windows needs llama.cpp built with the static CRT, through CMake.** sherpa-onnx's Windows archive is built
   with the static CRT (`/MT`); llama.cpp builds with the DLL CRT (`/MD`) by default, and the link fails:
   `mismatch detected for 'RuntimeLibrary': … MD_DynamicRelease … MT_StaticRelease`. `LLAMA_STATIC_CRT=1`
   alone was enough for the `cargo xwin` cross-build, but not on `windows-latest` with real MSVC: llama.cpp's
   `cmake_minimum_required(... 3.28)` turns on CMake policy CMP0091, so the runtime comes from
   `CMAKE_MSVC_RUNTIME_LIBRARY` (default `MultiThreaded…DLL`) and the `/MT` flags are overridden. The spike now
   also sets `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded` in `spikes/s5-llama/.cargo/config.toml`; the app will
   need the same.
3. **Keep the `s5-llama` CI job (or its equivalent for the app) green.** On its first run it caught condition
   2 on real MSVC, which the cross-build had hidden. It now passes on all three: the Linux leg builds inside a
   network namespace with no network (the CI-side proof of "no build-time download"), and on Windows the binary
   runs its self-check (sherpa-onnx 2.0.1 / onnxruntime 1.28.2 plus the llama.cpp backend in one process). The
   self-check is a link-and-backend smoke test: it opens no inference session in either runtime. No model has
   been loaded on Windows yet; that is for the reference laptop.
4. **Validate llama.cpp inputs before calling it.** llama.cpp **aborts the process** on misuse instead of
   returning an error: the first long-prompt run here died on `GGML_ASSERT(n_tokens_all <= cparams.n_batch)`.
   Inside the app that kills the whole app, so P2's wrapper must check prompt length against `n_batch`/`n_ctx`
   (or run the LLM in a child process).
5. **Decide the LLM pack layout for P2.** The spike keeps the existing pack format (a zip in `weights.bin`,
   with the GGUF Stored). That costs a second 2.4 GB copy on disk (pack plus extracted GGUF) and the
   extracted copy is trusted from the cache without being re-hashed, and the resolver re-opens the pack by path
   after verifying it rather than extracting the bytes it hashed (the speech packs work the same way). Since
   a Stored zip gains nothing for one file, consider letting an `llm` pack's `weights.bin` *be* the GGUF:
   verify it, then load it in place.

## Environment

- **Machine:** Apple M5 Pro (18 cores, 48 GB), macOS 27.2. Not the reference laptop. **Heavily loaded:**
  other spikes were running two `llama-server` processes (about 430% CPU each), so the load average was
  52–130 during the builds and first timings and about 16 during the later timings. Wall-clock build times
  are noisy; CPU time (user + sys) is the steadier figure.
- **Toolchain:** rustc 1.99.1; CMake 4.5.1 and Ninja 1.13.2 (Homebrew, installed for this spike); Apple
  clang; for Windows, `cargo-xwin` 0.23.1 with Homebrew LLVM and the cached Windows SDK.
- **Crates:** `llama-cpp-2 =0.1.161` and `llama-cpp-sys-2 =0.1.161` (published 2026-11-03, 21 days before the
  spike), default features off, no GPU feature. The sys crate vendors **llama.cpp v0.6.0** (commit
  `d812350`, 2026-10-05; ggml 0.26.0), which is the `llama.cpp` engine version recorded for packs.
  `sherpa-onnx =2.0.1` (onnxruntime 1.28.2), the same version the engine pins.
- **Build settings** (`spikes/s5-llama/.cargo/config.toml`): `GGML_METAL=OFF` (CPU only; llama-cpp-2 turns on
  the sys crate's `metal` feature on Apple Silicon, so only the Metal framework link remains),
  `CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=TRUE`, `LLAMA_STATIC_CRT=1`, `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded`
  (condition 2). `GGML_OPENMP` is off (no OpenMP runtime to
  clash with onnxruntime's threads), `GGML_NATIVE` off, `LLAMA_CURL` off.
- **Weights:** `Qwen3-4B-Instruct-2507-Q4_0.gguf` (2,375,773,280 bytes, SHA-256 `e0ba675d…13be2`, matching the
  Hugging Face LFS hash of `unsloth/Qwen3-4B-Instruct-2507-GGUF`) and `Qwen3.5-4B-Q4_0.gguf` (`298fcb5f…b72b8bb`,
  matching `unsloth/Qwen3.5-4B-GGUF`).

## Commands

```bash
cd spikes/s5-llama
cargo fetch
# sherpa-onnx's build script downloads its static libs unless pointed at them; here the extracted
# official sherpa-onnx-v2.0.1-osx-arm64-static-lib archive from an earlier engine build:
export SHERPA_ONNX_LIB_DIR=…/sherpa-onnx-v2.0.1-osx-arm64-static-lib/lib

# Clean builds with outbound network denied by the macOS sandbox (curl fails inside it):
sandbox-exec -p '(version 1)(allow default)(deny network-outbound (remote ip))' \
  cargo build --offline --release --target-dir target-sandbox
sandbox-exec -p '(version 1)(allow default)(deny network-outbound (remote ip))' \
  cargo build --offline --release --no-default-features --target-dir target-nollama

./target-sandbox/release/s5-llama --self-check
P="$(python3 prompt.py)" && /usr/bin/time -l ./target-sandbox/release/s5-llama \
  --model Qwen3-4B-Instruct-2507-Q4_0.gguf --threads 4 --n-predict 64 --prompt "$P" --vad silero_vad.onnx

# Windows cross-check (scripts/build-windows-on-mac.sh has the one-time setup):
cargo xwin build --release --target x86_64-pc-windows-msvc --target-dir target-xwin

# Real pack, with a throwaway key (never committed):
modelpack-sign keygen --out-dir /tmp/s5/keys
modelpack-sign pack --model-dir /tmp/s5/model --out /tmp/s5/packs/llm-qwen3-4b-instruct-2507 \
  --key /tmp/s5/keys/private.pkcs8.pem --id llm-qwen3-4b-instruct-2507 --version 1.0.0 \
  --engine llama.cpp --engine-min-version 0.6.0 --task llm --license Apache-2.0 \
  --attribution "Qwen3-4B-Instruct-2507 by the Qwen Team, Alibaba Cloud (Apache-2.0); GGUF Q4_0 by Unsloth"
modelpack-sign verify --pack /tmp/s5/packs/llm-qwen3-4b-instruct-2507 --pubkey /tmp/s5/keys/public.spki.pem
```

`prompt.py` (in the harness) prints a 24-line synthetic housing-visit transcript with an instruction to list its facts (730
tokens with Qwen3's chat template). The app-side resolve was measured with a temporary, uncommitted `#[ignore]`
test calling `resolve_verified_llm_in` on the real pack, run under `/usr/bin/time -l`. "Before" numbers come
from `main`'s `tools/modelpack-sign`, built from `git archive main`.

## No build-time download

- **`llama-cpp-sys-2`'s `build.rs`** runs bindgen on the vendored headers and CMake on the vendored
  `llama.cpp/` tree. It sets `LLAMA_CURL=OFF` and turns tests, examples, tools, server and the `llama` app off.
  It has no download code of its own.
- **Every download path in the vendored CMake is off by default and stays off:** KleidiAI (`GGML_CPU_KLEIDIAI`,
  OFF), the Windows LLVM OpenMP fetch (`GGML_OPENMP_FETCH`, OFF; OpenMP is off too), and BoringSSL/LibreSSL
  `FetchContent` inside `vendor/cpp-httplib` (only reached through `common`, condition 1, and only with options
  we don't set).
- **Proof:** the clean Mac build above ran with outbound IP traffic denied and `--offline`, and succeeded. The
  CMake cache shows `GGML_METAL=OFF`, `GGML_OPENMP=OFF`, `GGML_NATIVE=OFF`, `GGML_CPU_KLEIDIAI=OFF`; the build
  log has no "OpenSSL found" line.
- **Reproducible:** two clean builds into different target directories, one sandboxed and one not, produced
  byte-identical binaries (SHA-256 `fcf1d661…72fb`).
- **What reaches the binary:** the link line has `ggml-base`, `ggml-cpu`, `ggml`, `llama`, `llama-common`,
  `llama-common-base` (static) plus system frameworks; no `cpp-httplib`, no OpenSSL. `otool -L` lists only
  system libraries; `strings` finds no `httplib`, `User-Agent` or download URLs, only GGUF metadata key names.
  `libllama-common.a` does contain the downloader (34 `download` symbols) but the linker pulls none of it.
- **Blind spots of the gate this spike found:** `cargo tree -e normal` doesn't show build dependencies
  (sherpa-onnx-sys compiles `ureq` and `rustls` into its build script, which the offline build never runs), and
  it can't see anything CMake builds. The new CI job's no-network Linux build covers both; a cheap addition
  would be a binary-level check in the gate (imports and strings of the release binary).

## Raw numbers

### Build and size (Mac, release, `strip = "symbols"`)

| Build | Wall time | CPU time (user + sys) | Load average | Binary |
|---|---:|---:|---|---:|
| engine only (`--no-default-features`) | 40 s | 99 s | high | 16,986,704 B |
| engine only, sandboxed | 97 s | 96 s | 95–130 | 16,986,704 B |
| engine + llama.cpp | 7 min 40 s | 334 s | high | 20,616,816 B |
| engine + llama.cpp, sandboxed | 8 min 35 s | 313 s | 52–94 | 20,616,816 B |

llama.cpp adds **about 3.6 MB (+21%)** to the binary and **about 220–240 s of CPU time** to a clean build,
almost all of it the CMake build of llama.cpp. Windows (cross-compiled with `cargo xwin`): 18,067,968 B
without llama.cpp, 22,466,048 B with it (**+4.4 MB, +24%**). The `.exe` imports `kernel32`, `ntdll`,
`ADVAPI32`, `psapi`, `dbghelp`, `setupapi`, `dxgi` and two `api-ms-win-core` sets: no CRT DLL.

### Both runtimes in one process

`--self-check` prints sherpa-onnx 2.0.1 / onnxruntime 1.28.2, runs `SherpaEngine::new` (fails closed with no
models, as it should), starts the llama.cpp backend (devices: `CPU` only) and exits 0, at 10 MB peak RSS. It
opens no inference session, so it proves linking and backend start-up, not two live runtimes.
`--vad` opens a real Silero VAD session through sherpa-onnx (11–52 ms for 1 s of audio) and keeps it open while
llama.cpp loads and generates, then runs it again (2 ms). No duplicate symbols, no thread-pool clash.

### Generation, Qwen3-4B-Instruct-2507 Q4_0, 4 threads, CPU, 2k context, greedy

| Run | Load avg | Load | Prompt (730 tok) | Generation (64 tok) | Peak footprint | Peak RSS |
|---|---|---:|---:|---:|---:|---:|
| 1 | 52 | 1.78 s | 52.1 tok/s | 17.6 tok/s | 2.77 GB | 4.90 GB |
| 2 | 52 | 1.23 s | 52.2 tok/s | 20.7 tok/s | 2.77 GB | 5.13 GB |
| 3 | 72 | 1.26 s | 45.2 tok/s | 21.2 tok/s | 2.77 GB | 5.13 GB |
| 4 | 16 | 1.48 s | 101.8 tok/s | 34.0 tok/s | 2.77 GB | 5.14 GB |
| 5 | 16 | 0.80 s | 99.2 tok/s | 27.9 tok/s | 2.77 GB | 5.14 GB |
| 6 | 16 | 0.96 s | 92.0 tok/s | 28.1 tok/s | 2.76 GB | 5.14 GB |
| 7 | 16 | 1.86 s | 114.4 tok/s | 34.7 tok/s | 2.77 GB | 5.10 GB |
| 8 | 17 | 0.67 s | 116.4 tok/s | 36.7 tok/s | 2.77 GB | 5.14 GB |
| 9 | 16 | 0.61 s | 116.3 tok/s | 36.8 tok/s | 2.77 GB | 5.14 GB |

Every run also ran a Silero VAD session through sherpa-onnx. In runs 1–6 that session was closed before
llama.cpp loaded; runs 7–9 (2026-11-25, after the review) kept it open through generation, with the same
footprint. Qwen3.5-4B Q4_0, same prompt (754 tokens) at load 16: load
2.03 s, prompt 81.5 tok/s, generation 26.0 tok/s, footprint 2.83 GB. Load times are with the GGUF in the page
cache. "Peak footprint" is macOS's physical footprint; RSS also counts the memory-mapped GGUF pages and so
overstates real use (S2 saw the same). The loaded-machine figures match S2's medians (prefill 53, generation 16
tok/s for Qwen3-4B).

### The real pack

| Step | `main` (whole file in RAM, Deflate) | This branch (streamed, GGUF Stored) |
|---|---|---|
| Pack size | 2,274,469,097 B (Deflate saves 4.3%) | 2,375,773,442 B (GGUF + 162 B) |
| Sign (`pack`) | 66.5 s, **4.66 GB** peak footprint | 19.7 s, **3.7 MB** peak footprint |
| Verify (`verify`) | 6.25 s, **2.28 GB** | 6.52 s, **2.6 MB** |
| App resolve, first (verify + extract) | — | 8.65 s, 3.5 MB footprint (10.9 MB RSS) |
| App resolve, cache hit (verify only) | — | 6.16 s |

`main` can't sign a `task: "llm"` pack, so its numbers are for the same GGUF signed as `task: "asr"`; its
verifier rejects the new pack (`manifest engine "llama.cpp" is not one of: whisper.cpp, sherpa-onnx`), so older
apps fail closed on LLM packs. The extracted GGUF hashed to the original's `e0ba675d…` and loaded and
generated in the spike binary. After flipping the byte at offset 1,000,000,000 of `weights.bin`, the app
resolver returned `no verified LLM model pack found` and `modelpack-sign verify` printed `REJECTED: weights
hash mismatch (tampered or corrupt)`.

Disk use per installed LLM is about 4.75 GB: the 2.38 GB pack plus the 2.38 GB extracted GGUF (condition 5).
Re-verifying the pack costs about 6 s of hashing each time it is resolved. Speech resolution, which runs at every
recording start, skips LLM packs before hashing them.

## Licences

| Component | Licence | Redistribution | Attribution / notices | Use restrictions |
|---|---|---|---|---|
| Qwen3-4B-Instruct-2507 (`Qwen/Qwen3-4B-Instruct-2507`, rev `cdbee75`) | Apache-2.0 (LICENSE in repo, "Copyright 2024 Alibaba Cloud") | Allowed, including modified forms | Keep the licence and copyright line, mark the GGUF as modified (converted, quantised). No NOTICE file | None. Ungated, no acceptable-use policy |
| GGUF `unsloth/Qwen3-4B-Instruct-2507-GGUF` (rev `a06e946`) | Card says Apache-2.0, linking Qwen's LICENSE; the repo has no LICENSE of its own | Same terms as upstream | Credit Unsloth for the conversion | None |
| Qwen3.5-4B (`Qwen/Qwen3.5-4B`, rev `851bf6e`; base `Qwen3.5-4B-Base`) | Apache-2.0 ("Copyright 2026 Alibaba Cloud") | Allowed | As above. No NOTICE file | None. Ungated. The model is multimodal; the Q4_0 GGUF is the text model only |
| GGUF `unsloth/Qwen3.5-4B-GGUF` (rev `e87f176`) | Card says Apache-2.0, linking Qwen's LICENSE | Same | Credit Unsloth | None |
| llama.cpp + ggml v0.6.0 | MIT ("Copyright (c) 2023-2026 The ggml authors") | Allowed | Ship the MIT notice | None |
| `llama-cpp-2`, `llama-cpp-sys-2` 0.1.161 | MIT OR Apache-2.0 | Allowed | Licence text | None |

Apache-2.0 gives no trademark rights (section 6), so the app can say "Qwen3-4B-Instruct-2507 by the Qwen Team"
but shouldn't brand anything as Qwen. The rows are in `docs/assurance/third-party-components.md`.

## Pack support added on this branch

- `modelpack` (Rust) and `core/src/modelpack.ts` accept `task: "llm"` and `engine: "llama.cpp"`, only together:
  an `llm` pack on another engine, or another task on `llama.cpp`, is rejected. The conformance fixtures gained
  `valid-llm` and `malformed-llm-wrong-engine` cases; `context.json` gives `llama.cpp` version 0.6.0.
- `modelpack` hashes weights from a stream in 1 MiB chunks (`verify_model_pack_reader`, `sha256_hex_file`);
  `verify_model_pack` over a byte slice still works as before.
- `modelpack-sign` has `--task llm`, zips into a staging file (each file streamed in; `.gguf` Stored,
  everything else Deflated as before; zip64 for files of 4 GiB or more), hashes and self-verifies by streaming,
  and only then replaces `weights.bin` and `manifest.json`, so a failed rebuild leaves the old pack intact. It
  refuses an out-dir inside the model dir, and knows `llama.cpp` 0.6.0.
- The app (`app/src-tauri/src/models.rs`) verifies and extracts packs by streaming, skips LLM packs before
  hashing them when resolving speech models, and has `resolve_verified_llm_in` / `resolve_verified_llm`, which
  return the GGUF path from the newest verified `llm` pack and fail closed otherwise. Nothing calls them yet.
  The app's own engine map leaves `llama.cpp` out until P2 links it, so this build lists a signed LLM pack as
  "engine not supported by this build" and `resolve_verified_llm` fails closed; `resolve_verified_llm_in` takes
  the engine map as an argument, which is how it was measured.
- Tests (the agreed list): the `valid-llm` and unknown-task conformance cases (Rust and TS); `--task llm`
  through the real CLI producing a pack the verifier accepts; the app resolving a signed LLM fixture pack to
  its GGUF, rejecting a tampered one, and never returning a speech pack as an LLM; and a streamed hash of a
  file larger than one chunk against a `shasum`-computed literal. The Codex review on 2026-11-25 added one
  regression test per bug it found: `malformed-llm-wrong-engine`, speech resolution never opening an LLM
  pack's weights, this build not verifying LLM packs, a failed repack leaving the old pack intact, and an
  out-dir inside the model dir being refused.

## What changes in the PRD

- **Runtime:** name the binding and version: `llama-cpp-2 =0.1.161` (llama.cpp v0.6.0), default features off,
  `GGML_METAL=OFF`, `LLAMA_STATIC_CRT=1`, OpenMP off, and the `common` fix from condition 1. JSON-schema to
  grammar conversion (`json_schema_to_grammar`) needs `common`; for P3 prefer GBNF grammars given directly to
  `LlamaSampler::grammar`, which works without it.
- **Delivery:** the pack manifest gains `task: "llm"` and `engine: "llama.cpp"`, only as a pair (done here); P2
  adds `llama.cpp` to the app's engine map when it links it; and resolution
  uses `resolve_verified_llm`, not `resolve_verified_models_dir`, which now ignores LLM packs. Decide the pack
  layout (condition 5).
- **Sizes:** the app binary grows by about 4 MB; the LLM pack is about 2.4 GB to download and 4.75 GB on disk
  as built here. Memory for Qwen3-4B Q4_0 at a 2k context is about 2.8 GB physical (S2 measured 4.4–4.7 GB at 8k
  context with the S2 setup).
- **Install paths:** the per-user NSIS install is unaffected (packs live in per-user app data). There is no
  MSI target yet, so the plan's MSI check doesn't apply.

## Still to do

- Load a model and generate on Windows (CI's self-check opens no inference session).
- Repeat the generation timings on the reference Windows laptop, ideally with the app's live ASR running.
- Build a release installer with llama.cpp linked to measure its size (only the binary was measured).
- The signer's self-check still knows sherpa-onnx as 1.13.2 while the app ships 2.0.1, so it would refuse a
  pack with `--engine-min-version 2.0.1`. This predates S5 and is left as it was.
