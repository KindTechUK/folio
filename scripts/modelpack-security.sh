#!/usr/bin/env bash

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

verify_pinned_asset() {
  local manifest="$1"
  local file="$2"
  local asset="$3"
  local matches expected actual

  matches="$(awk -v asset="$asset" '$2 == asset { print $1 }' "$manifest")"
  if [[ -z "$matches" || "$matches" == *$'\n'* ]]; then
    echo "missing or duplicate pinned checksum for $asset" >&2
    return 1
  fi
  expected="$matches"
  actual="$(sha256_file "$file")"
  if [[ "$actual" != "$expected" ]]; then
    echo "checksum mismatch for $asset: expected $expected, got $actual" >&2
    return 1
  fi
}

reject_archive_links() {
  local root="$1"
  local link
  link="$(find "$root" -type l -print -quit)"
  if [[ -n "$link" ]]; then
    echo "archive contains a symbolic link: $link" >&2
    return 1
  fi
}

require_regular_file() {
  local file="$1"
  if [[ ! -f "$file" || -L "$file" ]]; then
    echo "expected a regular non-link file: $file" >&2
    return 1
  fi
}
