## FileTerm 2.2.8-beta.3

FileTerm 2.2.8-beta.3 是面向 SSH 跳板机资源监控的兼容性测试版，重点处理 SSH-2.0-Go 网关与目标机 OpenSSH 之间的 PTY、探针和监控通道衔接。

### 2.2.8-beta.3 更新重点

- **跳板机监控兼容**：平台探针发现目标入口要求 PTY 或返回 `No PTY requested` 时自动重试 PTY，并将兼容性决策传递给长期指标通道。
- **目标机采集执行**：PTY 模式使用 `bash --login` 和 Base64 单行命令执行 POSIX 采集脚本，避免向 PTY stdin 灌入长脚本导致卡死或回显污染；透明 `direct-tcpip` 场景继续与目标机 OpenSSH 端到端握手。
- **目标身份保护**：首个完整快照必须包含目标 hostname、OS 和 kernel 信息，确认拿到目标 CentOS/Linux 身份后才更新资源侧栏；无法确认时自动关闭监控，终端和 SFTP 仍可使用。
- **诊断与安全**：补充跳板机/目标机独立握手标识、PTY 重试、命令模式、退出码、远端 stdout/stderr 尾部、snapshot 发送/接收/应用和侧栏折叠原因日志；日志不记录密码、OTP、私钥口令或交互答案。本版本没有新增 FIDO2/Web SSO 流程，仍需真实 KoKo + CentOS 7 环境验证。

### 本版本包含的主要 PR 和问题修复

- [PR #233](https://github.com/St0ff3l/fileterm/pull/233)：SSH 跳板机资源监控兼容性修复，覆盖 SSH-2.0-Go 网关的 PTY 回退、目标机身份校验、监控通道生命周期和前后端状态同步。

完整变更记录请查看 [v2.2.8-beta.2 与 v2.2.8-beta.3 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.2...v2.2.8-beta.3)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.8-beta.3

FileTerm 2.2.8-beta.3 is a compatibility beta for SSH jump-host resource monitoring, focused on PTY, probing, and metrics-channel handoff between SSH-2.0-Go gateways and OpenSSH target hosts.

### Highlights

- **Jump-host monitoring compatibility**: Retry platform probes with a PTY when the target entry point requires one or returns `No PTY requested`, then carry that compatibility decision into the persistent metrics channel.
- **Target collection execution**: PTY mode runs POSIX collection through a single `bash --login` Base64 command, avoiding long scripts sent to PTY stdin and their echo or line-buffer failures; transparent `direct-tcpip` connections continue their end-to-end handshake with the OpenSSH target.
- **Target identity protection**: The first complete snapshot must identify the target hostname, OS, and kernel before the resource sidebar is updated; uncertain identity disables monitoring while leaving the terminal and SFTP available.
- **Diagnostics and security**: Add independent jump/target handshake identity, PTY retry, command-mode, exit-status, bounded remote stdout/stderr, snapshot emission/receipt/application, and sidebar-collapse diagnostics. Logs do not record passwords, OTPs, private-key passphrases, or interactive answers. This release adds no FIDO2/Web SSO flow and still requires validation on a real KoKo + CentOS 7 environment.

### Main PRs and issues

- [PR #233](https://github.com/St0ff3l/fileterm/pull/233): SSH jump-host resource-monitoring compatibility with PTY fallback for SSH-2.0-Go gateways, target identity validation, metrics-channel lifecycle, and renderer state synchronization.

See the [comparison between v2.2.8-beta.2 and v2.2.8-beta.3](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.2...v2.2.8-beta.3) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
