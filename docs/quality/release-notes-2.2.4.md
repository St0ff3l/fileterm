## FileTerm 2.2.4

FileTerm 2.2.4 聚焦终端会话、串口能力、远程文件操作和 Linux/Wayland 剪贴板体验的稳定性收口。

### 2.2.4 更新重点

- **终端与剪贴板**：拆分终端视图职责，改善字号与标签状态持久化；强化 Linux/Wayland 下 Ctrl+Shift+C/V、右键菜单和终端 Dock 按钮的复制粘贴路径，并避免多 pane 重复处理。
- **串口能力**：新增串口控制、协议传输与进度反馈，覆盖 XMODEM/YMODEM/ZMODEM/Kermit 等传输场景，并补充取消、重试、校验和平台串口参数处理。
- **SSH/FTP/Telnet 与远程文件**：增强重连、日志、文件完整性、FTP 符号链接、权限/Root 状态、递归目录和错误分类，减少远程会话与传输过程中的状态错乱。
- **跨平台与界面稳定性**：统一本地终端启动行为、国际化和文件时间显示，完善远程能力摘要布局及相关构建依赖；剪贴板内容仍只按用户操作留在本机，不会被终端复制粘贴逻辑额外发送到远端。
- **终端跟随与关闭确认**：修复 SSH 首次连接时过期 SFTP 列表覆盖终端初始目录的问题，补充上传临时文件标识，统一 Cmd+W/Cmd+Q 关闭确认不主动聚焦，并修正 FileTerm 主题焦点描边与 Linux 终端 Dock 提示布局。
- **验证**：通过 Tauri 类型检查、Lint、Prettier、Rust Clippy、433 个 Rust 单测和 19 个契约测试，并在 CI 中继续执行生产构建与协议夹具检查。

### 本版本包含的主要 PR 和问题修复

- [PR #215](https://github.com/St0ff3l/fileterm/pull/215)：终端会话、串口传输、远程文件操作、跨平台 UI 和 Linux/Wayland 剪贴板稳定性收口。
- [PR #217](https://github.com/St0ff3l/fileterm/pull/217)：修复终端目录跟随初始同步、上传临时文件展示、关闭确认焦点和相关跨平台 UI 问题。

完整变更记录请查看 [v2.2.3 与 v2.2.4 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.3...v2.2.4)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.4

FileTerm 2.2.4 focuses on stabilizing terminal sessions, serial capabilities, remote file operations, and Linux/Wayland clipboard behavior.

### Highlights

- **Terminal and clipboard**: Split terminal-view responsibilities, improve terminal font-size and tab-state persistence, and harden Ctrl+Shift+C/V, context-menu, and terminal Dock copy/paste paths on Linux/Wayland without duplicate handling across panes.
- **Serial capabilities**: Add serial controls, protocol transfers, and progress feedback across XMODEM, YMODEM, ZMODEM, and Kermit scenarios, with coverage for cancellation, retries, verification, and platform serial parameters.
- **SSH/FTP/Telnet and remote files**: Improve reconnects, session logs, file integrity, FTP symlink handling, permission/root-state synchronization, recursive directory operations, and error classification to keep remote sessions and transfers consistent.
- **Cross-platform and UI stability**: Align local-terminal startup behavior, localization, and file-time display; refine remote-capability layout and related build dependencies. Clipboard content remains local to the user's action and is not additionally sent to the remote host by terminal copy/paste handling.
- **Terminal follow and close confirmation**: Fix stale SFTP listings overriding the initial SSH terminal directory, label temporary upload files, keep Cmd+W/Cmd+Q close confirmations unfocused, and refine the FileTerm focus ring and Linux terminal Dock placeholder layout.
- **Validation**: Pass Tauri typecheck, lint, Prettier, Rust Clippy, 433 Rust unit tests, and 19 contract tests, with CI continuing to cover production builds and protocol fixtures.

### Main PRs and issues

- [PR #215](https://github.com/St0ff3l/fileterm/pull/215): Terminal sessions, serial transfers, remote file operations, cross-platform UI, and Linux/Wayland clipboard stability.
- [PR #217](https://github.com/St0ff3l/fileterm/pull/217): Initial terminal-directory follow-up sync, temporary upload-file labeling, close-confirmation focus, and related cross-platform UI fixes.

See the [comparison between v2.2.3 and v2.2.4](https://github.com/St0ff3l/fileterm/compare/v2.2.3...v2.2.4) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
