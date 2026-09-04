## FileTerm 2.2.8-beta.5

FileTerm 2.2.8-beta.5 补齐 JumpServer/KoKo 直连用户名与 MFA 交互认证，并继续保护交互式资产菜单下的监控和文件通道。

### 2.2.8-beta.5 更新重点

- **Koko 直连用户名**：主机填写堡垒机地址，用户名支持 `堡垒机账号@目标账号@资产IP`；也支持末尾追加 `@堡垒机Host`，当它与主机字段一致时连接前会自动去重。
- **MFA 认证配置**：新增 `JumpServer / KoKo MFA Interactive` 选项。固定密码用于密码提示，OTP/MFA 在单独交互框输入，不默认拼接保存。
- **监控与文件通道**：直连目标后才启动资源监控和 SFTP；如果仍然返回 `Opt>` 资产菜单，终端保持可用，但会明确关闭无法路由到目标机的后台能力。Linux/BusyBox 在 `/proc` 不可用时增加有界的 `htop`/`top` 兼容回退。
- **导入与诊断**：外部配置导入会统一认证类型内部值；增加用户名规范化、路由状态、目标身份和能力降级日志，不记录菜单正文、密码、OTP、私钥口令或交互答案。
- **使用边界**：四段式用户名的末尾主机必须与 Host 字段一致；如果服务端仍只提供交互式资产菜单，客户端无法从独立后台通道推断前台菜单选择，监控和 SFTP 会保持关闭。

### 本版本包含的主要 PR 和问题修复

- [PR #237](https://github.com/St0ff3l/fileterm/pull/237)：JumpServer/KoKo 直连用户名规范化、MFA 认证选项、监控回退和导入兼容性修复。

完整变更记录请查看 [v2.2.8-beta.4 与 v2.2.8-beta.5 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.4...v2.2.8-beta.5)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.8-beta.5

FileTerm 2.2.8-beta.5 completes JumpServer/KoKo direct-username routing and MFA interactive authentication while keeping monitoring and file channels safe around interactive asset menus.

### Highlights

- **Koko direct username**: Put the bastion address in Host. Username accepts `bastion-user@target-account@asset-ip`, with an optional trailing `@bastion-host`; when it matches Host, the duplicate gateway host is removed before connecting.
- **MFA authentication**: Add `JumpServer / KoKo MFA Interactive`. The saved password answers password prompts, while OTP/MFA is entered in a separate interaction dialog and is not concatenated by default.
- **Monitoring and file channels**: Start resource monitoring and SFTP only after the target route is confirmed. If the server still returns an `Opt>` asset menu, the terminal remains usable while background capabilities that cannot identify the target are explicitly disabled. Linux/BusyBox adds bounded `htop`/`top` fallbacks when `/proc` is unavailable.
- **Import and diagnostics**: Normalize imported authentication values to internal enum values and add diagnostics for username normalization, route state, target identity, and capability downgrades. Menu contents, passwords, OTPs, private-key passphrases, and interactive answers are not logged.
- **Usage boundary**: The trailing host in a four-part username must match Host. If the server only exposes an interactive asset menu, the client cannot infer the foreground menu selection from an independent background channel, so monitoring and SFTP remain disabled.

### Main PRs and issues

- [PR #237](https://github.com/St0ff3l/fileterm/pull/237): JumpServer/KoKo direct-username normalization, MFA authentication selection, monitoring fallbacks, and import compatibility fixes.

See the [comparison between v2.2.8-beta.4 and v2.2.8-beta.5](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.4...v2.2.8-beta.5) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
