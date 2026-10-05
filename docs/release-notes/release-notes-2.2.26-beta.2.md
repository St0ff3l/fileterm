## FileTerm 2.2.26-beta.2

本测试版修复使用不同等宽字体时，鼠标拖选终端特殊字符会被背景遮住的问题。

### 2.2.26-beta.2 更新重点

- **终端显示**：拖选期间会持续正确绘制选区，避免空心方框、复选框及其他特殊字形被相邻单元格背景遮挡。覆盖 JetBrains Mono、Cascadia Code 和 macOS 系统等宽字体。
- **测试版说明**：这是预发布版本，请通过 GitHub Release 页面下载并反馈问题。

### 本版本包含的主要 PR 和问题修复

- [PR #290](https://github.com/St0ff3l/fileterm/pull/290)：修复鼠标拖选期间特殊字符被终端单元格背景遮挡的问题。

完整变更记录请查看 [v2.2.26-beta.1 与 v2.2.26-beta.2 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.1...v2.2.26-beta.2)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.26-beta.2

This beta fixes special terminal glyphs being covered while mouse-selecting with different monospace fonts.

### Highlights

- **Terminal rendering**: Selection stays correct during mouse dragging, so hollow squares, checkboxes, and other special glyphs are not obscured by neighboring cell backgrounds. JetBrains Mono, Cascadia Code, and the macOS system monospace font are covered.
- **Beta note**: This is a prerelease. Download it from the GitHub Release page and share feedback there.

### Main PRs and issues

- [PR #290](https://github.com/St0ff3l/fileterm/pull/290): Fixes special terminal glyphs being obscured by cell backgrounds during mouse dragging.

See the [comparison between v2.2.26-beta.1 and v2.2.26-beta.2](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.1...v2.2.26-beta.2) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
