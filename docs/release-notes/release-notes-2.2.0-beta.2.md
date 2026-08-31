## FileTerm 2.2.0 Beta 2

这是一个围绕 AI Copilot 执行链路、主题自定义和终端工作区配色改进的测试版本。在保留现有默认深色与浅色主题视觉的同时，Beta 2 提供了更完整的主题导入/导出、终端调色板和工作区界面配色控制。

### Beta 2 更新重点

- **AI Copilot 执行链路**：完善纯对话、半自动和全自动模式，统一工具调用、审批、执行结果、终端交接和凭据等待状态的展示与同步。
- **安全执行边界**：继续收紧普通后台 exec、可见终端、sudo/su、MFA/REPL 和敏感凭据之间的边界，危险操作保留本地护栏与用户审批。
- **主题自定义**：在设置中提供 FileTerm/Codex 主题预设、浅色/暗色选择、主题导入/复制、基础颜色、字体、语义色以及终端 ANSI 和搜索调色板。
- **主题兼容与导出**：支持 `codex-theme-v1:` 导入；复制当前 FileTerm 主题时使用 FileTerm 自有的 `fileterm-theme-v1:` 格式，避免将 FileTerm 主题误标为 Codex 主题。
- **工作区配色覆盖**：补齐主界面、侧边栏、顶部标签、连接后的文件区域、底部命令栏、子窗口、Monaco、xterm 和终端搜索等界面的主题映射；默认主题外观保持不变。
- **跨平台质量与验收**：继续完善 macOS、Windows、Linux 的主题设置、终端工作区、图标、按钮、滚动和发行验证链路。

### 本版本包含的主要 PR 和问题修复

- [PR #198](https://github.com/St0ff3l/fileterm/pull/198)：增加可自定义主题、终端调色板和完整的主题导入/导出链路，并修复 UI issue #186。
- [Issue #186](https://github.com/St0ff3l/fileterm/issues/186)：完善终端、编辑器和连接文件区域等界面的主题配色控制。

完整变更记录请查看 [v2.2.0-beta.1 与 v2.2.0-beta.2 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.0-beta.1...v2.2.0-beta.2)。

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

### 测试版说明

- 这是 Beta 测试版，AI Copilot、主题编辑器和终端工作区的交互与视觉细节仍可能继续调整。
- AI 执行前请确认目标、权限、工作目录和副作用；半自动模式会逐次请求审批。
- 主题导入前请确认配置来源可信；自定义颜色可能降低对比度或影响终端 ANSI 显示效果。
- sudo/su 缺少凭据时，可使用 profile、主窗口安全输入，或在用户明确同意后传递一次性密码；不要把密码写进命令文本。

---

## FileTerm 2.2.0 Beta 2

FileTerm 2.2.0 Beta 2 is a test release focused on the AI Copilot execution flow, theme customization, and terminal workspace colors. It preserves the existing default dark and light appearance while adding a fuller theme import/export flow, terminal palettes, and workspace color coverage.

### Beta 2 Highlights

- **AI Copilot execution flow**: Refine pure chat, semi-automatic, and fully automatic modes, with synchronized tool activity, approvals, execution results, terminal handoff, and credential-wait states.
- **Secure execution boundaries**: Keep background exec, visible terminals, sudo/su, MFA/REPL interactions, and sensitive credentials separated, with local guardrails and approval for risky operations.
- **Theme customization**: Add FileTerm/Codex presets, light/dark selection, theme import/copy, base colors, fonts, semantic colors, and terminal ANSI/search palettes to Settings.
- **Theme compatibility and export**: Accept `codex-theme-v1:` imports; copying a FileTerm theme exports the FileTerm-owned `fileterm-theme-v1:` format instead of labeling it as a Codex theme.
- **Workspace color coverage**: Extend theme mappings across the app shell, sidebar, top tabs, connected file areas, bottom command bar, child windows, Monaco, xterm, and terminal search while preserving the default theme appearance.
- **Cross-platform quality and validation**: Continue polishing theme settings, terminal workspaces, icons, buttons, scrolling, and release validation across macOS, Windows, and Linux.

### Main PRs and Issues

- [PR #198](https://github.com/St0ff3l/fileterm/pull/198): Add customizable themes, terminal palettes, and the theme import/export flow; also address UI issue #186.
- [Issue #186](https://github.com/St0ff3l/fileterm/issues/186): Improve theme color coverage for terminal, editor, and connected file areas.

See the [comparison between v2.2.0-beta.1 and v2.2.0-beta.2](https://github.com/St0ff3l/fileterm/compare/v2.2.0-beta.1...v2.2.0-beta.2) for the complete change set.

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Never include passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81), or join QQ group `534418986`.

### Beta Notes

- This is a Beta release; AI Copilot, the theme editor, and terminal workspace interactions and visual details may continue to change.
- Review the target, permissions, working directory, and side effects of AI tool calls; semi-automatic mode asks before each call.
- Verify imported theme configurations before applying them; custom colors may reduce contrast or affect terminal ANSI readability.
- For sudo/su, use a profile secret, the secure prompt, or an explicitly user-provided one-shot password. Never put a password in the command text.
