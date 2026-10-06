# Linux ARM64、RPM 与 Windows ARM64 安装包

## 目标与范围

发行版补齐 Linux ARM64，并让 x86_64/ARM64 Linux 都提供 `.deb`、`.rpm` 和 AppImage；另提供 Arch x86_64 与 Arch Linux ARM aarch64 的 Pacman `.pkg.tar.zst`。Windows 增加 ARM64 NSIS 与 portable 产物，并让签名更新清单按架构选择安装器或 portable EXE。保留 macOS 现有 arm64/x64 DMG 和 Windows x64 资产。

## 实现

- Tauri Linux release bundle 启用 `deb`、`rpm`、`appimage`；三种格式共同携带桌面入口、AppStream 元数据、图标和许可证。DEB 声明 OpenSSL 运行依赖，RPM 通过共享库 SONAME 声明 WebKitGTK、GTK、AppIndicator 和 OpenSSL 依赖。GitHub Actions 在 Ubuntu 22.04 x64 与原生 ARM64 runner 上分别打包，验证 DEB/RPM 元数据架构及 AppImage/zsync 文件名。
- Linux DEB 解包后在官方 Arch `base-devel` 容器中使用 `makepkg` 生成 Pacman 包；x86_64 包目标 Arch Linux/Manjaro/EndeavourOS，aarch64 包目标 Arch Linux ARM。构建校验包架构、依赖、桌面文件、AppStream、图标和许可证，并单独上传两个 `.pkg.tar.zst` Release 资产。
- Windows release job 使用 x64 与原生 ARM64 runner，构建并签名 NSIS 安装器、portable EXE；最终 release job 合并两边签名生成的清单，保留独立的 x64、ARM64 安装器和 portable 更新键。
- Rust updater service 根据 Windows 编译架构选择 portable 更新目标；下载页说明、安装包命令和发行验证文档同步更新。

## 验证

- Node manifest tests、Rust Tauri tests（720 项）与 Clippy 均通过；lint、typecheck、Prettier、Rustfmt 和 shell 语法检查均通过。
- Arch package builder 会在 GitHub Actions 中校验 x86_64 与 aarch64 `.PKGINFO`、运行时依赖和安装文件清单；本地不运行发行包构建。Linux ARM64 及四种 Linux 包格式的实际构建与元数据检查留给 GitHub Release Action 执行，尚未运行该 Action。
