# 本地自动更新测试

> 适用对象：Windows NSIS/便携版、macOS 与 Linux AppImage/DEB/RPM 应用内更新器（Tauri `tauri_plugin_updater`）。下方 Windows 流程使用安装器，macOS 实测见文末；Linux 各发行版安装/授权步骤见[发布验收清单](./auto-update-release-checklist.md#linux-tauri-应用内更新)。

Tauri 的更新器端点由 `services/updates.rs` 中的 `RELEASE_DOWNLOAD_BASE`（`https://github.com/St0ff3l/fileterm/releases/download`）按 release tag 推导，运行时会拉取 `<tag>/latest.json` 并用清单内的签名验证下载内容（发布时另提供对应 `.sig` 文件）。本地测试需要一个能在相同路径布局下提供 `latest.json`、安装器与签名的本地 HTTP 服务，并把 `RELEASE_DOWNLOAD_BASE` 临时指向该服务（该常量为 Rust 代码，本地测试需临时改为指向 `http://127.0.0.1:<port>` 后重新构建，测试完毕还原）。

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
node ./apps/tauri/scripts/create-updater-manifest.mjs \
  apps/tauri/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis
```

ARM64 本机构建使用 `npm run release:win:arm64 -w @fileterm/tauri`；需要 ARM64 Windows 编译工具链。合并两个架构产物时，向脚本传入 `--merge` 和各自的 bundle 目录，它会在当前目录生成包含四个平台键的清单：

```bash
node ./apps/tauri/scripts/create-updater-manifest.mjs --merge \
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

## macOS 更新清单与回滚检查

`node --test apps/tauri/tests/build/updater-manifest.test.mjs` 覆盖 Windows、macOS 与 Linux：合并清单包含十二个目标，Linux 按架构和 AppImage/DEB/RPM 格式区分；缺失/空签名、重复目标及 Linux 格式缺失、错误版本/架构必须拒绝生成。

正式发布流水线继续保留 DMG，另发布 `FileTerm-<version>-macos-<arm64|x64>.app.tar.gz` 及匹配 `.sig`。macOS 配置与 Windows 使用同一更新公钥，签名私钥只从 CI secrets 注入；签名不提供 Apple 公证。

在 macOS 上执行 `cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml services::updates::macos` 验证安装失败后恢复旧应用、安装成功清理备份及不支持路径的拒绝行为；测试只修改临时目录中的模拟 `.app`。

真实打包升级按 [应用内更新验收清单](./auto-update-release-checklist.md#macostauri-签名更新) 执行，必须使用已安装的旧版本，不用开发态代替验证。首次启用需手动安装新版；后续升级如遇系统拦截，按“隐私与安全性 → 仍要打开”放行。没有 Apple 签名/公证时不保证跨系统版本免批准。

### 2026-10-24 Apple Silicon 打包实测

环境：macOS 27.0.1 / arm64；使用 release 构建和 ad-hoc 应用签名，测试版本为 `2.3.1-beta.14 → 2.3.1-beta.15`。这些版本仅通过临时构建配置覆盖，没有修改仓库版本或发布 Release。

隔离方式：测试应用使用独立 bundle ID `com.fileterm.desktop.updater-test-20261024`、独立数据目录和 `~/Applications` 下的独立安装目录。预置完整测试连接和暂停传输记录，写入已完成的迁移标记，防止读取用户旧版数据。使用临时 updater 密钥和只监听 `127.0.0.1` 的更新服务；仅测试构建临时替换 `RELEASES_API`、`RELEASE_DOWNLOAD_BASE` 和配置中的公钥/端点，并允许本地 HTTP。构建结束即恢复源文件，测试完成后关闭服务并移除私钥；正式配置仍为 HTTPS 和既有生产公钥。

实测结果：

- 两个版本均生成真实 `.app`、`.dmg`、`.app.tar.gz` 和 `.sig`，均通过 `codesign --verify --deep --strict`。
- 从旧版 DMG 安装并启动，界面显示 `v2.3.1-beta.14`；发现 `beta.15` 后，篡改更新包被拒绝，界面显示 `The signature verification failed`，旧应用二进制 SHA-256 不变。
- 换回正确签名更新后，完成下载并显示“重启并更新”；点击后旧进程退出，新进程自动从同一安装路径启动，界面显示 `v2.3.1-beta.15`，再次检查显示已是最新版本。验证新进程已运行后才重新绑定 UI 工具，未用手动启动代替自动重启。
- 安装后二进制 SHA-256 与新版打包产物完全一致，应用签名仍有效；连接 ID/名称、暂停传输 ID/状态/42% 进度与可恢复标记保留；成功更新后没有遗留 `.fileterm-update-backup-*`。
- 本次从本机构建的 DMG 安装，升级前后应用均无 quarantine 属性，更新后无需额外批准。没有清除 quarantine、关闭 Gatekeeper 或修改全局安全设置；该结果不覆盖浏览器下载带隔离属性的首次安装，也不保证其他 macOS 版本不再要求批准。
- 测试版、测试服务已退出，原 `/Applications/FileTerm.app` 进程保持运行；测试安装和数据移至临时取证目录，未替换用户正式应用或改动其数据。

未覆盖：Intel 实机升级、真实 GitHub Release/生产签名密钥下载、系统手动放行、活动远程传输中断和真实安装失败。安装失败/部分安装/安装器 panic 的旧版恢复由 Rust 测试覆盖。

## 2026-10-25 Linux 代码与容器验证

- Debian 12 ARM64 容器：`cargo test --locked services::updates -- --test-threads=1` 编译 Linux 原生分支，复审加固后 20 项测试通过；`cargo clippy --locked --all-targets --all-features -- -D warnings` 通过。
- 覆盖六种格式/架构目标选择、系统包归属解析/超时及未知二进制回退；AppImage 先暂存并校验后原子替换，保留路径、执行权限和配置。通过保持旧文件句柄验证原 inode 未被截断；通过真实终止暂存中的单元测试子进程验证原文件完整且备份保留。另覆盖格式/架构错误、部分写入、内容不一致、panic、原文件并发变化、恢复受阻和重启路径无效。
- 清单测试覆盖十二个平台目标，拒绝缺失、空签名、空文件、非普通文件、重复及版本/架构错误的 Linux 资产。
- 以上是代码与故障夹具验证，未运行图形桌面安装/授权，也未完成已安装旧版到新版的 AppImage/DEB/RPM 升级；按[Linux 发布验收清单](./auto-update-release-checklist.md#linux-tauri-应用内更新)单独记录实测。版本与已发布资产未修改。
