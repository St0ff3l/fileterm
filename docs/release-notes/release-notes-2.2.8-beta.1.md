## FileTerm 2.2.8-beta.1

FileTerm 2.2.8-beta.1 是一次 Beta 版本，重点完善 AI/MCP 工作流、SSH 稳定性、系统指标兼容性以及 Tauri 工作区的主题与交互收口。

### 2.2.8-beta.1 更新重点

- **AI 模型与能力配置**：支持按 Provider 配置模型能力、请求参数映射和图片输入声明，并允许手动输入模型 ID，便于接入未出现在内置列表中的模型。
- **MCP CLI 与 Agent 工作流**：支持长生命周期 JSONL bridge、多请求复用、后台命令与远程命令状态会话，并发布显式的 Agent workflow contract；远程操作仍受访问策略、显式授权和人工确认边界约束。
- **SSH 与系统兼容性**：强化跳板诊断、认证交互、取消与 MCP 执行通道恢复；修正 FreeBSD 资源指标在账户配额场景下的计算范围。
- **主题与桌面交互**：收敛 Tauri renderer 的主题 token、通用 UI 组件和 CSS contract，改进主题设置、下拉框、终端工作区边框、滚动条与跨平台视觉一致性。
- **文档与质量**：补充 FileTerm CLI 与长生命周期 bridge 文档，增加 MCP bridge、命令会话和 Agent contract 的测试覆盖。

### 本版本包含的主要 PR 和问题修复

- [PR #229](https://github.com/St0ff3l/fileterm/pull/229)：集中改进 AI/MCP 工作流、SSH 稳定性、FreeBSD 指标兼容性以及主题和工作区交互。

完整变更记录请查看 [v2.2.7 与 v2.2.8-beta.1 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.7...v2.2.8-beta.1)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.8-beta.1

FileTerm 2.2.8-beta.1 is a beta release focused on AI/MCP workflows, SSH stability, system-metrics compatibility, and Tauri workspace and theme polish.

### Highlights

- **AI model and capability configuration**: Configure model capabilities, request-parameter mappings, and image-input declarations per provider, with support for manually entering model IDs that are not present in the built-in list.
- **MCP CLI and Agent workflows**: Add long-lived JSONL bridge sessions, multi-request reuse, background and remote command sessions, and an explicit Agent workflow contract. Remote operations remain bounded by access policies, explicit authorization, and human confirmation.
- **SSH and system compatibility**: Harden jump-host diagnostics, authentication interaction, cancellation, and MCP execution-channel recovery; correct FreeBSD resource metrics for account-quota scenarios.
- **Theme and desktop interaction**: Consolidate Tauri renderer theme tokens, shared UI components, and the CSS contract, while improving theme settings, dropdowns, terminal workspace framing, scrollbars, and cross-platform visual consistency.
- **Documentation and quality**: Expand the FileTerm CLI and long-lived bridge documentation, and add coverage for MCP bridge routing, command sessions, and the Agent contract.

### Main PRs and issues

- [PR #229](https://github.com/St0ff3l/fileterm/pull/229): Consolidated improvements to AI/MCP workflows, SSH stability, FreeBSD metrics compatibility, and theme and workspace interaction.

See the [comparison between v2.2.7 and v2.2.8-beta.1](https://github.com/St0ff3l/fileterm/compare/v2.2.7...v2.2.8-beta.1) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
