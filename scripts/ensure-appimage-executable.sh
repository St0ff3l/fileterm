#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "Usage: $0 path/to/FileTerm.AppImage [update-information]" >&2
  exit 2
fi

command -v zsyncmake >/dev/null
command -v python3 >/dev/null
appimage_file="$(realpath "$1")"
update_information="${2:-gh-releases-zsync|St0ff3l|fileterm|latest|FileTerm-*-linux-x86_64.AppImage.zsync}"
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

# Runtime extraction may apply private permissions to its temporary tree, so
# verify the repacked SquashFS directly instead of inspecting that temp tree.
find "$appdir" -type d -exec chmod 755 {} +
find "$appdir" -type f -perm /0111 -exec chmod a+rx {} +
chmod 755 "$appdir/AppRun" "$appdir/AppRun.wrapped"

tool="$work_dir/appimagetool-x86_64.AppImage"
curl --fail --location --silent --show-error \
  'https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage' \
  --output "$tool"
printf 'ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  %s\n' "$tool" | sha256sum --check --status
chmod a+x "$tool"

rebuilt="$work_dir/$(basename "$appimage_file")"
(
  cd "$work_dir"
  ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" -u "$update_information" "$appdir" "$(basename "$rebuilt")"
)
chmod a+x "$rebuilt"

test -s "$rebuilt.zsync"
mv "$rebuilt" "$appimage_file"
mv "$rebuilt.zsync" "$appimage_file.zsync"

embedded_update="$("$appimage_file" --appimage-updateinformation)"
if [ "$embedded_update" != "$update_information" ]; then
  echo "AppImage update information does not match the requested channel" >&2
  exit 1
fi
python3 scripts/verify-appimage-zsync.py "$appimage_file"

squashfs_offset="$("$appimage_file" --appimage-offset)"
root_access="$(unsquashfs -lls -no-progress -offset "$squashfs_offset" "$appimage_file" | awk '$NF == "squashfs-root" { print substr($1, 8, 1) substr($1, 10, 1); exit }')"
if [ "$root_access" != "rx" ]; then
  echo "AppImage root directory is not readable and traversable by other users: ${root_access:-missing metadata}" >&2
  exit 1
fi

payload_dir="$work_dir/verified-payload"
unsquashfs -no-progress -offset "$squashfs_offset" -d "$payload_dir" "$appimage_file" >/dev/null

for launcher in AppRun AppRun.wrapped; do
  test -n "$(find "$payload_dir/$launcher" -maxdepth 0 -perm -0001 -print -quit)" || {
    echo "$launcher is not executable by other users in $appimage_file" >&2
    exit 1
  }
done

untraversable_dir="$(find "$payload_dir" -mindepth 1 -type d ! -perm -0005 -print -quit)"
if [ -n "$untraversable_dir" ]; then
  echo "An AppImage directory is not readable and traversable by other users in $appimage_file: $(stat -c '%a %u:%g %n' "$untraversable_dir")" >&2
  exit 1
fi
if find "$payload_dir" -type f -perm /0111 ! -perm -0005 -print -quit | grep -q .; then
  echo "An AppImage executable is not readable and executable by all users in $appimage_file" >&2
  exit 1
fi

echo "AppImage payload permissions are accessible by other users: $appimage_file"
