#!/usr/bin/env bash
# check-no-network.sh — the zero-egress guard rail (the heart of PR2).
#
# Fails (non-zero exit) if ANY known networking crate appears in the LINKED
# dependency graph of the hardened app or the engine. The product's core
# commercial differentiator is that outbound network is absent BY CONSTRUCTION
# (see docs/prd/local-private-transcription.md and docs/plans/vibe-fork-plan.md
# §4). This script proves it locally; .github/workflows/zero-egress.yml runs the
# same check in CI.
#
# Scope / allowlist note:
#   sherpa-onnx's build script downloads prebuilt C libs at BUILD time
#   (a one-time dev fetch, controllable via SHERPA_ONNX_LIB_DIR — see
#   engine/README.md). That is a build-script fetch, NOT a linked network crate,
#   and NOT client/runtime egress, so it is intentionally OUT of this gate's
#   scope. This gate targets *linked* runtime dependencies via `cargo tree`.
#
# Usage:  bash scripts/check-no-network.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_MANIFEST="$REPO_ROOT/app/src-tauri/Cargo.toml"
ENGINE_MANIFEST="$REPO_ROOT/engine/Cargo.toml"
MODELPACK_MANIFEST="$REPO_ROOT/modelpack/Cargo.toml"

# Denylist: HTTP clients, low-level HTTP/2, and the Vibe network plugins we must
# never reintroduce. Matched as whole crate names at the start of a `cargo tree
# --prefix none` line (so e.g. `hyper` matches `hyper`/`hyper-util` but not an
# unrelated `hyperlocal-ish` false positive mid-string).
DENYLIST=(
  reqwest
  hyper
  h2
  ureq
  isahc
  surf
  attohttpc
  curl
  tauri-plugin-http
  tauri-plugin-updater
  tauri-plugin-aptabase
  aptabase
)

# Build a single ERE alternation anchored at the crate-name position.
PATTERN="^($(IFS='|'; echo "${DENYLIST[*]}"))( |\$)"

FAIL=0

scan() {
  local label="$1"; shift
  local manifest="$1"; shift
  # Remaining args are extra `cargo tree` flags (e.g. --features hardened).
  echo "== Scanning: $label =="
  local tree
  if ! tree="$(cargo tree --manifest-path "$manifest" -e normal --prefix none "$@" 2>/dev/null)"; then
    echo "ERROR: cargo tree failed for $label" >&2
    FAIL=1
    return
  fi
  # Normalize: strip the trailing " (*)" dedup markers cargo emits.
  local offenders
  offenders="$(printf '%s\n' "$tree" | sed 's/ (\*)$//' | sort -u | grep -E "$PATTERN" || true)"
  if [[ -n "$offenders" ]]; then
    echo "  NETWORK CRATE(S) FOUND:" >&2
    printf '%s\n' "$offenders" | sed 's/^/    - /' >&2
    FAIL=1
  else
    echo "  OK — no network crates."
  fi
}

scan "app/src-tauri (default features)"   "$APP_MANIFEST"
scan "app/src-tauri (--features hardened)" "$APP_MANIFEST" --features hardened
scan "engine (default features)"          "$ENGINE_MANIFEST"
# The model-pack verification crate runs at the Rust trust boundary and must be
# network-free too (ed25519-dalek/sha2/serde have no network deps).
scan "modelpack (default features)"        "$MODELPACK_MANIFEST"

echo
if [[ "$FAIL" -ne 0 ]]; then
  echo "FAIL: zero-egress gate tripped — a network crate is in the dependency graph." >&2
  exit 1
fi
echo "PASS: zero-egress gate clean — no network crates in app, engine, or modelpack."
exit 0
