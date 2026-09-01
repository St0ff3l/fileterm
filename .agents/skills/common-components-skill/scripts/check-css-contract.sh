#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
RENDERER="$REPO_ROOT/apps/tauri/src/renderer"
TOKEN_DIR="$RENDERER/styles/tokens"
SEMANTIC="$TOKEN_DIR/semantic.css"
COMMON="$RENDERER/components/common"
FEATURES="$RENDERER/features"

if [[ ! -d "$RENDERER" || ! -f "$SEMANTIC" ]]; then
  echo "[ERROR] FileTerm renderer or semantic.css was not found."
  echo "       Run this script from the FileTerm repository."
  exit 2
fi

failures=0

check_matches() {
  local label="$1"
  local pattern="$2"
  shift 2
  local output

  if output="$(rg -n --pcre2 --glob '*.css' --glob '*.tsx' "$pattern" "$@" 2>/dev/null)"; then
    echo "[FAIL] $label"
    echo "$output"
    failures=$((failures + 1))
  fi
}

check_css_matches() {
  local label="$1"
  local pattern="$2"
  shift 2
  local output

  if output="$(rg -n --pcre2 --glob '*.css' "$pattern" "$@" 2>/dev/null)"; then
    echo "[FAIL] $label"
    echo "$output"
    failures=$((failures + 1))
  fi
}

check_matches "semantic.css contains a direct color value" \
  '#[0-9a-fA-F]{3,8}\b|rgba?\(' \
  "$SEMANTIC"

check_matches "canonical common component code contains a direct color value" \
  '#[0-9a-fA-F]{3,8}\b|rgba?\(' \
  "$COMMON"

check_css_matches "new feature CSS contains a direct color value" \
  '#[0-9a-fA-F]{3,8}\b|rgba?\(' \
  "$FEATURES"

check_matches "canonical common component code skips the semantic layer" \
  'var\(--ref-[a-z0-9-]+' \
  "$COMMON"

check_css_matches "new feature CSS skips the semantic layer" \
  'var\(--ref-[a-z0-9-]+' \
  "$FEATURES"

check_matches "canonical common component code uses a legacy color alias" \
  'var\(--(primary|primary-hover|primary-active|bg-[a-z0-9-]+|text-main|accent-highlight|theme-(accent|surface|text)[a-z0-9-]*|danger([a-z0-9-]+)?|success([a-z0-9-]+)?|warning([a-z0-9-]+)?|info([a-z0-9-]+)?)\b' \
  "$COMMON"

check_css_matches "new feature CSS uses a legacy color alias" \
  'var\(--(primary|primary-hover|primary-active|bg-[a-z0-9-]+|text-main|accent-highlight|theme-(accent|surface|text)[a-z0-9-]*|danger([a-z0-9-]+)?|success([a-z0-9-]+)?|warning([a-z0-9-]+)?|info([a-z0-9-]+)?)\b' \
  "$FEATURES"

important_count="$(
  (rg -n --glob '*.css' '!important' "$COMMON" "$FEATURES" 2>/dev/null || true) | wc -l | tr -d ' '
)"
if [[ "$important_count" != "0" ]]; then
  echo "[WARN] canonical component/feature CSS contains $important_count !important occurrence(s)."
  echo "       Keep only documented native-control compatibility exceptions."
fi

legacy_count="$(
  (rg -n --glob '*.css' --pcre2 '#[0-9a-fA-F]{3,8}\b|rgba?\(' \
    "$RENDERER/styles/features" \
    "$RENDERER/styles/global.css" \
    "$RENDERER/styles/workstation.css" \
    "$RENDERER/styles/app.css" \
    2>/dev/null || true) | wc -l | tr -d ' '
)"
if [[ "$legacy_count" != "0" ]]; then
  echo "[INFO] legacy styles still contain approximately $legacy_count direct color occurrence(s)."
  echo "       This is migration debt; do not add new occurrences."
fi

for theme_file in \
  "$TOKEN_DIR/fileterm-dark.css" \
  "$TOKEN_DIR/fileterm-light.css" \
  "$TOKEN_DIR/codex-dark.css" \
  "$TOKEN_DIR/codex-light.css"; do
  if [[ ! -f "$theme_file" ]]; then
    echo "[FAIL] missing built-in theme file: $theme_file"
    failures=$((failures + 1))
  fi
done

if (( failures > 0 )); then
  echo "[ERROR] CSS contract failed with $failures blocking check(s)."
  exit 1
fi

echo "[OK] CSS contract passed. Warnings and legacy debt above are non-blocking."
