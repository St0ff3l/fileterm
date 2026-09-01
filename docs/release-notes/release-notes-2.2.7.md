## FileTerm 2.2.7

FileTerm 2.2.7 扩展了网络设备与后台会话能力，强化 SSH 多因素认证链路，并完成 Tauri 主链路的职责收敛。

### 2.2.7 更新重点

- **网络设备 SSH 兼容性**：新增受限范围的网络设备会话兼容支持，覆盖设备识别、会话启动与基础命令交互；Comware 旧版密钥交换兼容仅在精确识别且协商到指定算法时启用，不会降低常规 SSH 连接的默认安全算法。
- **后台会话与 Agent 工作流**：可在后台管理外部 CLI、MCP 与 Agent 会话，并增强连接复用、访问级别、连接白名单和执行取消/交接等受控工作流；涉及远程操作时仍需遵守既有的显式授权与人工确认边界。
- **SSH 认证稳定性**：修复跳板链路中的多因素认证续接与交互隔离，避免不同认证流程之间相互串扰。
- **架构与桌面稳定性**：将 Tauri commands、services、sessions 和 renderer 大模块按职责拆分，保留既有 IPC、协议与类型边界，为后续维护和测试提供更清晰的结构。

### 本版本包含的主要 PR 和问题修复

- [PR #227](https://github.com/St0ff3l/fileterm/pull/227)：网络设备兼容性、后台会话与 Agent 工作流、SSH MFA 跳板认证稳定性，以及 Tauri 主链路的职责拆分与测试收敛。

完整变更记录请查看 [v2.2.6 与 v2.2.7 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.6...v2.2.7)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.7

FileTerm 2.2.7 expands network-device and background-session capabilities, strengthens SSH MFA flows, and clarifies responsibility boundaries across the Tauri application.

### Highlights

- **Network-device SSH compatibility**: Add narrowly scoped compatibility for network-device sessions, including device detection, session startup, and basic command interaction. Legacy Comware key-exchange support activates only for an exact device match and the specified negotiated algorithm; it does not weaken the default algorithms used for ordinary SSH connections.
- **Background sessions and Agent workflows**: Manage external CLI, MCP, and Agent sessions in the background, with stronger connection reuse, access levels, allowlists, and controlled execution cancellation and handoff. Remote operations remain subject to the existing explicit-authorization and human-confirmation boundaries.
- **SSH authentication stability**: Preserve MFA continuation and isolate interactions on jump-host paths so separate authentication flows do not interfere with one another.
- **Architecture and desktop stability**: Split large Tauri commands, services, sessions, and renderer modules by responsibility while retaining IPC, protocol, and type boundaries, improving maintainability and testability.

### Main PRs and issues

- [PR #227](https://github.com/St0ff3l/fileterm/pull/227): Network-device compatibility, background sessions and Agent workflows, SSH MFA jump-host stability, and responsibility-based Tauri application decomposition with consolidated tests.

See the [comparison between v2.2.6 and v2.2.7](https://github.com/St0ff3l/fileterm/compare/v2.2.6...v2.2.7) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
