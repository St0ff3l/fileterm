---
name: fileterm-app-icons
description: Use for FileTerm desktop application icon artwork and platform assets, including Dock, shortcut, window, taskbar, launcher, tray, and DeepSeek Harness plugin icons, especially when fixing sizing or blur. Do not use for renderer file-type icons or general UI icons.
---

# FileTerm application icons

## Scope and source artwork

This skill covers the branded application icon and its operating-system assets. For file-type, folder, or general renderer icons, use the [common-components skill](../fileterm-common-components/SKILL.md).

Read [platform and protocol contracts](../../../docs/quality/agent-platform-contracts.md) before changing platform outputs. The editable Icon Composer projects live in `apps/tauri/assets/icons/source/`. Export the Windows/Linux full-bleed masters directly to `apps/tauri/src-tauri/icons/app/shared/{classic,material,clean}-1024.png`; keep the macOS Dock images in `apps/tauri/src/renderer/assets/app-icons/dock-{classic,material,clean}.png`.

The source projects are `FileTerm.icon`, `FileTerm-material.icon`, and `FileTerm-clean.icon`, matching the classic, material, and clean variants. Their standalone SVG directories are `svg/`, `svg-material/`, and `svg-clean/`.

The 1024px Dock images keep a 100px transparent inset on each side (about 9.8%). macOS needs this inset so the icon does not look oversized. Windows/Linux exports have alpha bounds `(0, 0, 1024, 1024)`, with no macOS safety inset; transparent corners belonging to the icon shape are preserved. Export these masters from the layered source rather than enlarging a cropped 824px Dock bitmap or a 256/512px output.

## DeepSeek Harness plugin icons

A DSH bundle row reads the `./icon` export of its own package. Reuse `apps/tauri/assets/dsh-plugin-icons/fileterm-512.png` (exported from `dock-classic.png` with its transparent margin) instead of drawing a new icon: copy it into the bundle as `icon.png` and export `"./icon": "./icon.png"`. See `apps/tauri/assets/dsh-plugin-icons/README.md` for the export command and the in-package, 256 KiB constraints.

## Platform differences

Taskbar and Dock icons use 1024×1024px artwork on all three platforms. Windows/Linux use full-bleed masters; macOS keeps its safety inset. Do not upscale a small tray bitmap for the taskbar or Dock.

- **macOS:** the Dock uses `dock-*.png`; Finder, Launchpad, and the packaged app use the checked-in `apps/tauri/src-tauri/icons/app/macos/icon.icns`. The generator does not update either macOS asset. Update or verify the static ICNS separately when the default artwork changes. The menu-bar template icons in `icons/tray/macos/` are a separate monochrome asset family.
- **Windows:** both runtime window HICONs use 1024px PNGs through `CreateIconFromResourceEx`; Windows scales them for the taskbar and other window surfaces. The tray uses the derived 256px PNG. Shortcut/install ICOs retain their standard 16–256px frames and are generated from the 1024px master; do not try to put a nonstandard 1024px frame into these ICOs.
- **Linux:** windows and runtime launchers use 1024px images from `icons/app/shared/`. Packages install the classic 1024px image into hicolor and pixmaps, alongside the smaller hicolor sizes. Package icons and tray images are full-bleed 8-bit RGBA. Preserve user-customized icons when changing launcher refresh behavior.

## Updating a variant

1. Edit the relevant layered `.icon` project. Export its Default rendition at 1024×1024px, scale 1, directly to `icons/app/shared/<variant>-1024.png` with Icon Composer or its bundled `ictool --export-image`. If also updating macOS, keep the 100px transparent inset in the corresponding `dock-*.png`.
2. Run `uv run --with pillow python apps/tauri/scripts/generate-platform-app-icons.py` to normalize the masters to 8-bit sRGB RGBA and derive the 256/512px PNGs, ICOs, package icons, and tray assets. The script validates their dimensions and alpha bounds. Icon Composer can export 16-bit Display P3 PNGs; native RGBA loaders do not apply the embedded color profile. Do not hand-edit derived outputs.
3. Review the generated diff and check the affected platform consumers. Preserve the macOS inset, full-bleed Windows/Linux artwork, and Linux 8-bit RGBA requirement.

Native Windows loading must still be checked on Windows; decoding the source PNG on macOS does not prove that HICON creation succeeded.
