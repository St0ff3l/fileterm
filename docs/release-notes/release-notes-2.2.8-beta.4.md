## FileTerm 2.2.8-beta.4

FileTerm 2.2.8-beta.4 是面向 JumpServer/KoKo SSH 网关的兼容性测试版，重点确保资源监控和文件面板只连接已经确认路由到的目标机。

### 2.2.8-beta.4 更新重点

- **网关路由识别**：识别 JumpServer/KoKo 的交互式资产选择菜单，区分“停留在跳板机”与“已经进入目标机”的会话，不再把跳板机菜单误当成 CentOS 资源数据。
- **目标机监控保护**：指标首个快照必须包含目标 hostname、OS 和 kernel 身份；无法确认目标身份时，自动关闭资源采集并折叠侧栏，终端仍保持可用。
- **文件通道稳定性**：交互式资产选择尚未完成时不再启动独立 SFTP/exec 通道，避免每个后台通道重新打开菜单、卡住或读取错误主机；已路由目标继续使用 PTY/login-shell 兼容路径。
- **可排查性与安全**：增加 route hint、跳板/目标握手、PTY、通道退出、目标身份、snapshot 和侧栏状态日志；菜单正文、密码、OTP、私钥口令和交互答案不会写入日志。本版本不新增 FIDO2/Web SSO 流程，仍需真实 KoKo + CentOS 7 环境验证。
- **使用边界**：JumpServer 请使用其文档中的直接资产登录格式（如 `JumpServerUser@AssetUser@AssetIP`），或配置真正提供 `direct-tcpip` 透传的普通 OpenSSH 跳板机；可参考 [JumpServer SSH 终端连接说明](https://www.jumpserver.com/blog/connecting-via-ssh-terminal)。

### 本版本包含的主要 PR 和问题修复

- [PR #235](https://github.com/St0ff3l/fileterm/pull/235)：识别 JumpServer/KoKo 交互式网关并对指标、SFTP 和侧栏进行目标路由保护，补充全链路诊断日志。

完整变更记录请查看 [v2.2.8-beta.3 与 v2.2.8-beta.4 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.3...v2.2.8-beta.4)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.8-beta.4

FileTerm 2.2.8-beta.4 is a compatibility beta for JumpServer/KoKo SSH gateways, ensuring that resource monitoring and file browsing are enabled only after the session is confirmed to be routed to the target host.

### Highlights

- **Gateway route detection**: Detect JumpServer/KoKo interactive asset-selection menus and distinguish a session still at the jump gateway from one that has reached the target host, avoiding false CentOS resource data from the gateway.
- **Target monitoring protection**: Require the first metrics snapshot to identify the target hostname, OS, and kernel; when identity cannot be confirmed, disable collection and collapse the sidebar while keeping the terminal usable.
- **File-channel stability**: Do not start independent SFTP/exec channels while interactive asset selection is pending, preventing each background channel from opening a fresh menu, hanging, or observing the wrong host; routed targets retain the PTY/login-shell compatibility path.
- **Diagnostics and security**: Add route-hint, jump/target handshake, PTY, channel-exit, target-identity, snapshot, and sidebar-state diagnostics. Menu contents, passwords, OTPs, private-key passphrases, and interactive answers are not logged. This release adds no FIDO2/Web SSO flow and still requires validation with a real KoKo + CentOS 7 environment.
- **Usage boundary**: For JumpServer, use its documented direct-asset username format such as `JumpServerUser@AssetUser@AssetIP`, or configure a regular OpenSSH jump host that provides transparent `direct-tcpip` forwarding; see the [JumpServer SSH terminal guide](https://www.jumpserver.com/blog/connecting-via-ssh-terminal).

### Main PRs and issues

- [PR #235](https://github.com/St0ff3l/fileterm/pull/235): Detect JumpServer/KoKo interactive gateways, protect metrics/SFTP/sidebar state until the target route is known, and add end-to-end diagnostics.

See the [comparison between v2.2.8-beta.3 and v2.2.8-beta.4](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.3...v2.2.8-beta.4) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
