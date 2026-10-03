## FileTerm 2.2.23 Beta 2

Beta 2 延续 Windows 便携版应用内更新测试，供用户验证从 Beta 1 下载并替换到 Beta 2 的升级链路。

### 2.2.23-beta.2 更新重点

- **便携版升级验证**：版本号更新至 Beta 2，便于通过 Beta 通道从 Beta 1 安装本版本。
- **重点检查**：确认下载进度结束后应用退出，新 EXE 写回原目录与原文件名，应用能重新启动，连接配置和便携标记保持不变。
- **问题恢复**：请同时验证更新包签名错误、程序目录不可写、EXE 被占用和更新后无法启动时的错误提示与回滚结果。

### 本版本包含的主要 PR 和问题修复

- Beta 测试版本号更新，用于验证 Beta 1 到 Beta 2 的应用内升级。

完整变更记录请查看 [v2.2.23-beta.1 与 v2.2.23-beta.2 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.23-beta.1...v2.2.23-beta.2)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.23 Beta 2

Beta 2 continues testing in-app updates for the Windows portable build. Use it to verify the update from Beta 1.

### Highlights

- **Portable update verification**: The version advances to Beta 2 so it can be installed from Beta 1 through the Beta update channel.
- **Main checks**: Confirm the app exits after the download completes, the new executable is written to the original directory with the original filename, the app restarts, and connection data and the portable marker remain intact.
- **Failure recovery**: Also verify the error and rollback behavior for an invalid update signature, a non-writable program directory, a locked executable, and a failed restart.

### Main PRs and issues

- Beta version bump for testing the in-app update from Beta 1 to Beta 2.

See the [comparison between v2.2.23-beta.1 and v2.2.23-beta.2](https://github.com/St0ff3l/fileterm/compare/v2.2.23-beta.1...v2.2.23-beta.2) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
