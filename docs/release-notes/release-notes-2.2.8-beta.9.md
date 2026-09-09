## FileTerm 2.2.8-beta.9

FileTerm 2.2.8-beta.9 improves macOS Overlay window-control hover behavior and refines the bookmark dialog focus experience.

### 2.2.8-beta.9 更新重点

- **macOS 窗口控制**：同步原生标题栏容器与 Overlay 标题栏高度，修复鼠标位于可见控制按钮上时不显示按钮内容的问题。
- **收藏弹窗焦点**：优化路径收藏弹窗的键盘焦点循环，保留按钮焦点反馈并避免弹窗容器显示多余描边。
- **兼容性**：修复适用于 macOS 26、27 及其他使用 Overlay 标题栏的环境，继续保留 AppKit 原生窗口控制绘制。

### 本版本包含的主要 PR 和问题修复

- [PR #247](https://github.com/St0ff3l/fileterm/pull/247)：修复 macOS Overlay 窗口控制按钮的 hover 区域与可见位置不一致。
- [PR #246](https://github.com/St0ff3l/fileterm/pull/246)：优化路径收藏弹窗的焦点管理与按钮焦点样式。

完整变更记录请查看 [v2.2.8-beta.8 与 v2.2.8-beta.9 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.8...v2.2.8-beta.9)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.8-beta.9

FileTerm 2.2.8-beta.9 improves macOS Overlay window-control hover behavior and refines bookmark dialog focus handling.

### Highlights

- **macOS window controls**: Align the native title-bar container with the Overlay title bar so control contents appear when the pointer is over the visible buttons.
- **Bookmark dialog focus**: Improve keyboard focus trapping and preserve visible button focus feedback without an extra outline on the dialog container.
- **Compatibility**: Fixes the behavior on macOS 26, 27, and other Overlay title-bar environments while retaining AppKit's native window-control rendering.

### Main PRs and issues

- [PR #247](https://github.com/St0ff3l/fileterm/pull/247): Align the macOS Overlay window-control hover region with the visible controls.
- [PR #246](https://github.com/St0ff3l/fileterm/pull/246): Refine bookmark dialog focus management and button focus styling.

See the [comparison between v2.2.8-beta.8 and v2.2.8-beta.9](https://github.com/St0ff3l/fileterm/compare/v2.2.8-beta.8...v2.2.8-beta.9) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
