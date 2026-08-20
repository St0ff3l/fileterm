## FileTerm 2.2.0

FileTerm 2.2.0 将 AI Copilot、主题系统、远程文件工作流和跨平台桌面体验收敛为稳定版，并继续保持凭据与危险操作的人工确认边界。

### 2.2.0 更新重点

- **AI Copilot 与安全执行**：完善对话、半自动和全自动执行链路，统一工具活动、审批、结果、终端交接与凭据等待状态；普通后台 exec 与可见终端保持隔离，sudo/su、MFA、确认和 REPL 仍由用户在明确边界内处理。
- **主题、终端与字体**：支持 FileTerm/Codex 主题、自定义主题来源与明暗 variant、主题导入/导出、ANSI 普通色与亮色调色板、终端日志 ANSI 渲染，以及 UI 字体和终端等宽字体导入与配置。
- **远程文件工作流**：支持按连接记忆文件面板比例；改进远程文件编辑、符号链接保存、编码校验和 root/su/sudo 保存安全；完善远程文件拖到 FileTerm、本地文件管理器、Finder、Explorer 和桌面的跨平台行为。
- **系统指标与会话稳定性**：改进 CPU、内存、磁盘、网络和进程采样，增加资源指标选择与挂载列表布局修复，并稳定会话刷新、重连、文件面板和系统侧栏同步。
- **更新与桌面集成**：Windows 使用签名的 Tauri 应用内更新；macOS 继续跳转 GitHub Release 下载；同时收敛 macOS 原生窗口、Windows/Linux 拖放、设置弹窗、滚动条、标签栏和工作区布局。
- **跨平台质量**：持续通过 Rust/Tauri 单元与协议契约测试、renderer/shared 类型检查、静态检查、格式检查和 macOS/Windows/Linux 打包冒烟验证。

### 本版本包含的主要 PR 和问题修复

- [PR #202](https://github.com/St0ff3l/fileterm/pull/202)：收敛 2.2.0-beta.2 以来的主题、终端、文件面板、远程文件编辑、桌面拖放、系统指标、MCP/CLI 和更新流程改进。
- [Issue #188](https://github.com/St0ff3l/fileterm/issues/188)：增加并稳定按连接保存文件面板比例的工作流。
- [Issue #189](https://github.com/St0ff3l/fileterm/issues/189)：完善远程文件拖放到本地面板和系统文件管理器的处理。
- [Issue #180](https://github.com/St0ff3l/fileterm/issues/180)：跟进桌面工作流与跨平台交互稳定性。

完整变更记录请查看 [v2.2.0-beta.2 与 v2.2.0 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.0-beta.2...v2.2.0)。

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

### 使用提示

- AI 执行前请确认目标、权限、工作目录和副作用；半自动模式会逐次请求审批，全自动模式仍受本地危险命令和目标绑定护栏约束。
- 主题、字体和终端配置导入前请确认来源可信；自定义颜色可能降低对比度或影响终端 ANSI 显示效果。
- sudo/su 缺少凭据时，请使用 profile、主窗口安全输入或用户明确同意的一次性密码；不要把密码写进命令文本、终端结果或 MCP 返回内容。
- Windows portable 版本需要系统已有 WebView2 Runtime；macOS 更新会打开 GitHub Release 页面供用户下载。

---

## FileTerm 2.2.0

FileTerm 2.2.0 brings AI Copilot, theme customization, remote file workflows, and cross-platform desktop behavior together as a stable release while preserving explicit user confirmation boundaries for credentials and risky actions.

### Highlights

- **AI Copilot and secure execution**: Refine conversation, semi-automatic, and fully automatic execution flows with unified tool activity, approvals, results, terminal handoff, and credential-wait states. Background exec remains separate from the visible terminal, while sudo/su, MFA, confirmations, and REPLs stay within explicit user-controlled boundaries.
- **Themes, terminal colors, and fonts**: Support FileTerm/Codex themes, custom theme sources and light/dark variants, theme import/export, normal and bright ANSI palettes, ANSI terminal-log rendering, and configurable UI and terminal fonts with local font import.
- **Remote file workflows**: Remember the file-panel ratio per connection; improve remote editing, symlink preservation, encoding validation, and root/su/sudo saves; and refine remote-file dragging to FileTerm, Finder, Explorer, and desktop file managers across platforms.
- **System metrics and session stability**: Improve CPU, memory, disk, network, and process sampling, add resource-metric selection and mount-list layout fixes, and stabilize synchronization between session refresh, reconnects, file panels, and the system sidebar.
- **Updates and desktop integration**: Use signed in-app Tauri updates on Windows while continuing to send macOS users to GitHub Releases; also polish native macOS windows, Windows/Linux drag-and-drop, settings dialogs, scrollbars, tabs, and workspace layout.
- **Cross-platform quality**: Continue validating the release with Rust/Tauri unit and protocol-contract tests, renderer/shared type checks, lint and formatting checks, and macOS/Windows/Linux packaging smoke tests.

### Main PRs and issues

- [PR #202](https://github.com/St0ff3l/fileterm/pull/202): Consolidate the theme, terminal, file-panel, remote-editor, desktop-drag, system-metrics, MCP/CLI, and update-flow improvements since 2.2.0-beta.2.
- [Issue #188](https://github.com/St0ff3l/fileterm/issues/188): Add and stabilize per-connection file-panel ratio persistence.
- [Issue #189](https://github.com/St0ff3l/fileterm/issues/189): Improve remote-file dragging to the local panel and system file managers.
- [Issue #180](https://github.com/St0ff3l/fileterm/issues/180): Track desktop workflow and cross-platform interaction stability work.

See the [comparison between v2.2.0-beta.2 and v2.2.0](https://github.com/St0ff3l/fileterm/compare/v2.2.0-beta.2...v2.2.0) for the complete change set.

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not include passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).

### Usage notes

- Review the target, permissions, working directory, and side effects before AI execution; semi-automatic mode asks for approval per call, and fully automatic mode remains constrained by local dangerous-command and target-binding guardrails.
- Verify the source of imported themes, fonts, and terminal configurations; custom colors may reduce contrast or affect ANSI readability.
- For missing sudo/su credentials, use a profile, the secure main-window prompt, or an explicitly approved one-shot password. Never put passwords in command text, terminal results, or MCP responses.
- The Windows portable build requires an existing WebView2 Runtime; macOS updates open the GitHub Release page for user-initiated downloads.
