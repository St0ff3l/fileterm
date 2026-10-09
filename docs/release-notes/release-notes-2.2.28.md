## FileTerm 2.2.28

FileTerm 2.2.28 improves SSH terminal and SFTP directory following, including privileged shells, and refines the workspace frame border.

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.2.28 更新重点

- **SSH 目录跟随与文件刷新**：`sudo -i`、`su -` 等交互式提权 shell 会同步工作目录和文件访问身份。终端命令完成后，文件面板在偏离终端目录时恢复跟随并显示加载状态；目录一致时不重复加载。命令造成文件变化时静默检查列表，仅在内容变化后更新。
- **终端粘贴选中文字**：右键终端选区可将选中文字粘贴到当前终端输入，不访问系统剪贴板，也不会自动回车。
- **工作区边框**：统一终端右侧框线颜色，暗色主题使用 `#383838`。

### 本版本包含的主要 PR 和问题修复

- [PR #305](https://github.com/St0ff3l/fileterm/pull/305)：新增右键粘贴终端选中文字，不使用系统剪贴板且不自动执行。
- [Issue #239](https://github.com/St0ff3l/fileterm/issues/239)：改进终端命令完成后的 SFTP 文件列表静默更新，并保持当前目录跟随行为。

完整变更记录请查看 [v2.2.27 与 v2.2.28 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.27...v2.2.28)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.28

FileTerm 2.2.28 improves SSH terminal and SFTP directory following, including privileged shells, and refines the workspace frame border.

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **SSH directory following and file refresh**: Interactive privileged shells such as `sudo -i` and `su -` synchronize the working directory and file access identity. After a terminal command completes, the file pane follows the terminal directory with a loading indicator when it has diverged; it skips redundant loads when paths match. Commands that change files trigger a quiet listing check and update the view only when its contents change.
- **Paste selected terminal text**: Right-clicking a terminal selection pastes it into the active terminal input without accessing the system clipboard or pressing Enter automatically.
- **Workspace frame**: Unifies the right terminal frame border color, using `#383838` in the dark theme.

### Main PRs and issues

- [PR #305](https://github.com/St0ff3l/fileterm/pull/305): Adds a context-menu action to paste selected terminal text without using the system clipboard or executing it automatically.
- [Issue #239](https://github.com/St0ff3l/fileterm/issues/239): Improves quiet SFTP file-list updates after terminal commands while preserving directory following.

See the [comparison between v2.2.27 and v2.2.28](https://github.com/St0ff3l/fileterm/compare/v2.2.27...v2.2.28) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
