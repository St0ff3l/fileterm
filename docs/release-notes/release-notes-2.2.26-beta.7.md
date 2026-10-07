## FileTerm 2.2.26-beta.7

本测试版改进 Linux 桌面窗口外观，让窗口圆角跟随本机桌面会话，并避免 LXQt 等直角桌面和远程会话出现不匹配的圆角。

**安装指南 / Installation guides**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-1677FF?style=for-the-badge)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)
[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-1677FF?style=for-the-badge)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### 2.2.26-beta.7 更新重点

- **Linux 桌面圆角适配**：按本机桌面和会话类型选择窗口外框；仅 GNOME Wayland 保留 15px 圆角，LXQt、KDE、Xfce、X11、远程桌面和未知环境使用直角，最大化窗口始终使用直角。此策略不依赖 DEB、RPM 或 AppImage 包格式。
- **Linux 窗口控制按钮**：最小化、最大化和关闭按钮在静止状态下持续显示圆形底色，并保留悬停反馈。

### 本版本包含的主要 PR 和问题修复

- [PR #297](https://github.com/St0ff3l/fileterm/pull/297)：新增按桌面会话识别的 Linux 窗口圆角策略，并将 LXQt 等直角桌面的窗口外框回退为直角。

完整变更记录请查看 [v2.2.26-beta.6 与 v2.2.26-beta.7 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.6...v2.2.26-beta.7)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.26-beta.7

This beta improves Linux desktop window chrome by adapting its frame corners to the local desktop session, avoiding mismatched rounded edges on square-corner desktops such as LXQt and in remote sessions.

**Installation guides / 安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-1677FF?style=for-the-badge)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)
[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-1677FF?style=for-the-badge)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Linux desktop corners**: Select the window frame style from the local desktop and session type. Only GNOME Wayland retains 15px corners; LXQt, KDE, Xfce, X11, remote desktops, and unknown environments use square corners. Maximized windows always use square corners. The policy is independent of DEB, RPM, or AppImage packaging.
- **Linux window controls**: Minimize, maximize, and close buttons keep their circular surfaces visible at rest and retain hover feedback.

### Main PRs and issues

- [PR #297](https://github.com/St0ff3l/fileterm/pull/297): Adds a conservative Linux window corner policy based on the local desktop session and uses square outer corners on desktops such as LXQt.

See the [comparison between v2.2.26-beta.6 and v2.2.26-beta.7](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.6...v2.2.26-beta.7) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
