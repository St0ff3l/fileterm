## FileTerm 2.3.2

FileTerm 2.3.2 加入服务器间文件传输，并整合 Linux 应用内更新与桌面工作区体验改进。AppImage 需要放在当前用户可写的位置；DEB/RPM 更新可能需要管理员授权；Arch 和其他不受支持的安装仍通过发布页更新。

**下载地址**

| 平台    | 类型      | 下载                                                                                                                                                                                                                                                   |
| ------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| macOS   | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-macos-x64.dmg)                                      |
| Windows | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-windows-arm64-setup.exe)                                |
| Windows | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-windows-arm64-portable.exe)（需要 WebView2 Runtime） |
| Linux   | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-arm64.deb)                                          |
| Linux   | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-arm64.rpm)                                          |
| Linux   | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-arm64.AppImage)                                |
| Linux   | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-arm64.pkg.tar.zst)                          |

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.3.2 更新重点

- **Linux 应用内更新**：对受支持的 AppImage、DEB 和 RPM 安装使用 Tauri updater，并验证 Ed25519 更新签名；AppImage 通过备份和原子替换安装，DEB/RPM 由系统安装器处理，可能要求管理员授权。
- **安装格式边界**：AppImage 必须位于当前用户可写的位置；DEB/RPM 仅对由系统包管理器安装的 FileTerm 启用；Arch 包和不支持的架构继续打开 GitHub Releases。
- **Linux 侧栏启动宽度**：窗口尺寸稳定后恢复 214px 默认宽度；窗口启动期间较窄的临时视口不会永久改变宽度，手动调整也会保留。
- **服务器间文件传输**：可在两个远程服务器之间复制文件，传输经本机中转；支持同名冲突处理，并在任务区查看进度。
- **传输安全与稳定性**：加强目标路径和端点校验；覆盖同名替换、符号链接及失败回滚，减少覆盖到意外文件的风险。
- **工作区界面**：统一表单输入焦点反馈、弹窗密度和按钮加载状态，修复管理器搜索框阴影被裁切的问题，并在侧栏提供文件传输快捷入口；保留现有悬停反馈。
- **侧栏与标签切换**：减少收起、展开侧栏和文件区域时重复的布局测量，优化滚动条尺寸更新及切换标签后的工作区同步。

### 本版本包含的主要 PR 和问题修复

- 本版本包含 Linux 侧栏启动宽度修正；Linux 应用内更新实现见 [PR #362](https://github.com/St0ff3l/fileterm/pull/362)。
- [PR #365](https://github.com/St0ff3l/fileterm/pull/365)：修复 Linux 窗口启动后侧栏宽度变窄的问题。
- [PR #366](https://github.com/St0ff3l/fileterm/pull/366)：加入服务器间文件传输并改进工作区界面；相关需求见 [Issue #354](https://github.com/St0ff3l/fileterm/issues/354)。

完整变更记录请查看 [v2.3.1 与 v2.3.2 的比较](https://github.com/St0ff3l/fileterm/compare/v2.3.1...v2.3.2)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.3.2

FileTerm 2.3.2 adds remote-to-remote file transfers and brings Linux in-app updates and workspace improvements together. AppImages must be in a location writable by the current user; DEB/RPM updates may require administrator authorization. Arch and other unsupported installations continue to use the Releases page.

**Downloads**

| Platform | Type      | Downloads                                                                                                                                                                                                                                                 |
| -------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS    | DMG       | [Apple silicon](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-macos-arm64.dmg) · [Intel](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-macos-x64.dmg)                                         |
| Windows  | Installer | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-windows-x64-setup.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-windows-arm64-setup.exe)                                   |
| Windows  | Portable  | [x64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-windows-x64-portable.exe) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-windows-arm64-portable.exe) (requires WebView2 Runtime) |
| Linux    | DEB       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-x86_64.deb) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-arm64.deb)                                             |
| Linux    | RPM       | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-x86_64.rpm) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-arm64.rpm)                                             |
| Linux    | AppImage  | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-x86_64.AppImage) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-arm64.AppImage)                                   |
| Linux    | Arch      | [x86_64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-x86_64.pkg.tar.zst) · [ARM64](https://github.com/St0ff3l/fileterm/releases/download/v2.3.2/FileTerm-2.3.2-linux-arm64.pkg.tar.zst)                             |

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### 2.3.2 Highlights

- **Linux in-app updates**: Use the Tauri updater with Ed25519 signature verification for supported AppImage, DEB, and RPM installations. AppImages are installed with a verified backup and atomic replacement; DEB/RPM updates use the system installer and may require administrator authorization.
- **Installation boundaries**: AppImages must be in a location writable by the current user. DEB/RPM updates are enabled only for FileTerm installations owned by the system package manager. Arch packages and unsupported architectures continue to open GitHub Releases.
- **Linux sidebar startup width**: Restore the 214px default when the window reaches its final size. A temporarily narrow startup viewport no longer permanently changes the width, and manual resizing is preserved.
- **Remote-to-remote transfers**: Copy files between two remote servers through the local app, choose how to handle name conflicts, and follow progress in the transfer task list.
- **Transfer safety and reliability**: Strengthen destination path and endpoint validation, with coverage for replacement, symbolic links, and rollback after failures.
- **Workspace UI**: Unify form focus feedback, dialog spacing, and loading states, prevent manager search shadows from being clipped, and add a sidebar shortcut to file transfers while preserving existing hover feedback.
- **Sidebar and tab switching**: Reduce repeated layout measurements while collapsing or expanding the sidebar and file area, and improve scrollbar size updates and workspace synchronization after switching tabs.

### Main PRs and issues

- [PR #362](https://github.com/St0ff3l/fileterm/pull/362): Add signed in-app updates for supported Linux packages.
- [PR #365](https://github.com/St0ff3l/fileterm/pull/365): Keep the Linux sidebar width stable after startup.
- [PR #366](https://github.com/St0ff3l/fileterm/pull/366): Add remote-to-remote transfers and refine the workspace UI; related to [Issue #354](https://github.com/St0ff3l/fileterm/issues/354).

See the [comparison between v2.3.1 and v2.3.2](https://github.com/St0ff3l/fileterm/compare/v2.3.1...v2.3.2) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
