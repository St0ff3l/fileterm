## FileTerm 2.2.23

FileTerm 2.2.23 为 Windows 便携版增加应用内更新，更新后保留程序位置和便携数据，并在替换失败时恢复原程序。

### 2.2.23 更新重点

- **Windows 便携版应用内更新**：可在应用内检查更新、下载并验证 Tauri 更新签名，然后退出应用，在原目录以原文件名替换 EXE；便携配置和标记会保留，替换失败时会恢复旧程序。
- **升级提示**：早于 2.2.23 的便携版首次升级需要手动下载并替换一次；之后可使用应用内更新。应用退出并安装更新时，当前会话会中断。
- **签名说明**：Tauri 更新签名用于验证下载的更新包，不等同于 Windows Authenticode 应用签名。

### 本版本包含的主要 PR 和问题修复

- [PR #279](https://github.com/St0ff3l/fileterm/pull/279)：为 Windows 便携版增加签名校验、应用内更新、原位置替换和失败回滚。

完整变更记录请查看 [v2.2.22 与 v2.2.23 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.22...v2.2.23)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.23

FileTerm 2.2.23 adds in-app updates for the Windows portable build, preserving the executable's location and portable data while restoring the previous executable if replacement fails.

### Highlights

- **Windows portable in-app updates**: Check for updates in the app, download and verify the Tauri updater signature, then exit and replace the executable in its original directory with the same filename. Portable configuration and marker files are preserved, and a failed replacement restores the previous executable.
- **Upgrade note**: Portable builds older than 2.2.23 require one manual download and replacement. Later updates can be installed in the app. Applying an update exits the app and interrupts active sessions.
- **Signature note**: The Tauri updater signature verifies downloaded update packages; it is separate from Windows Authenticode application signing.

### Main PRs and issues

- [PR #279](https://github.com/St0ff3l/fileterm/pull/279): Add signature verification, in-app updates, in-place replacement, and rollback for Windows portable builds.

See the [comparison between v2.2.22 and v2.2.23](https://github.com/St0ff3l/fileterm/compare/v2.2.22...v2.2.23) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
