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

needs_rebuild=false
if find "$appdir" -type d ! -perm -0555 -print -quit | grep -q .; then
  needs_rebuild=true
fi
if find "$appdir" -type f -perm /0111 ! -perm -0005 -print -quit | grep -q .; then
  needs_rebuild=true
fi
if find "$appdir/AppRun" "$appdir/AppRun.wrapped" -maxdepth 0 ! -perm -0001 -print -quit | grep -q .; then
  needs_rebuild=true
fi

if [ "$needs_rebuild" = true ]; then
  # Firejail runs AppImage payloads as an unprivileged user. Root-owned 700
  # directories in the extracted AppDir prevent it from reaching AppRun.
  find "$appdir" -type d -exec chmod 755 {} +
  find "$appdir" -type f -perm /0111 -exec chmod a+rx {} +
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

if find "$appdir" -type d ! -perm -0555 -print -quit | grep -q .; then
  echo "An AppImage directory is not readable and traversable by all users in $appimage_file" >&2
  exit 1
fi
if find "$appdir" -type f -perm /0111 ! -perm -0005 -print -quit | grep -q .; then
  echo "An AppImage executable is not readable and executable by all users in $appimage_file" >&2
  exit 1
fi

echo "AppImage directories and launchers are accessible by all users: $appimage_file"
