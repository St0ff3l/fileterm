## FileTerm 2.2.17

修复 SSH 新建连接测试中的主机指纹保存问题和网络波动后的系统监控恢复，并改进首次启动语言选择与 Linux AppImage 运行权限。

### 2.2.17 更新重点

- **SSH 主机确认**：新建连接测试时选择“接受并保存”，指纹会随连接信息一起保存；“仅接受一次”仍只对本次连接有效。修改主机参数后会清除旧指纹，跳板机指纹也不会写入目标主机连接。
- **系统监控恢复**：监控采集通道中断后显示恢复状态并进行有限次数的自动重试；可手动重试、停止或启动监控，不影响 SSH 终端连接。
- **首次启动语言**：首次启动时优先使用系统语言（`zh` 使用中文，其他语言使用英语），已保存的语言选择继续优先；默认 README 改为英文，并保留中文版本。
- **Linux AppImage**：修复启动器执行权限，避免其他用户运行时出现 `Permission denied`。
- **诊断日志**：统一 Rust 诊断日志入口，并在正常退出时刷新缓冲日志，便于排查监控问题。

### 本版本包含的主要 PR 和问题修复

- [PR #268](https://github.com/St0ff3l/fileterm/pull/268)：保存新建连接测试中已确认的 SSH 主机指纹，避免正式连接时重复确认。
- [PR #269](https://github.com/St0ff3l/fileterm/pull/269)：恢复网络波动后的系统监控，增加重试和启停控制，并完善诊断日志。
- [PR #270](https://github.com/St0ff3l/fileterm/pull/270)：首次启动跟随系统语言，修复 Linux AppImage 启动权限。

完整变更记录请查看 [v2.2.16 与 v2.2.17 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.16...v2.2.17)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.17

Fix SSH host fingerprint persistence during connection tests and restore system monitoring after network interruptions, while improving first-launch language selection and Linux AppImage permissions.

### Highlights

- **SSH host confirmation**: Choosing “Accept and Save” during a new connection test stores the fingerprint with the connection; “Accept Once” remains valid only for that connection. Changing host settings clears the old fingerprint, and jump host fingerprints are not saved to the target connection.
- **System monitoring recovery**: When the monitoring channel is interrupted, FileTerm shows recovery status and performs a limited number of automatic retries. Users can retry, stop, or start monitoring without interrupting the SSH terminal session.
- **First-launch language**: The app follows the system language on first launch (`zh` selects Chinese; other languages select English), while a saved language choice takes precedence. The default README is now English, with the Chinese version retained.
- **Linux AppImage**: Fix launcher permissions that could cause `Permission denied` when run by another user.
- **Diagnostic logging**: Route Rust diagnostics through a unified logging entry point and flush buffered logs on normal shutdown to support monitoring investigations.

### Main PRs and issues

- [PR #268](https://github.com/St0ff3l/fileterm/pull/268): Persist SSH host fingerprints accepted during new connection tests, preventing duplicate confirmation on the first real connection.
- [PR #269](https://github.com/St0ff3l/fileterm/pull/269): Recover system monitoring after network interruptions, add retry and start/stop controls, and improve diagnostic logging.
- [PR #270](https://github.com/St0ff3l/fileterm/pull/270): Follow the system language on first launch and fix Linux AppImage launcher permissions.

See the [comparison between v2.2.16 and v2.2.17](https://github.com/St0ff3l/fileterm/compare/v2.2.16...v2.2.17) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
