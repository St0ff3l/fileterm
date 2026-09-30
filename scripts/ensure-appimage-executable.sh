#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 path/to/FileTerm.AppImage" >&2
  exit 2
fi

appimage_file="$(realpath "$1")"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

chmod a+x "$appimage_file"
(
  cd "$work_dir"
  "$appimage_file" --appimage-extract >/dev/null
)

appdir="$work_dir/squashfs-root"
for launcher in AppRun AppRun.wrapped; do
  test -f "$appdir/$launcher"
done

if [ -n "$(find "$appdir/AppRun" "$appdir/AppRun.wrapped" -maxdepth 0 ! -perm -0001 -print -quit)" ]; then
  chmod 755 "$appdir/AppRun" "$appdir/AppRun.wrapped"

  tool="$work_dir/appimagetool-x86_64.AppImage"
  curl --fail --location --silent --show-error \
    'https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage' \
    --output "$tool"
  echo 'ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  '"$tool" | sha256sum --check --status
  chmod a+x "$tool"

  rebuilt="$work_dir/FileTerm-rebuilt.AppImage"
  ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" "$appdir" "$rebuilt"
  chmod a+x "$rebuilt"
  mv "$rebuilt" "$appimage_file"

  rm -rf "$appdir"
  (
    cd "$work_dir"
    "$appimage_file" --appimage-extract >/dev/null
  )
fi

for launcher in AppRun AppRun.wrapped; do
  test -n "$(find "$appdir/$launcher" -maxdepth 0 -perm -0001 -print -quit)" || {
    echo "$launcher is not executable by other users in $appimage_file" >&2
    exit 1
  }
done

echo "AppImage launchers are executable by all users: $appimage_file"
