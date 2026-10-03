## FileTerm 2.2.23 Beta 1

这是用于测试 Windows 便携版应用内更新的 Beta 版本，首次更新请先手动安装本版本的便携 EXE。

### 2.2.23-beta.1 更新重点

- **Windows 便携版应用内更新**：可在设置中检查更新、下载并验证签名，然后重启更新；新版 EXE 保持原目录与原文件名。
- **数据保留与恢复**：更新保留 EXE 旁的 `config` 数据与便携标记；替换失败会恢复原程序。更新将退出应用并中断当前会话。
- **测试提示**：首次从旧便携版升级到本测试版需手动覆盖；后续可测试 Beta 1 到 Beta 2 的应用内更新。请重点反馈重命名 EXE、含空格或中文的目录、配置保留及更新失败恢复情况。

### 本版本包含的主要 PR 和问题修复

- Windows 便携版签名更新包、退出后替换辅助流程与失败回滚。

完整变更记录请查看 [v2.2.22 与 v2.2.23-beta.1 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.22...v2.2.23-beta.1)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.23 Beta 1

This Beta release lets users test in-app updates for the Windows portable build. Manually install this portable executable for the first update.

### Highlights

- **Windows portable in-app updates**: Check for updates in Settings, download and verify the signature, then restart to update. The new executable stays in the same directory with the same filename.
- **Data preservation and recovery**: Updates preserve the adjacent `config` data and portable marker; a failed replacement restores the previous executable. Updating exits the app and interrupts active sessions.
- **Testing notes**: The first upgrade from an older portable build requires manually replacing the executable. Later, test the in-app update from Beta 1 to Beta 2. Please focus feedback on renamed executables, paths containing spaces or CJK characters, preserved data, and recovery after update failures.

### Main PRs and issues

- Windows portable signed update payload, post-exit replacement helper, and rollback flow.

See the [comparison between v2.2.22 and v2.2.23-beta.1](https://github.com/St0ff3l/fileterm/compare/v2.2.22...v2.2.23-beta.1) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
