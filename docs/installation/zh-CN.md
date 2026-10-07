# FileTerm 安装指南

本指南适用于 FileTerm 桌面版。请只从 [FileTerm GitHub Releases](https://github.com/St0ff3l/fileterm/releases) 下载。日常使用请选择最新正式版；测试版本请从 Releases 列表中选择标记为 prerelease 的版本。

## 选择正确的安装包

- **Windows**：x64 电脑下载文件名含 `windows-x64` 的安装包；Windows ARM 设备下载含 `windows-arm64` 的安装包。
- **macOS**：Apple Silicon（M 系列）下载 `macos-arm64.dmg`；Intel Mac 下载 `macos-x64.dmg`。
- **Linux**：运行 `uname -m`。输出 `x86_64` 时选 `linux-x86_64`；输出 `aarch64` 或 `arm64` 时选 `linux-arm64`。

不要混用不同架构的安装包。

## Windows

### 安装版

1. 下载与设备架构匹配的 `FileTerm-<版本>-windows-x64-setup.exe` 或 `FileTerm-<版本>-windows-arm64-setup.exe`。
2. 运行安装器并按提示完成安装。
3. 从开始菜单启动 FileTerm。

安装版提供签名的应用内更新。发现新版本后，FileTerm 会下载并验证对应架构的更新包，再提示重启完成更新。

### 便携版

1. 下载 `FileTerm-<版本>-windows-x64-portable.exe` 或 `FileTerm-<版本>-windows-arm64-portable.exe`。
2. 将文件放在有写入权限的目录后运行。

便携版不需要安装，也不写入安装器注册信息；配置保存在可执行文件旁的 `config/` 目录。它需要系统已安装 WebView2 Runtime。便携版配置与凭据受当前 Windows 设备保护；把配置复制到另一台电脑后，需要重新配置凭据。

## macOS

1. 根据芯片类型下载 `FileTerm-<版本>-macos-arm64.dmg` 或 `FileTerm-<版本>-macos-x64.dmg`。
2. 打开 DMG，将 FileTerm 拖到“应用程序”文件夹。
3. 从“应用程序”文件夹启动。

Apple Silicon 版本要求 macOS 11 或更新版本；Intel 版本的最低系统版本为 macOS 10.13。macOS 安装包未经过 Apple 公证，首次打开时系统可能显示安全提示。确认文件来自本项目的 GitHub Releases 后，可按系统提示在“系统设置 → 隐私与安全性”中允许打开。

FileTerm 的 macOS 更新检查会打开 GitHub Releases 页面。下载对应架构的新 DMG 后，将应用拖到“应用程序”文件夹覆盖旧版本。

## Linux

### 架构对应关系

| `uname -m` 输出      | Release 文件名中的架构 |
| -------------------- | ---------------------- |
| `x86_64`             | `x86_64`               |
| `aarch64` 或 `arm64` | `arm64`                |

### 选择软件包格式

| 格式           | 适用范围                                           | 安装或运行                                                                                                                        |
| -------------- | -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `.AppImage`    | 支持 AppImage 的桌面发行版                         | 先运行 `chmod +x FileTerm-<版本>-linux-<架构>.AppImage`，然后运行该文件                                                           |
| `.deb`         | Debian、Ubuntu 及其衍生版                          | `sudo apt install ./FileTerm-<版本>-linux-<架构>.deb`                                                                             |
| `.rpm`         | Fedora、openSUSE 等能提供所需运行库的 RPM 发行版   | Fedora：`sudo dnf install ./FileTerm-<版本>-linux-<架构>.rpm`；openSUSE：`sudo zypper install ./FileTerm-<版本>-linux-<架构>.rpm` |
| `.pkg.tar.zst` | x86_64 的 Arch Linux 系；aarch64 的 Arch Linux ARM | `sudo pacman -U ./FileTerm-<版本>-linux-<架构>.pkg.tar.zst`                                                                       |

x86_64 `.deb` 内含 Debian 11 兼容运行库，并会在系统运行库可用时自动使用系统版本。通过 APT 升级 `.deb` 时，私有运行库会随 FileTerm 一起更新。arm64 `.deb` 使用系统运行库，要求 Debian 12 或更新版本。RPM 和 Pacman 包使用系统运行库；RPM 要求发行版提供 WebKitGTK 4.1 和 OpenSSL 3。AppImage 随包提供所需的运行库，但仍依赖与 Ubuntu 22.04 构建基线兼容的系统 glibc。

Arch Linux 官方发行版目前使用 x86_64 包；aarch64 包面向独立的 Arch Linux ARM 发行版。请勿把 ARM 包安装到 x86_64 系统，或反过来。

AppImage 首次运行示例：

```sh
chmod +x FileTerm-<版本>-linux-x86_64.AppImage
./FileTerm-<版本>-linux-x86_64.AppImage
```

将文件名中的 `x86_64` 换成 `arm64`，即可运行 ARM64 版本。Release 中的 `.zsync` 文件可供 AppImageUpdate 等工具执行差量更新。Linux 应用内“检查更新”会打开 GitHub Releases 页面；DEB、RPM 和 Pacman 用户下载匹配的新版本后，使用相同包管理器命令升级。

## 更新与帮助

若 Arch + LXQt 中“打开下载页面”或更新提示无法启动浏览器，先运行 `sudo pacman -S --needed xdg-utils qtxdg-tools`，然后用 `xdg-open https://github.com/St0ff3l/fileterm/releases` 检查系统链接打开能力。若仍然失败，请在 LXQt 的默认应用设置中关联 HTTP/HTTPS 浏览器，并保留终端错误信息。FileTerm 的链接打开失败也会写入应用日志。

- Windows 安装版使用签名的应用内更新；按提示重启即可完成更新。
- macOS 与 Linux 可在应用内打开 GitHub Releases 页面，下载匹配架构的新版本。
- AppImage 可直接替换为新版本，也可使用对应的 `.zsync` 文件和 AppImageUpdate。
- 如果安装或启动失败，请在 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交操作系统、处理器架构、FileTerm 版本和脱敏后的错误信息。不要提交密码、私钥或 token。
