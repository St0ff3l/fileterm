#!/usr/bin/env python3
"""Reject stale or misnamed delta metadata before uploading release assets."""

import hashlib
from pathlib import Path
import sys


def main():
    if len(sys.argv) != 2:
        raise SystemExit("Usage: verify-appimage-zsync.py path/to/FileTerm.AppImage")
    appimage = Path(sys.argv[1])
    with Path(str(appimage) + ".zsync").open("rb") as metadata:
        fields = {}
        for line in metadata:
            if line in (b"\n", b"\r\n"):
                break
            key, separator, value = line.decode("utf-8").strip().partition(": ")
            if separator:
                fields[key] = value
    digest = hashlib.sha1()
    with appimage.open("rb") as payload:
        for chunk in iter(lambda: payload.read(1024 * 1024), b""):
            digest.update(chunk)
    expected = {
        "Filename": appimage.name,
        "URL": appimage.name,
        "Length": str(appimage.stat().st_size),
        "SHA-1": digest.hexdigest(),
    }
    for key, value in expected.items():
        if fields.get(key) != value:
            raise SystemExit(f"Invalid zsync {key}: expected {value}, got {fields.get(key)}")
    print(f"Verified delta metadata for {appimage.name}")


if __name__ == "__main__":
    main()
