# 应用内更新发布与验收清单

FileTerm 使用 `electron-updater` + GitHub Release 更新已安装的 Windows NSIS 和 macOS 应用。客户端会在启动时检查更新；发现版本后显示下载进度，下载完成后由用户选择“重启并更新”。

## 首次启用前

1. GitHub repository 设置中添加 `CSC_LINK`、`CSC_KEY_PASSWORD` secrets，提供 Apple Developer ID Application 签名证书。macOS 未签名应用不能使用自动更新。
2. Windows 建议配置 Authenticode 代码签名，避免 SmartScreen 警告；应用内 NSIS 更新本身不依赖该证书，但面向用户发布应签名。
3. 确认 release workflow 会上传以下文件：
   - Windows：NSIS `.exe` 与 `latest.yml`
   - macOS：Apple Silicon（arm64）和 Intel（x64）各自的 `.dmg`、`.zip`，以及包含两种架构文件的 `latest-mac.yml`

## 发布步骤

1. 仅修改根目录 `package.json` 的版本号并运行 `npm run sync:version`。
2. 按仓库 release SOP 从 `main` 创建 `release/x.y.z`，推送该分支。
3. 在 release 分支提交上打 `vx.y.z` tag 并推送，等待 GitHub Release workflow 完成。
4. 打开 GitHub Release，确认 macOS arm64/x64、Windows x64 安装包和两份 `.yml` 元数据都已附件发布。

## 升级验收

使用较低版本的已安装应用测试，不能只运行开发态或直接打开新安装包。

### Windows（NSIS）

1. 安装旧版本 NSIS 安装包，确保应用位于正常安装目录。
2. 启动旧版本，等待或在设置中手动检查更新。
3. 在更新提示条点击“下载更新”，确认进度到 100%。
4. 点击“重启并更新”，确认应用退出、安装器覆盖旧文件并自动重新打开新版本。
5. 确认连接配置、传输记录仍保留。

### macOS（DMG / ZIP）

1. 将旧版本应用拖入 `/Applications`，不要从 DMG 或 App Translocation 位置直接运行。
2. 启动旧版本并检查更新，下载后点击“重启并更新”。
3. 确认新版本重新打开，应用签名有效，连接配置仍保留。
4. 若失败，先检查 GitHub Release 是否包含 `latest-mac.yml`、与当前 Mac 架构匹配的 ZIP，以及签名证书是否配置正确。
