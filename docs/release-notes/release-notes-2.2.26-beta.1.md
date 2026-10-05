## FileTerm 2.2.26-beta.1

本测试版改进 macOS DMG 安装界面，并修复终端选中特殊符号时显示不完整的问题。

### 2.2.26-beta.1 更新重点

- **终端显示**：选中文本时，星号、线框及块状符号保持完整；局部选区变化和取消选区后也会正确刷新。
- **Windows 本地终端**：按输入事件顺序串行写入 ConPTY，避免隐藏提示中的字符顺序错乱导致重复要求输入。
- **macOS 安装体验**：DMG 使用定制背景和 Finder 窗口布局。
- **测试版说明**：这是预发布版本，请通过 GitHub Release 页面下载并反馈问题。

### 本版本包含的主要 PR 和问题修复

- [PR #286](https://github.com/St0ff3l/fileterm/pull/286)：改善侧边栏未选中文字对比度。
- [PR #287](https://github.com/St0ff3l/fileterm/pull/287)：改进 macOS DMG 安装界面并修复终端特殊符号选区渲染。
- [PR #289](https://github.com/St0ff3l/fileterm/pull/289)：修复 Windows 本地终端输入乱序导致隐藏提示重复的问题。

完整变更记录请查看 [v2.2.25 与 v2.2.26-beta.1 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.25...v2.2.26-beta.1)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.26-beta.1

This beta improves the macOS DMG installer and fixes special symbols being clipped while selected in the terminal.

### Highlights

- **Terminal rendering**: Asterisks, box drawing, and block symbols remain intact while selected; partial selection changes and clearing the selection also repaint correctly.
- **Windows local terminal**: ConPTY input writes follow input-event order, preventing hidden prompts from repeating because characters arrive out of order.
- **macOS installation**: The DMG now uses a custom background and Finder window layout.
- **Beta note**: This is a prerelease. Download it from the GitHub Release page and share feedback there.

### Main PRs and issues

- [PR #286](https://github.com/St0ff3l/fileterm/pull/286): Improves contrast for inactive sidebar labels.
- [PR #287](https://github.com/St0ff3l/fileterm/pull/287): Improves the macOS DMG installer and fixes terminal special-symbol selection rendering.
- [PR #289](https://github.com/St0ff3l/fileterm/pull/289): Fixes out-of-order Windows local terminal input that caused hidden prompts to repeat.

See the [comparison between v2.2.25 and v2.2.26-beta.1](https://github.com/St0ff3l/fileterm/compare/v2.2.25...v2.2.26-beta.1) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
