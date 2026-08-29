#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$repo_root/scripts/modelpack-security.sh"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

printf 'trusted\n' > "$work/asset.bin"
printf '%s  %s\n' \
  7bd39a7cbcf687fd60f819645b8bcaf731a9f19cb102484a7b84530516d7e8b8 \
  asset.bin > "$work/assets.sha256"

verify_pinned_asset "$work/assets.sha256" "$work/asset.bin" asset.bin

printf 'tampered\n' > "$work/asset.bin"
if verify_pinned_asset "$work/assets.sha256" "$work/asset.bin" asset.bin; then
  echo "expected checksum mismatch to fail" >&2
  exit 1
fi

mkdir -p "$work/extracted/sub"
printf 'regular\n' > "$work/extracted/sub/model.onnx"
reject_archive_links "$work/extracted"

ln -s /etc/passwd "$work/extracted/sub/escape.onnx"
if reject_archive_links "$work/extracted"; then
  echo "expected archive symlink to fail" >&2
  exit 1
fi

echo "PASS: model-pack supply-chain checks"
