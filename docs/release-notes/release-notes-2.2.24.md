## FileTerm 2.2.24

FileTerm 2.2.24 改善终端文字和常用操作按钮的主题对比度，并修复浅色主题下本地终端左侧边界不清的问题。

### 2.2.24 更新重点

- **终端可读性**：调整内置亮色 ANSI 白色，并为终端启用 4.5 的最小文字对比度保护，涵盖 ANSI、256 色、RGB、反色、选区和搜索文字；仅改变屏幕显示颜色，不改写终端原始输出。
- **主题配色**：修正多个 iTerm2 主题的选区和搜索文字配色；主按钮与危险按钮会依据背景选择白色或深色文字，提升浅色按钮上的文字清晰度。
- **本地终端边界**：左右留边都在终端内缘显示分隔线，浅色主题下也能辨识终端区域。

### 本版本包含的主要 PR 和问题修复

- [PR #282](https://github.com/St0ff3l/fileterm/pull/282)：改善终端文字对比度、亮色主题选区/搜索配色、操作按钮文字颜色和本地终端边界。

完整变更记录请查看 [v2.2.23 与 v2.2.24 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.23...v2.2.24)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.24

FileTerm 2.2.24 improves theme contrast for terminal text and common action buttons, and restores the left terminal boundary in light themes.

### Highlights

- **Terminal readability**: Adjust the built-in light ANSI white and enable a 4.5 minimum text contrast ratio for ANSI, 256-color, RGB, inverse, selection, and search text. This changes rendered colors only and leaves raw terminal output untouched.
- **Theme colors**: Correct selection and search text pairs in several iTerm2 themes. Primary and danger buttons choose white or dark text based on their background for clearer labels on light-colored buttons.
- **Local terminal frame**: Draw dividers on the inner edges of both side gutters so the terminal boundary remains visible in light themes.

### Main PRs and issues

- [PR #282](https://github.com/St0ff3l/fileterm/pull/282): Improve terminal text contrast, light-theme selection/search colors, action button labels, and the local terminal frame.

See the [comparison between v2.2.23 and v2.2.24](https://github.com/St0ff3l/fileterm/compare/v2.2.23...v2.2.24) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
