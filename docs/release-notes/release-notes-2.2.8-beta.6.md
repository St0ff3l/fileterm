## FileTerm 2.2.8-beta.6

FileTerm 2.2.8-beta.6 修复 JumpServer/KoKo MFA 认证顺序，并优化交互式资产菜单的提示体验。

### 2.2.8-beta.6 更新重点

- **KoKo MFA 认证**：先提交堡垒机密码，服务端返回部分认证成功后，再在同一 SSH 连接上继续 keyboard-interactive MFA，避免直接启动 MFA 导致认证失败。
- **诊断日志**：补充密码认证、部分成功和 MFA 继续阶段的日志，不记录密码、OTP 或交互答案。
- **菜单提示**：交互式资产菜单提示与其他状态提示保持 15 秒自动消失，并支持公共关闭按钮；菜单模式仍不会虚构后台监控和 SFTP 的目标资产。

### 本版本包含的主要 PR 和问题修复

- KoKo MFA 认证顺序修复、交互式资产菜单提示自动关闭和公共关闭按钮优化。

完整变更记录请查看 [v2.2.8-beta.5 与 v2.2.8-beta.6 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.5...v2.2.8-beta.6)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.8-beta.6

FileTerm 2.2.8-beta.6 fixes the JumpServer/KoKo MFA authentication order and improves interactive asset-menu feedback.

### Highlights

- **KoKo MFA authentication**: Submit the bastion password first, then continue keyboard-interactive MFA on the same SSH connection after the server reports partial success, avoiding authentication failures caused by starting MFA directly.
- **Diagnostics**: Add password-authentication, partial-success, and MFA-continuation logs without recording passwords, OTPs, or interactive answers.
- **Menu feedback**: Interactive asset-menu notices now share the 15-second auto-dismiss behavior of other status notices and use the shared close button. Menu mode still does not invent a target for background monitoring or SFTP.

### Main PRs and issues

- KoKo MFA authentication order fix, interactive asset-menu notice auto-dismiss, and shared close-button improvements.

See the [comparison between v2.2.8-beta.5 and v2.2.8-beta.6](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.5...v2.2.8-beta.6) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
