## FileTerm 2.2.13

本版本修复 Windows 上滚动时意外缩放应用页面的问题，并新增可调整的文件列表字号。

### 2.2.13 更新重点

- **Windows 页面缩放**：在终端以外的区域滚动时，不再意外缩放整个应用页面；原有终端缩放操作保持不变。
- **文件列表字号**：可在设置中将文件列表字号调整为 9–24 px，也可通过视图菜单放大、缩小或重置；支持锁定视图菜单中的缩放操作。

### 本版本包含的主要 PR 和问题修复

- [PR #261](https://github.com/St0ff3l/fileterm/pull/261)：修复 Windows WebView 页面滚轮缩放，并增加文件列表字号调整与缩放锁定。

完整变更记录请查看 [v2.2.12 与 v2.2.13 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.12...v2.2.13)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.13

This release fixes accidental Windows application page zoom while scrolling and adds adjustable file-list font sizing.

### Highlights

- **Windows page zoom**: Scrolling outside the terminal no longer accidentally zooms the entire application page; existing terminal zoom controls are unchanged.
- **File-list font size**: Set the file-list font size to 9–24 px in Settings, or zoom in, zoom out, and reset it from the View menu. View-menu zoom actions can also be locked.

### Main PRs and issues

- [PR #261](https://github.com/St0ff3l/fileterm/pull/261): Fix Windows WebView page zoom and add file-list font sizing and zoom locking.

See the [comparison between v2.2.12 and v2.2.13](https://github.com/St0ff3l/fileterm/compare/v2.2.12...v2.2.13) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
