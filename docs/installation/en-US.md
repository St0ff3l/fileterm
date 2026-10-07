# FileTerm Installation Guide

This guide covers the FileTerm desktop app. Download FileTerm only from [FileTerm GitHub Releases](https://github.com/St0ff3l/fileterm/releases). For everyday use, choose the latest stable release. To test prerelease builds, select a release marked as a prerelease.

## Choose the Correct Download

- **Windows**: choose an installer whose name contains `windows-x64` for an x64 PC, or `windows-arm64` for a Windows ARM device.
- **macOS**: Apple silicon (M-series) Macs should use `macos-arm64.dmg`; Intel Macs should use `macos-x64.dmg`.
- **Linux**: run `uname -m`. Choose `linux-x86_64` for `x86_64`, or `linux-arm64` for `aarch64` or `arm64`.

Do not install a package built for a different architecture.

## Windows

### Installer

1. Download `FileTerm-<version>-windows-x64-setup.exe` or `FileTerm-<version>-windows-arm64-setup.exe` for your device.
2. Run the installer and follow its prompts.
3. Launch FileTerm from the Start menu.

Installed builds support signed in-app updates. FileTerm downloads and verifies the update for your architecture, then asks you to restart to complete the update.

### Portable

1. Download `FileTerm-<version>-windows-x64-portable.exe` or `FileTerm-<version>-windows-arm64-portable.exe`.
2. Place it in a directory where you have write access and run it.

The portable build does not install files or register an uninstaller. It stores its configuration in a `config/` directory beside the executable and requires the WebView2 Runtime to be installed on Windows. Its configuration and credentials are protected for the current Windows device; credentials must be set up again if you move the configuration to another computer.

## macOS

1. Download `FileTerm-<version>-macos-arm64.dmg` or `FileTerm-<version>-macos-x64.dmg` for your Mac.
2. Open the DMG and drag FileTerm to the Applications folder.
3. Launch FileTerm from Applications.

The Apple silicon build requires macOS 11 or later. The Intel build has a minimum system version of macOS 10.13. The macOS package is not notarized by Apple, so macOS may show a security warning on first launch. After confirming that the file came from this project's GitHub Releases, follow macOS prompts to allow it under **System Settings → Privacy & Security**.

FileTerm's macOS update check opens GitHub Releases. Download the DMG for your architecture and drag the app to Applications to replace the previous version.

## Linux

### Match Your Architecture

| `uname -m` output    | Architecture tag in release filenames |
| -------------------- | ------------------------------------- |
| `x86_64`             | `x86_64`                              |
| `aarch64` or `arm64` | `arm64`                               |

### Choose a Package Format

| Format         | For                                                                                       | Install or run                                                                                                                          |
| -------------- | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `.AppImage`    | Desktop distributions with AppImage support                                               | Run `chmod +x FileTerm-<version>-linux-<arch>.AppImage`, then launch the file                                                           |
| `.deb`         | Debian, Ubuntu, and derivatives                                                           | `sudo apt install ./FileTerm-<version>-linux-<arch>.deb`                                                                                |
| `.rpm`         | Fedora, openSUSE, and other RPM distributions that provide the required runtime libraries | Fedora: `sudo dnf install ./FileTerm-<version>-linux-<arch>.rpm`; openSUSE: `sudo zypper install ./FileTerm-<version>-linux-<arch>.rpm` |
| `.pkg.tar.zst` | Arch Linux derivatives on x86_64; Arch Linux ARM on aarch64                               | `sudo pacman -U ./FileTerm-<version>-linux-<arch>.pkg.tar.zst`                                                                          |

The x86_64 `.deb` includes a compatibility runtime for Debian 11 and selects the system runtime automatically when available. Its private runtime is updated together with FileTerm when you upgrade the `.deb` through APT. The arm64 `.deb` uses system libraries and requires Debian 12 or newer. RPM and Pacman packages use system libraries; the RPM requires a distribution that provides WebKitGTK 4.1 and OpenSSL 3. The AppImage includes its runtime libraries but still requires host glibc compatible with the Ubuntu 22.04 build baseline.

The official Arch Linux distribution currently uses the x86_64 package. The aarch64 package is for the separate Arch Linux ARM distribution. Do not install the ARM package on an x86_64 system or vice versa.

Example for launching an AppImage:

```sh
chmod +x FileTerm-<version>-linux-x86_64.AppImage
./FileTerm-<version>-linux-x86_64.AppImage
```

Replace `x86_64` in the filename with `arm64` to run the ARM64 build. The `.zsync` sidecar in Releases can be used by AppImageUpdate and similar tools for differential updates. The Linux in-app update check opens GitHub Releases. To update a DEB, RPM, or Pacman installation, download the matching package and use the same package-manager command.

## Updates and Support

If the download-page button or update hint cannot launch a browser on Arch + LXQt, run `sudo pacman -S --needed xdg-utils qtxdg-tools`, then test the system launcher with `xdg-open https://github.com/St0ff3l/fileterm/releases`. If it still fails, configure the HTTP/HTTPS browser association in LXQt's default application settings and retain the terminal error. FileTerm also records link-launch failures in its application log.

- Windows installer builds use signed in-app updates. Restart when prompted to finish updating.
- On macOS and Linux, the in-app update check opens GitHub Releases so you can download the package for your architecture.
- AppImages can be replaced with a newer download or updated with AppImageUpdate and the matching `.zsync` file.
- If installation or startup fails, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your OS, processor architecture, FileTerm version, and redacted error details. Do not include passwords, private keys, or tokens.
