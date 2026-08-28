## FileTerm 2.2.6

FileTerm 2.2.6 聚焦 Tauri 工作区稳定性、远程兼容性、本地会话安全和桌面体验收口。

### 2.2.6 更新重点

- **终端与工作区稳定性**：保留工作区切换时的终端和 TUI 状态，隔离过期标签的输入、焦点、滚轮、尺寸同步和异步恢复，改善分屏、文件面板和本地终端的交互可靠性。
- **SSH/SFTP 与远程兼容性**：为连接提示、SFTP 初始化、远程列表和后台操作增加超时、取消与恢复路径；改进动态 Home、路径命名空间、受限 POSIX/FreeBSD 平台、系统指标以及 root/sudo 状态同步。
- **本地安全与凭据边界**：新增可配置的本地会话锁定、启动锁定和解锁重试；敏感凭据继续留在 Rust 存储边界，公开 profile 快照只保留必要的存在性和状态信息。
- **AI、连接设置与桌面体验**：完善 AI 会话生命周期、连接配置、会话日志、主题和共享 UI 控件，并修复锁屏状态下窗口拖动与退出快捷键交互。
- **Portable 与发版稳定性**：增强 Windows portable 配置迁移、字体导入诊断和跨平台发布产物处理，同时记录网络设备兼容性的后续实施边界。

### 本版本包含的主要 PR 和问题修复

- [PR #223](https://github.com/St0ff3l/fileterm/pull/223)：Tauri 工作区、终端生命周期、SSH/SFTP 兼容性、本地安全、设置界面和发布产物的稳定性改进。

完整变更记录请查看 [v2.2.5 与 v2.2.6 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.5...v2.2.6)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.6

FileTerm 2.2.6 focuses on Tauri workspace stability, remote compatibility, local session security, and desktop experience polish.

### Highlights

- **Terminal and workspace stability**: Preserve terminal and TUI state across workspace switches, isolate input, focus, wheel, resize, and asynchronous recovery work from stale tabs, and improve split-pane, file-panel, and local-terminal reliability.
- **SSH/SFTP and remote compatibility**: Add bounded timeouts, cancellation, and recovery paths for connection prompts, SFTP initialization, remote listings, and background operations; improve dynamic home resolution, path namespaces, restricted POSIX/FreeBSD support, system metrics, and root/sudo state synchronization.
- **Local security and credential boundaries**: Add configurable local session locking, startup lock, and unlock retry handling; keep sensitive credentials inside the Rust storage boundary while exposing only necessary presence and status information in public profile snapshots.
- **AI, connection settings, and desktop experience**: Improve AI session lifecycle, connection configuration, session logs, themes, and shared UI controls, and fix window dragging and quit-shortcut interactions while the workspace is locked.
- **Portable and release stability**: Improve Windows portable configuration migration, font-import diagnostics, and cross-platform release artifact handling, while documenting the follow-up boundary for network-device compatibility.

### Main PRs and issues

- [PR #223](https://github.com/St0ff3l/fileterm/pull/223): Stability improvements across the Tauri workspace, terminal lifecycle, SSH/SFTP compatibility, local security, settings UI, and release artifacts.

See the [comparison between v2.2.5 and v2.2.6](https://github.com/St0ff3l/fileterm/compare/v2.2.5...v2.2.6) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
