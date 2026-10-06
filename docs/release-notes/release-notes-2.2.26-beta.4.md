## FileTerm 2.2.26-beta.4

本测试版统一各主题下的侧边栏文字颜色，并收紧 Windows/Linux 自绘菜单栏布局。

**安装指南 / Installation guides**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-1677FF?style=for-the-badge)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)
[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-1677FF?style=for-the-badge)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### 2.2.26-beta.4 更新重点

- **自绘菜单栏**：Windows/Linux 菜单轨道调整为 28px，标签栏保持 48px，工作区内容和下方分隔线随总高度减少 4px；最左侧菜单项的 hover 背景延伸到边缘。
- **Linux 窗口控制**：最小化、最大化和关闭圆形按钮调整为 20×20px，图标大小保持不变。
- **侧边栏文字**：主导航与设置侧栏的选中、未选中文字统一为主文字色，保留选中项背景，适用于全部主题。
- **平台范围**：菜单栏调整只作用于 Windows/Linux 自绘窗口框架，macOS 原生标题栏不变。请重点验证窗口最大化、恢复及 Linux 安装包中的布局。

### 本版本包含的主要 PR 和问题修复

- [PR #295](https://github.com/St0ff3l/fileterm/pull/295)：统一侧边栏文字颜色，修正菜单 hover 边缘，并压缩 Windows/Linux 菜单栏布局。

完整变更记录请查看 [v2.2.26-beta.3 与 v2.2.26-beta.4 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.3...v2.2.26-beta.4)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.26-beta.4

This beta unifies sidebar text colors across themes and tightens the custom Windows/Linux menu bar layout.

### Highlights

- **Custom menu bar**: The Windows/Linux menu rail is reduced to 28px while the tab rail remains 48px, moving workspace content and its lower divider up by 4px. The first menu item's hover background reaches the left edge.
- **Linux window controls**: The minimize, maximize, and close circles are reduced to 20×20px while their icons keep the same size.
- **Sidebar text**: Selected and unselected text in the main navigation and settings sidebar uses the same primary text color across all themes; selected-row backgrounds remain.
- **Platform scope**: The menu bar changes apply only to the custom Windows/Linux frame. The native macOS title bar is unchanged. Please focus testing on maximize/restore and Linux package layouts.

### Main PRs and issues

- [PR #295](https://github.com/St0ff3l/fileterm/pull/295): Unifies sidebar text colors, restores the menu hover edge, and tightens the custom Windows/Linux menu bar layout.

See the [comparison between v2.2.26-beta.3 and v2.2.26-beta.4](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.3...v2.2.26-beta.4) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
