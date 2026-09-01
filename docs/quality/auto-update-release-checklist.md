# 应用内更新发布与验收清单

FileTerm 的桌面运行时是 Rust + Tauri（唯一维护、构建和发布的运行时，历史桌面实现已彻底移除）。更新机制按平台不同：

- **Windows**：Tauri 签名 NSIS 安装器 + 内置应用内更新（`tauri_plugin_updater`）。`services/updates.rs` 从 GitHub Release 的下载路径拉取 `latest.json`，校验 `.sig` 后再下载并替换安装。
- **macOS**：当前发布配置（`tauri.release.macos.conf.json`）未配置应用内更新签名，更新入口回退为打开 GitHub Release 下载页，由用户手动下载新版 DMG。
- **Linux**：通过 GitHub Release 提供 `.deb` / `.AppImage`，无应用内更新器。

## 首次启用前

1. 在 GitHub repository 的 secrets 中配置 `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，用于 Windows NSIS 更新器签名。缺少该密钥时 release workflow 的 Windows 打包会直接失败。
2. macOS 当前为未签名发布（`signingIdentity: "-"`），无需 Apple 证书即可构建；若后续要启用 macOS 应用内更新或 notarization，再补充对应签名密钥。
3. 确认 release workflow（`.github/workflows/release.yml`）在 tag push 时产出以下产物：
   - Windows：`*-setup.exe`、`*-setup.exe.sig`、`latest.json`
   - macOS：Apple Silicon（arm64）与 Intel（x64）各自的 `.dmg`
   - Linux：`.deb` 与 `.AppImage`

## 发布步骤

1. 仅修改根目录 `package.json` 的 `version` 字段，随后运行 `npm run sync:version`（严禁手改 workspace 内部版本）。
2. 按仓库 release SOP 从 `main` 创建 `release/x.y.z` 分支并推送。
3. 在 `release/x.y.z` 分支的最新提交上打 `vx.y.z` tag 并推送，等待 `release.yml` 完成构建与 GitHub Release 创建。
4. 打开 GitHub Release，确认 Windows（exe / sig / latest.json）、macOS（arm64 + x64 dmg）、Linux（deb / AppImage）均已作为资产附加。

## 升级验收

必须使用已安装的旧版本测试，不能只运行开发态或直接打开新安装包。

### Windows（NSIS 应用内更新）

1. 安装旧版本 NSIS 安装包，确认应用位于正常安装目录。
2. 启动旧版本，在设置中检查更新（或等待自动检查）。
3. 确认更新器拉取到 `latest.json`、校验签名、下载并提示重启。
4. 点击“重启并更新”，确认应用退出、NSIS 覆盖旧文件并自动重新打开新版本。
5. 确认连接配置、传输记录仍保留。

### macOS（GitHub Release 下载）

1. 将旧版本应用拖入 `/Applications`，不要从 DMG 挂载点直接运行。
2. 在更新入口检查，预期行为为打开 GitHub Release 下载页（应用内更新当前未启用）。
3. 手动下载匹配架构的新版 DMG，拖入 `/Applications` 覆盖，确认连接配置仍保留。
4. 若签名/公证缺失导致系统拦截，按 macOS 安全提示在“系统设置 → 隐私与安全性”中允许打开。
