## FileTerm 2.2.8-beta.2

FileTerm 2.2.8-beta.2 是面向 SSH 跳板机和老系统连接场景的诊断测试版，重点验证资源侧栏获取失败时是否能自动降级而不影响终端和 SFTP。

### 2.2.8-beta.2 更新重点

- **跳板机与旧系统兼容性**：改进 CentOS/RHEL 7 类 POSIX 主机的平台探测、非交互采集和本地化输出处理；资源采集失败不会中断目标机终端或文件连接。
- **侧栏稳定性**：为指标通道增加 watchdog、退出状态、远端 stderr、EOF/Close 和超时诊断；采集能力关闭后会通过 workspace snapshot 同步到前端并折叠资源侧栏。
- **跳板认证诊断**：保留跳板机与目标机独立的认证阶段、请求序号和跳数信息，便于定位密码、键盘交互或 OTP 在哪一跳失败。
- **安全边界**：日志只记录请求 ID、跳数、状态、字节数和受限的错误尾部，不记录 OTP、密码、私钥口令或交互答案；本版本不新增 FIDO2/Web SSO 流程，也未宣称完成 CentOS 7 实机验证。

### 本版本包含的主要 PR 和问题修复

- [PR #230](https://github.com/St0ff3l/fileterm/pull/230)：跳板机资源侧栏诊断与降级修复，覆盖平台探测、指标 channel 关闭原因、snapshot 发送/接收/应用以及侧栏折叠状态。

完整变更记录请查看 [v2.2.8-beta.1 与 v2.2.8-beta.2 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.1...v2.2.8-beta.2)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.8-beta.2

FileTerm 2.2.8-beta.2 is a diagnostic beta for SSH jump-host and legacy-system workflows, focused on degrading resource-sidebar collection failures without affecting the terminal or SFTP.

### Highlights

- **Jump-host and legacy-system compatibility**: Improve platform detection, non-interactive collection, and locale-stable output handling for CentOS/RHEL 7-class POSIX hosts; a monitoring failure no longer interrupts the target terminal or file connection.
- **Sidebar stability**: Add watchdog, exit-status, remote-stderr, EOF/Close, and timeout diagnostics for the metrics channel; after monitoring is disabled, a workspace snapshot synchronizes the capability to the renderer and collapses the resource sidebar.
- **Jump authentication diagnostics**: Preserve independent jump-host and target authentication stages with request sequence and hop information, making password, keyboard-interactive, and OTP failures attributable to a specific hop.
- **Security boundary**: Logs contain only request IDs, hop metadata, status, byte counts, and a bounded error tail; OTPs, passwords, private-key passphrases, and interactive answers are not recorded. This release does not add a FIDO2/Web SSO flow and has not been claimed as an in-person CentOS 7 validation.

### Main PRs and issues

- [PR #230](https://github.com/St0ff3l/fileterm/pull/230): Jump-host resource-sidebar diagnostics and graceful-degradation fixes covering platform probing, metrics-channel close reasons, snapshot emission/receipt/application, and sidebar collapse state.

See the [comparison between v2.2.8-beta.1 and v2.2.8-beta.2](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.1...v2.2.8-beta.2) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
