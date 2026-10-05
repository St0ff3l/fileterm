#!/usr/bin/env bash
set -euo pipefail

dmg_file="${1:?Usage: verify-macos-dmg-finder-layout.sh <dmg-file>}"
if [[ ! -f "$dmg_file" ]]; then
  echo "DMG not found: $dmg_file" >&2
  exit 1
fi

mountpoint="$(mktemp -d)"
attached=false
cleanup() {
  if [[ "$attached" == true ]]; then
    hdiutil detach "$mountpoint" -quiet >/dev/null 2>&1 || true
  fi
  rmdir "$mountpoint" 2>/dev/null || true
}
trap cleanup EXIT

hdiutil attach -readonly -nobrowse -mountpoint "$mountpoint" "$dmg_file"
attached=true
test -s "$mountpoint/.DS_Store"
test -s "$mountpoint/.background/dmg-background.png"
