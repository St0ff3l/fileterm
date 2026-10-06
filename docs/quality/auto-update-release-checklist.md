# 应用内更新发布与验收清单

FileTerm 的桌面运行时是 Rust + Tauri（唯一维护、构建和发布的运行时，历史桌面实现已彻底移除）。更新机制按平台不同：

- **Windows**：x64 与 ARM64 Tauri 签名 NSIS 安装器 + 内置应用内更新（`tauri_plugin_updater`）。`services/updates.rs` 从 GitHub Release 的下载路径拉取 `latest.json`，按架构选取安装器或 portable EXE，校验 `.sig` 后再下载并替换。
- **macOS**：当前发布配置（`tauri.release.macos.conf.json`）未配置应用内更新签名，更新入口回退为打开 GitHub Release 下载页，由用户手动下载新版 DMG。
- **Linux**：x86_64 与 ARM64 通过 GitHub Release 提供 `.deb` / `.rpm` / `.AppImage`；另提供 Arch x86_64 与 Arch Linux ARM aarch64 `.pkg.tar.zst`，无应用内更新器。各架构 AppImage 内嵌 GitHub Release 更新信息，同时提供匹配的 `.AppImage.zsync`，供 AppImageUpdate 等外部工具执行差量更新。Linux 安装包均带桌面入口、图标、AppStream 元数据和 MIT 许可证；包管理器格式声明运行时依赖。

## 首次启用前

1. 在 GitHub repository 的 secrets 中配置 `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，用于 Windows NSIS 更新器签名。缺少该密钥时 release workflow 的 Windows 打包会直接失败。
2. macOS 当前为未签名发布（`signingIdentity: "-"`），无需 Apple 证书即可构建；若后续要启用 macOS 应用内更新或 notarization，再补充对应签名密钥。
3. 确认 release workflow（`.github/workflows/release.yml`）在 tag push 时产出以下产物：
   - Windows x64/ARM64：`*-setup.exe`、`*-setup.exe.sig`、portable EXE 与签名，以及含两种架构条目的 `latest.json`
   - macOS：Apple Silicon（arm64）与 Intel（x64）各自的 `.dmg`
   - Linux x86_64/ARM64：`.deb`、`.rpm`、`.AppImage` 与 `.AppImage.zsync`
   - Arch x86_64、Arch Linux ARM aarch64：各一个 `.pkg.tar.zst`

## 发布步骤

1. 仅修改根目录 `package.json` 的 `version` 字段，随后运行 `npm run sync:version`（严禁手改 workspace 内部版本）。
2. 按仓库 release SOP 从 `main` 创建 `release/x.y.z` 分支并推送。
3. 在 `release/x.y.z` 分支的最新提交上打 `vx.y.z` tag 并推送，等待 `release.yml` 完成构建与 GitHub Release 创建。
4. 打开 GitHub Release，确认 Windows（x64/ARM64 exe / sig / combined `latest.json`）、macOS（arm64 + x64 dmg）、Linux（x86_64/arm64 deb / rpm / AppImage / zsync）均已作为资产附加。
5. 确认 Arch x86_64 与 Arch Linux ARM aarch64 两个 `.pkg.tar.zst` 都已发布；包内 `.PKGINFO` 的架构、运行依赖、桌面入口、AppStream 元数据、图标与 MIT 许可证均通过构建校验。
6. 确认 Linux AppImage 的 AppDir 根目录、内部目录对其他用户可读和遍历，所有可执行文件及 `AppRun`、`AppRun.wrapped` 对其他用户可执行。Release 工作流会在上传前校正权限，并直接检查内嵌 SquashFS 的权限元数据。AppImage 资产统一命名为 `FileTerm-<version>-linux-x86_64.AppImage` 或 `FileTerm-<version>-linux-arm64.AppImage`。AppImageHub 会对 `linux` 字样给出命名建议警告，该警告不影响运行，也不作为本仓库的发布失败条件。

## 升级验收

必须使用已安装的旧版本测试，不能只运行开发态或直接打开新安装包。

### Windows（x64 / ARM64 NSIS 应用内更新）

1. 分别在 x64 与 ARM64 Windows 上安装对应架构的旧版 NSIS 安装包，确认应用位于正常安装目录。
2. 启动旧版本，在设置中检查更新（或等待自动检查）。
3. 确认更新器拉取到 `latest.json`、校验签名、下载并提示重启。
4. 点击“重启并更新”，确认应用退出、NSIS 覆盖旧文件并自动重新打开新版本。
5. 确认连接配置、传输记录仍保留。

### macOS（GitHub Release 下载）

1. 将旧版本应用拖入 `/Applications`，不要从 DMG 挂载点直接运行。
2. 在更新入口检查，预期行为为打开 GitHub Release 下载页（应用内更新当前未启用）。
3. 手动下载匹配架构的新版 DMG，拖入 `/Applications` 覆盖，确认连接配置仍保留。
4. 若签名/公证缺失导致系统拦截，按 macOS 安全提示在“系统设置 → 隐私与安全性”中允许打开。

### Linux（AppImageUpdate 外部更新）

- `scripts/ensure-appimage-executable.sh` 在修正权限后，通过固定版本的 `appimagetool -u` 重打包并生成 `.zsync`。最终文件名必须在重打包前确定，禁止生成校验文件后再修改 AppImage 内容。
- 正式版对每种架构分别使用 `gh-releases-zsync|St0ff3l|fileterm|latest|FileTerm-*-linux-<arch>.AppImage.zsync`；预发布版使用 `latest-pre`，避免正式版升级到测试版。
- 上传前验证内嵌更新信息，并检查 `.zsync` 的 Filename、URL、Length、SHA-1 与最终 AppImage 一致；两者必须上传至同一个 Release。
- 首次发布后，用 `--appimage-updateinformation` 检查发布资产，并使用 AppImageUpdate 检查更新；下一次正式发布后，再从旧的可更新 AppImage 实测差量升级。2.2.18 的已发布资产没有内嵌更新信息，无法自动获得此能力。
- 规范参考：[AppImage 更新指南](https://docs.appimage.org/packaging-guide/optional/updates.html)。
