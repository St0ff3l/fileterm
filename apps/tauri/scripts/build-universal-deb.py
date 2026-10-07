#!/usr/bin/env python3
"""Bundle an amd64 DEB with a Debian 11-compatible private runtime.

Run inside a disposable Ubuntu 22.04 root containing runtime dependencies,
patchelf and Python 3. The release workflows use this script to produce the
single amd64 DEB with a private compatibility runtime.
"""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


PREFIX = Path("/usr/lib/fileterm/runtime")
LIBDIR = b"/usr/lib/x86_64-linux-gnu"
RELOCATIONS = (
    (LIBDIR, bytes(PREFIX)),
    (b"/usr/share/glvnd/egl_vendor.d", bytes(PREFIX) + b"/egl"),
)


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def relocate(runtime):
    counts = {old.decode(): 0 for old, _ in RELOCATIONS}
    for file in sorted(runtime.rglob("*")):
        if file.is_symlink():
            target = file.readlink()
            if target.is_absolute():
                destination = runtime / target.name
                if destination == file or not destination.exists():
                    raise RuntimeError(f"Cannot relocate symlink: {file} -> {target}")
                file.unlink()
                file.symlink_to(os.path.relpath(destination, file.parent))
            continue
        if not file.is_file():
            continue
        data = file.read_bytes()
        if not data.startswith(b"\x7fELF"):
            continue
        for old, new in RELOCATIONS:
            assert len(new) <= len(old)
            counts[old.decode()] += data.count(old)
            # Keep embedded string lengths intact; repeated '/' is valid in paths.
            data = data.replace(old, new + b"/" * (len(old) - len(new)))
        file.write_bytes(data)
        if file.name.startswith(("ld-linux", "libc.so")):
            continue
        run("patchelf", "--force-rpath", "--set-rpath", str(PREFIX), str(file))
        interpreter = subprocess.run(
            ["patchelf", "--print-interpreter", str(file)],
            capture_output=True,
        )
        if interpreter.returncode == 0:
            run(
                "patchelf", "--set-interpreter",
                str(PREFIX / "ld-linux-x86-64.so.2"), str(file),
            )
    if not all(counts.values()):
        raise RuntimeError(f"Expected runtime paths not found: {counts}")
    for file in runtime.rglob("*"):
        if file.is_symlink() and not file.exists():
            raise RuntimeError(f"Broken private runtime symlink: {file}")
    return counts


def write_control(stage):
    control = stage / "DEBIAN/control"
    lines = control.read_text().splitlines()
    replaced = {
        "Depends": "libc6 (>= 2.31), libgtk-3-0 | libgtk-3-0t64, xdg-utils, ca-certificates",
        "Installed-Size": str(sum(
            file.stat().st_size for file in stage.rglob("*")
            if file.is_file() and not file.is_symlink()
        ) // 1024),
    }
    for name, value in replaced.items():
        lines = [f"{name}: {value}" if line.startswith(f"{name}:") else line
                 for line in lines]
    control.write_text("\n".join(lines) + "\n")
    # Original package hashes no longer describe the relocated files.
    (stage / "DEBIAN/md5sums").unlink(missing_ok=True)


def build(input_deb, output_dir):
    release = Path("/etc/os-release").read_text()
    if 'VERSION_ID="22.04"' not in release or 'ID=ubuntu' not in release:
        raise RuntimeError("Use an isolated Ubuntu 22.04 runtime root")
    architecture = run("dpkg-deb", "-f", str(input_deb), "Architecture",
                       capture_output=True, text=True).stdout.strip()
    if architecture != "amd64":
        raise RuntimeError("Only amd64 universal packages are supported")
    version = run("dpkg-deb", "-f", str(input_deb), "Version",
                  capture_output=True, text=True).stdout.strip()
    output_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="fileterm-debian11-") as temp:
        stage = Path(temp) / "package"
        run("dpkg-deb", "-R", str(input_deb), str(stage))
        runtime = stage / PREFIX.relative_to("/")
        runtime.mkdir(parents=True)
        sources = {Path("/usr/lib/x86_64-linux-gnu").resolve(),
                   Path("/lib/x86_64-linux-gnu").resolve()}
        for source in sorted(sources):
            shutil.copytree(source, runtime, symlinks=True, dirs_exist_ok=True)
        shutil.copytree(Path("/usr/share/glvnd/egl_vendor.d"), runtime / "egl")
        launcher = stage / "usr/bin/fileterm"
        native = stage / "usr/lib/fileterm/fileterm"
        native.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(launcher, native)
        shutil.copy2(launcher, runtime / "fileterm")
        path_counts = relocate(runtime)
        launcher.write_text(
            '#!/bin/sh\n'
            'native=/usr/lib/fileterm/fileterm\n'
            'loader=/lib64/ld-linux-x86-64.so.2\n'
            '# Check dependencies and symbol versions before starting the app.\n'
            'if [ -x "$loader" ] && "$loader" --list "$native" >/dev/null 2>&1; then\n'
            '    exec "$native" "$@"\n'
            'fi\n'
            f'exec {PREFIX}/fileterm "$@"\n'
        )
        launcher.chmod(0o755)
        licenses = stage / "usr/share/doc/file-term/private-runtime"
        licenses.mkdir(parents=True)
        for copyright_file in Path("/usr/share/doc").glob("*/copyright"):
            destination = licenses / copyright_file.parent.name
            destination.mkdir()
            shutil.copy2(copyright_file, destination / "copyright", follow_symlinks=True)
        manifest = run(
            "dpkg-query", "-W",
            "-f=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n",
            capture_output=True, text=True,
        ).stdout
        (licenses / "runtime-packages.tsv").write_text(manifest)
        write_control(stage)
        artifact = output_dir / f"FileTerm-{version}-linux-x86_64.deb"
        run("dpkg-deb", "--root-owner-group", "-Zxz", "-z6", "--build", str(stage), str(artifact))
        print(artifact)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input_deb", type=Path)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    build(args.input_deb.resolve(strict=True), args.output_dir.resolve())
