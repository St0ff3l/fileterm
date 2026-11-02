# 应用内更新发布与验收清单

FileTerm 的桌面运行时是 Rust + Tauri（唯一维护、构建和发布的运行时，历史桌面实现已彻底移除）。更新机制按平台不同：

- **Windows**：x64 与 ARM64 Tauri 签名 NSIS 安装器 + 内置应用内更新（`tauri_plugin_updater`）。`services/updates.rs` 从 GitHub Release 的下载路径拉取 `latest.json`，按架构选取安装器或 portable EXE，校验 `.sig` 后再下载并替换。
- **macOS**：保留 ad-hoc 应用签名，使用 Tauri updater 校验独立签名的 `.app.tar.gz`，用户确认后备份旧应用、安装并重启。不购买 Apple Developer ID、不执行公证；被系统拦截时需用户手动放行。旧 Release 无对应更新条目时回退 GitHub 下载页。
- **Linux**：x86_64 与 ARM64 的 `.deb` / `.rpm` / `.AppImage` 使用 Tauri 签名更新，按安装格式选择独立目标；DEB/RPM 必须确认当前程序归属 `file-term` 系统包，AppImage 必须在 AppImage 环境中运行。AppImage 原地备份/替换，DEB/RPM 使用插件的系统提权安装；成功后保存传输断点并重启。Arch/pacman、开发态、解包运行或无法确认包归属时继续打开 Release 页。各架构 AppImage 仍提供匹配 `.AppImage.zsync`，供外部工具执行差量更新；应用内 updater 下载完整签名包。

## 首次启用前

1. 在 GitHub repository 的 secrets 中配置 `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，用于 Windows、macOS 和 Linux 更新包签名。缺少该密钥时 release workflow 的对应签名步骤会直接失败。三个平台配置中的公钥必须匹配该私钥；不重新生成或覆盖现有生产密钥。
2. macOS 使用 ad-hoc 签名（`signingIdentity: "-"`），无需 Apple 证书；更新包签名与 Apple 签名、公证相互独立。未接入更新器的旧版必须先手动安装一次新版 DMG。
3. 确认 release workflow（`.github/workflows/release.yml`）在 tag push 时产出以下产物：
   - Windows x64/ARM64：`*-setup.exe`、`*-setup.exe.sig`、portable EXE 与签名，以及含两种架构条目的 `latest.json`
   - macOS：Apple Silicon（arm64）与 Intel（x64）各自的 `.dmg`、`.app.tar.gz` 与 `.app.tar.gz.sig`；同一 `latest.json` 包含 `darwin-aarch64` 与 `darwin-x86_64`
   - Linux x86_64/ARM64：`.deb`、`.rpm`、`.AppImage` 及各自的 `.sig`，另有 `.AppImage.zsync`；统一 `latest.json` 包含六个 `linux-<x86_64|aarch64>-<appimage|deb|rpm>` 目标，不提供跨包格式的 `linux-<arch>` fallback
   - Arch x86_64、Arch Linux ARM aarch64：各一个 `.pkg.tar.zst`

## 发布步骤

1. 仅修改根目录 `package.json` 的 `version` 字段，随后运行 `npm run sync:version`（严禁手改 workspace 内部版本）。
2. 按仓库 release SOP 从 `main` 创建 `release/x.y.z` 分支并推送。
3. 在 `release/x.y.z` 分支的最新提交上打 `vx.y.z` tag 并推送，等待 `release.yml` 完成构建与 GitHub Release 创建。
4. 打开 GitHub Release，确认 Windows（x64/ARM64 exe / sig）、macOS（arm64 + x64 dmg / app.tar.gz / sig）、Linux（x86_64/arm64 deb / rpm / AppImage / sig / zsync）及含十二个目标的统一 `latest.json` 均已作为资产附加。Linux 签名必须在 universal DEB 和 AppImage 重打包完成后生成。
5. 确认 Arch x86_64 与 Arch Linux ARM aarch64 两个 `.pkg.tar.zst` 都已发布；包内 `.PKGINFO` 的架构、运行依赖、桌面入口、AppStream 元数据、图标与 MIT 许可证均通过构建校验。
6. 确认 Linux AppImage 的 AppDir 根目录、内部目录对其他用户可读和遍历，所有可执行文件及 `AppRun`、`AppRun.wrapped` 对其他用户可执行。Release 工作流会在上传前校正权限，并直接检查内嵌 SquashFS 的权限元数据。`scripts/appimage-host-libraries.py` 在重打包前移除宿主机 display-stack 库（Wayland、xkbcommon 及指定 X11/XCB 库），最终产物校验再次拒绝这些库，避免旧 Ubuntu 库覆盖新版 Mesa 所需的宿主库；GTK/WebKitGTK 等应用运行库继续打包。AppImage 资产统一命名为 `FileTerm-<version>-linux-x86_64.AppImage` 或 `FileTerm-<version>-linux-arm64.AppImage`。AppImageHub 会对 `linux` 字样给出命名建议警告，该警告不影响运行，也不作为本仓库的发布失败条件。

## 升级验收

必须使用已安装的旧版本测试，不能只运行开发态或直接打开新安装包。

### Windows（x64 / ARM64 NSIS 应用内更新）

1. 分别在 x64 与 ARM64 Windows 上安装对应架构的旧版 NSIS 安装包，确认应用位于正常安装目录。
2. 启动旧版本，在设置中检查更新（或等待自动检查）。
3. 确认更新器拉取到 `latest.json`、校验签名、下载并提示重启。
4. 点击“重启并更新”，确认应用退出、NSIS 覆盖旧文件并自动重新打开新版本。
5. 确认连接配置、传输记录仍保留。

### macOS（Tauri 签名更新）

1. 在 arm64 与 x64 Mac 上分别将包含更新器的旧版安装到当前用户可写的 `/Applications` 或 `~/Applications`，首次打开按系统提示放行；不要从 DMG 或 App Translocation 路径运行。
2. 检查更新，确认按所选 Release tag 读取 `latest.json`，匹配运行架构的 `darwin-aarch64` / `darwin-x86_64`；旧 Release 无对应条目时打开其下载页。
3. 下载并验签完成后，确认显示“重启并更新”和手动放行提示。篡改包或签名时必须报错，不进入安装。
4. 打开有未保存修改的文件编辑器，点击更新后取消保存/退出，确认不安装且仍可再次点击；确认保存后继续。
5. 安装前会在应用同目录创建 `.fileterm-update-backup-*`，使用系统 `ditto` 保留旧应用；安装失败恢复旧版，若恢复也失败则保留备份并显示恢复路径。不可写目录不触发提权覆盖，需手动下载 DMG 更新。
6. 安装成功后保存传输断点、停止会话并重新启动，确认版本、连接配置和传输记录保留。若保存断点失败则显示新版已安装，保存工作后手动退出重开。
7. 新版若被系统拦截，在“系统设置 → 隐私与安全性 → 仍要打开”批准后重新启动。不保证所有 macOS 版本只批准一次；不关闭 Gatekeeper 或全局修改安全设置。
8. 在 DMG 挂载点、App Translocation、不可写目录验证拒绝安装；安装中断、空间不足及恢复失败场景需确认旧应用副本仍可恢复。重启未成功时可手动打开安装目录中的 FileTerm。

### Linux Tauri 应用内更新

1. 在 x86_64/ARM64 对应发行版安装包含 updater 的旧版 AppImage、DEB 和 RPM，确认只匹配当前架构及格式的清单条目。无该格式条目的旧 Release 应回退下载页，不能下载其他格式。
2. AppImage 放在用户可写目录并直接运行，允许用户重命名。安装前确认编辑器保存/取消；更新成功后确认原路径、执行权限和配置保留、版本更新、传输断点保留。只读目录、符号链接及解包运行不应覆盖原程序。
3. AppImage 下载仍由 Tauri 验签，安装前还检查 ELF、Type 2 标记及架构。安装在同目录 `.fileterm-update-backup-*` 中备份和暂存，回读校验 SHA-256、保留执行权限并刷盘后原子替换；不再调用上游直接重写原文件的 AppImage 安装器。暂存失败、panic 或进程被强制终止时原文件保持完整；原文件并发变化时拒绝覆盖并显示备份路径。提交后失败尝试恢复，恢复失败保留备份。实际磁盘满、设备断电和网络文件系统仍需验收。格式标记依据 [AppImage Type 2 规范](https://github.com/AppImage/AppImageSpec/blob/master/draft.md#type-2-image-format)。
4. DEB/RPM 安装由 Tauri 插件先尝试 `pkexec dpkg -i` / `pkexec rpm -U`，可能回退到系统其他提权方式。验证授权成功后安装并重启；取消/拒绝授权、缺少授权工具、依赖冲突或包管理器锁应显示错误、保持应用运行，不自动重启。插件使用包管理器安装，不负责自动获取新增依赖。
5. 安装前后检查重启文件存在且可执行；已安装但重启路径无效时保持当前应用运行，提示手动打开。检查重启后 DEB 的 universal 启动路径（Debian 11 私有运行时与现代发行版系统运行时）、包版本/校验，以及 RPM 包版本/校验。确认篡改 payload/signature 在安装前拒绝；下载失败、签名失败不能调用包管理器。
6. Arch/pacman 包虽然来自 DEB 重打包，也必须继续显示下载页更新，不能调用 dpkg；开发态和未被系统包拥有的裸二进制同样回退。系统包归属查询超过两秒会终止并保守回退下载页。AppImage 在 Arch 上仍按 AppImage 更新。
7. 首次启用需手动安装一次包含 Linux updater 的版本；此次未替换已发布资产。实际发布和跨发行版图形授权流程需按对应版本单独记录结果。

用户实测记录（2026-10-25）：Arch x86_64 的 beta.1 AppImage 在临时设置 `LD_PRELOAD=/usr/lib/libwayland-client.so.0` 后可启动，beta.1 → beta.2 应用内更新及重启成功。此结果来自用户反馈；修复后无需该变量的 AppImage 仍需在相同机器验证。尚不能据此判定 ARM64、DEB/RPM 提权或其他 GPU/发行版通过。

### Linux（AppImageUpdate 外部更新）

- `scripts/ensure-appimage-executable.sh` 移除宿主机图形库并修正权限后，通过固定版本的 `appimagetool -u` 重打包并生成 `.zsync`。最终文件名必须在重打包前确定，禁止生成校验文件后再修改 AppImage 内容；Tauri updater 签名在全部处理之后生成。
- 正式版对每种架构分别使用 `gh-releases-zsync|St0ff3l|fileterm|latest|FileTerm-*-linux-<arch>.AppImage.zsync`；预发布版使用 `latest-pre`，避免正式版升级到测试版。
- 上传前验证内嵌更新信息，并检查 `.zsync` 的 Filename、URL、Length、SHA-1 与最终 AppImage 一致；两者必须上传至同一个 Release。
- 首次发布后，用 `--appimage-updateinformation` 检查发布资产，并使用 AppImageUpdate 检查更新；下一次正式发布后，再从旧的可更新 AppImage 实测差量升级。2.2.18 的已发布资产没有内嵌更新信息，无法自动获得此能力。
- 规范参考：[AppImage 更新指南](https://docs.appimage.org/packaging-guide/optional/updates.html)。
