#!/bin/bash
# Cross-build the Windows x64 installer (NSIS -setup.exe) on a Mac, without GitHub Actions.
#
# Usage: scripts/build-windows-on-mac.sh /path/to/vendor-public.spki.pem
#
# The key must be the public key the official model packs are signed with, or the packs won't
# import. It is compiled into every official release; to recover it from one, mount the release's
# .dmg and run: strings -n 8 Folio.app/Contents/MacOS/kindtech-transcribe-app | grep -A2 "BEGIN PUBLIC KEY"
#
# One-time setup:
#   brew install nsis llvm lld
#   rustup target add x86_64-pc-windows-msvc
#   cargo install --locked cargo-xwin
#
# Tauri calls cross-compilation experimental, and the installer is unsigned. Test the result on Windows.
# Output: app/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/Folio_<version>_x64-setup.exe
set -euo pipefail

key="${1:?usage: $0 /path/to/vendor-public.spki.pem}"
[ -f "$key" ] || { echo "no such key file: $key" >&2; exit 1; }
cd "$(dirname "$0")/../app"

export PATH="/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$HOME/.cargo/bin:$PATH"
for tool in makensis lld-link llvm-rc cargo-xwin; do
  command -v "$tool" >/dev/null || { echo "missing $tool; see the one-time setup in $0" >&2; exit 1; }
done
export XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-$HOME/.cache/cargo-xwin}" # Windows SDK, downloaded once
VENDOR_PUBKEY_SPKI_PEM="$(cat "$key")"
export VENDOR_PUBKEY_SPKI_PEM

npm ci
npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc --features hardened

exe=src-tauri/target/x86_64-pc-windows-msvc/release/kindtech-transcribe-app.exe
# The build must have embedded this key, and nothing but Windows' own DLLs may be needed at runtime.
# grep -q would exit early and SIGPIPE strings, failing the pipeline under pipefail.
strings -n 8 "$exe" | grep -F "$(sed -n 2p "$key")" >/dev/null || { echo "key not embedded in $exe" >&2; exit 1; }
if llvm-objdump -p "$exe" | grep "DLL Name" | grep -viE "api-ms-win-|advapi32|bcryptprimitives|combase|comctl32|dbghelp|dwmapi|dxgi|gdi32|kernel32|mmdevapi|ntdll|ole32|oleaut32|setupapi|shell32|shlwapi|user32|uxtheme|version|winmm|ws2_32|crypt32|bcrypt|userenv|propsys"; then
  echo "the exe needs a DLL Windows doesn't ship (listed above)" >&2; exit 1
fi
ls -l src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe
