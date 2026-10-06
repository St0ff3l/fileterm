# 本地自动更新测试

> 适用对象：Windows 签名 NSIS 应用内更新器（Tauri `tauri_plugin_updater`）。macOS / Linux 当前无应用内更新器，不在此流程内。

Tauri 的更新器端点由 `services/updates.rs` 中的 `RELEASE_DOWNLOAD_BASE`（`https://github.com/St0ff3l/fileterm/releases/download`）按 release tag 推导，运行时会拉取 `<tag>/latest.json` 并校验 `<tag>/<installer>.exe.sig`。本地测试需要一个能在相同路径布局下提供 `latest.json`、安装器与签名的本地 HTTP 服务，并把 `RELEASE_DOWNLOAD_BASE` 临时指向该服务（该常量为 Rust 代码，本地测试需临时改为指向 `http://127.0.0.1:<port>` 后重新构建，测试完毕还原）。

## Windows 测试

先分别构建两个版本（构建前用根 `package.json` 的 `version` 切换版本号并运行 `npm run sync:version`）：

```bash
# 版本 1.0.0
npm run release:win -w @fileterm/tauri
# 版本 1.0.1
npm run release:win -w @fileterm/tauri
```

`release:win` 与 `release:win:arm64` 基于 `tauri.release.windows.conf.json`（`targets: ["nsis"]`，`createUpdaterArtifacts: true`），分别在 x64 与 ARM64 target 目录下生成 `-setup.exe`、`-setup.exe.sig`。发布流水线将两种架构的签名资产合并到同一个 `latest.json`：

```bash
export GITHUB_REPOSITORY=St0ff3l/fileterm
export GITHUB_REF_NAME=v1.0.1
node ./apps/tauri/scripts/create-windows-updater-manifest.mjs \
  apps/tauri/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis
```

ARM64 本机构建使用 `npm run release:win:arm64 -w @fileterm/tauri`；需要 ARM64 Windows 编译工具链。合并两个架构产物时，向脚本传入 `--merge` 和各自的 bundle 目录，它会在当前目录生成包含四个平台键的清单：

```bash
node ./apps/tauri/scripts/create-windows-updater-manifest.mjs --merge \
  FileTerm-Windows-x64 FileTerm-Windows-arm64
```

将该 `nsis` 目录按 `<tag>/` 路径前缀布局后，用任意静态服务器在本地暴露（例如 `python3 -m http.server 8765`），并把 `updates.rs` 的 `RELEASE_DOWNLOAD_BASE` 临时改为 `http://127.0.0.1:8765`。

安装 `1.0.0` 的 NSIS 安装包，启动后在设置 → 应用更新中点击检查更新。预期流程：发现 `1.0.1` → 校验签名 → 下载 → 重启并更新。

测试完成后关闭 HTTP 服务即可；`target/.../bundle/nsis` 产物可用 `npm run clean:release -w @fileterm/tauri` 清理，务必还原 `updates.rs` 中的 `RELEASE_DOWNLOAD_BASE`。

## 注意事项

- 必须测试已安装的 NSIS 版本，不能用 `npm run dev` 或 portable 包验证覆盖安装。
- 本地产物中的 `latest.json` 与安装器必须位于更新器期望的 `<tag>/` 路径下，且签名文件与安装器同名成对出现。
- 未配置 `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 时不会生成 `.sig`，更新器会因签名校验失败拒绝更新。

## Windows 便携版更新验收

便携版使用同一 `latest.json` 中独立的 `windows-x86_64-portable` 或 `windows-aarch64-portable` 条目，下载对应架构的便携 EXE 并用 updater 公钥验签。不得把 NSIS 安装器填入便携条目。

1. 构建两个支持便携更新的版本，分别使用 `FILETERM_PORTABLE_BUILD=1 npm run release:win:portable -w @fileterm/tauri` 和 `FILETERM_PORTABLE_BUILD=1 npm run release:win:portable:arm64 -w @fileterm/tauri`（PowerShell 先设置 `$env:FILETERM_PORTABLE_BUILD='1'`）。将便携 EXE 按 `*-windows-x64-portable.exe` / `*-windows-arm64-portable.exe` 命名并执行 `npx tauri signer sign <完整路径>`；与两种架构的 NSIS 签名产物一起合并清单。首次从旧版升级仍需手动覆盖。
2. 在含中文、空格的可写目录启动旧版，把 EXE 重命名为 `我的 FileTerm.exe`，保存连接配置。在更新页下载并重启更新，确认新版仍从原目录、原文件名启动，`config` 内容和 `portable` 标记保留，成功事务子目录被清理。
3. 分别测试稳定/测试通道、下载断网、错误签名、旧 Release 缺少便携条目；验签失败必须拒绝安装，缺少条目回退下载页。
4. 测试目录不可写、helper 启动被拦截；应用不得先退出。测试原 EXE 被另一进程占用、新 EXE 启动失败：保持或恢复旧程序，保留错误记录；回滚失败时从 `.fileterm-update-<uuid>/previous.exe` 手动恢复。
5. 更新会退出应用并中断当前远程会话；确认现有“重启更新”交互仍由用户主动触发。
6. 在 x64 和 ARM64 Windows 上分别回归 NSIS 安装版与便携版，确认各自读取相同架构的清单条目，未误用另一架构或便携 EXE。

签名指 updater 内容签名，不等同于 Windows Authenticode，也不保证消除 SmartScreen 提示。Windows 专用等待/文件占用测试由三平台 Rust CI 的 Windows job 执行；本机 macOS 测试不能替代实际 Windows 更新验收。
