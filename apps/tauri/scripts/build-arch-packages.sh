#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 3 ]]; then
  echo "Usage: $0 <version> <linux-artifacts-dir> <output-dir>" >&2
  exit 2
fi

version="$1"
linux_artifacts_dir="$2"
output_dir="$3"
if [[ ! "$version" =~ ^[A-Za-z0-9.+-]+$ ]]; then
  echo "Invalid release version: $version" >&2
  exit 2
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/../../.." && pwd)"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/fileterm-arch-packages.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT
mkdir -p "$output_dir"

for entry in "x86_64:x86_64" "arm64:aarch64"; do
  asset_arch="${entry%%:*}"
  package_arch="${entry##*:}"
  deb_name="FileTerm-${version}-linux-${asset_arch}.deb"
  deb_file="$(find "$linux_artifacts_dir" -type f -name "$deb_name" -print -quit)"
  build_dir="$work_dir/$package_arch"

  if [[ -z "$deb_file" ]]; then
    echo "Missing Linux DEB input: $deb_name" >&2
    echo "Available Linux package inputs:" >&2
    find "$linux_artifacts_dir" -maxdepth 5 -type f -print >&2
    exit 1
  fi

  deb_arch="$(dpkg-deb --field "$deb_file" Architecture 2>/dev/null || true)"
  expected_deb_arch="amd64"
  if [[ "$asset_arch" == "arm64" ]]; then
    expected_deb_arch="arm64"
  fi
  if [[ "$deb_arch" != "$expected_deb_arch" ]]; then
    echo "Unexpected DEB architecture for $asset_arch input: $deb_arch" >&2
    exit 1
  fi

  mkdir -p "$build_dir/stage"
  dpkg-deb --extract "$deb_file" "$build_dir/stage"
  for required_file in \
    usr/bin/fileterm \
    usr/share/applications/FileTerm.desktop \
    usr/share/metainfo/com.fileterm.desktop.metainfo.xml \
    usr/share/pixmaps/fileterm.png \
    usr/share/licenses/file-term/LICENSE; do
    if [[ ! -e "$build_dir/stage/$required_file" ]]; then
      echo "DEB input is missing required package content: $required_file" >&2
      exit 1
    fi
  done
  cp "$repo_root/apps/tauri/packaging/arch/PKGBUILD" "$build_dir/PKGBUILD"
done

docker run --rm \
  --platform linux/amd64 \
  --volume "$work_dir:/work" \
  --env "FILETERM_VERSION=$version" \
  archlinux:base-devel \
  bash -euc '
    pacman -Syu --noconfirm --needed base-devel
    builder_uid="$(stat -c "%u" /work)"
    useradd --uid "$builder_uid" --user-group --create-home builder

    cp /etc/makepkg.conf /work/makepkg-x86_64.conf
    cp /etc/makepkg.conf /work/makepkg-aarch64.conf
    sed -i '\''s/^CARCH=.*/CARCH="aarch64"/'\'' /work/makepkg-aarch64.conf
    chown -R builder:builder /work

    for entry in x86_64:x86_64 arm64:aarch64; do
      asset_arch="${entry%%:*}"
      package_arch="${entry##*:}"
      cd "/work/$package_arch"
      runuser -u builder -- env FILETERM_VERSION="$FILETERM_VERSION" \
        makepkg --config "/work/makepkg-$package_arch.conf" \
          --noextract --nodeps --noconfirm --force
      package_file="$(find . -maxdepth 1 -type f -name "file-term-*.pkg.tar.zst" -print -quit)"
      test -n "$package_file"
      cp "$package_file" "/work/FileTerm-${FILETERM_VERSION}-linux-${asset_arch}.pkg.tar.zst"

      pkginfo="$(tar --zstd -xOf "$package_file" .PKGINFO)"
      grep -Fqx "pkgname = file-term" <<< "$pkginfo"
      grep -Fqx "arch = $package_arch" <<< "$pkginfo"
      grep -Fqx "license = MIT" <<< "$pkginfo"
      grep -Fqx "depend = gtk3" <<< "$pkginfo"
      grep -Fqx "depend = webkit2gtk-4.1" <<< "$pkginfo"
      grep -Fqx "depend = libayatana-appindicator" <<< "$pkginfo"
      grep -Fqx "depend = openssl" <<< "$pkginfo"
      tar --zstd -tf "$package_file" | grep -Fxq "usr/bin/fileterm"
      tar --zstd -tf "$package_file" | grep -Fxq "usr/share/metainfo/com.fileterm.desktop.metainfo.xml"
      tar --zstd -tf "$package_file" | grep -Fxq "usr/share/licenses/file-term/LICENSE"
    done
  '

for asset_arch in x86_64 arm64; do
  package_file="$work_dir/FileTerm-${version}-linux-${asset_arch}.pkg.tar.zst"
  test -f "$package_file"
  mv "$package_file" "$output_dir/"
done
