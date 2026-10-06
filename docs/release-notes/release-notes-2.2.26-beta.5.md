## FileTerm 2.2.26-beta.5

本测试版修复 AI Copilot 窗口顶部布局，并改进开发版的数据共享和 Linux 主机信息采集。

**安装指南 / Installation guides**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-1677FF?style=for-the-badge)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)
[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-1677FF?style=for-the-badge)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### 2.2.26-beta.5 更新重点

- **AI Copilot 窗口**：Windows/Linux 下延伸菜单栏右侧背景，并将 Copilot 面板顶部与 28px 菜单栏对齐，取消面板右上圆角。
- **开发版配置共享**：开发版与安装版共用配置和日志目录，开发版仍保留独立应用身份；Windows portable 继续使用 exe 旁的独立目录，历史 `.dev` 目录不会读取或迁移。
- **Linux 系统信息**：当 `hostname` 未返回主机名时，改用 `uname -n` 作为备用来源，避免系统信息侧栏因主机身份缺失而无法显示。

### 本版本包含的主要 PR 和问题修复

- [PR #296](https://github.com/St0ff3l/fileterm/pull/296)：修复 AI Copilot 顶部布局、开发版与安装版配置共享，以及 POSIX 主机名采集备用逻辑。

完整变更记录请查看 [v2.2.26-beta.4 与 v2.2.26-beta.5 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.4...v2.2.26-beta.5)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.26-beta.5

This beta fixes the AI Copilot window layout and improves development-build data sharing and Linux host information collection.

### Highlights

- **AI Copilot window**: On Windows/Linux, extend the menu bar background across the right side, align the Copilot panel below the 28px menu bar, and square its top-right corner.
- **Shared development data**: The development build shares its settings and logs with the installed app while keeping a distinct application identity. Windows portable builds continue to use the directory beside the executable; the historical `.dev` directory is not read or migrated.
- **Linux system information**: Use `uname -n` when `hostname` returns no host name, so the system information sidebar can still identify the host.

### Main PRs and issues

- [PR #296](https://github.com/St0ff3l/fileterm/pull/296): Fixes the AI Copilot top layout, shares development and installed app data, and adds a POSIX host name fallback.

See the [comparison between v2.2.26-beta.4 and v2.2.26-beta.5](https://github.com/St0ff3l/fileterm/compare/v2.2.26-beta.4...v2.2.26-beta.5) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
