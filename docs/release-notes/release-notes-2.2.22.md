## FileTerm 2.2.22

修复连接后监控侧栏无法展开及终端右键菜单触发 React 崩溃的问题，并加固相关连接表单同步。

### 2.2.22 更新重点

- **监控兼容性**：识别 Armbian 及其他 Linux 派生发行版；监控不可用时仍可手动展开侧栏查看连接摘要。
- **终端稳定性**：修复连接后立即打开终端右键菜单可能导致 React 更新循环的问题，并加固菜单定位和焦点处理。
- **连接表单**：修复选择已保存代理或 SSH 隧道时配置同步反复触发更新的问题。
- **浮层定位**：加固下拉菜单和命令发送目标选择器的定位更新，减少不必要的界面刷新。

### 本版本包含的主要 PR 和问题修复

- 修复监控侧栏兼容性、终端右键菜单 React 更新循环，以及连接表单和浮层的重复更新。

完整变更记录请查看 [v2.2.21 与 v2.2.22 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.21...v2.2.22)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.22

Fixes the monitoring sidebar failing to expand and a React crash when opening the terminal context menu after connecting, and hardens related connection form synchronization.

### Highlights

- **Monitoring compatibility**: Recognize Armbian and other Linux derivatives. Keep the sidebar available for connection details when monitoring is unavailable.
- **Terminal stability**: Fix a React update loop that could crash the app when opening the terminal context menu just after connecting, and improve menu positioning and focus handling.
- **Connection forms**: Fix repeated updates when synchronizing a selected saved proxy or SSH tunnel.
- **Popup positioning**: Harden positioning updates for dropdowns and the command send target picker to reduce unnecessary UI refreshes.

### Main PRs and issues

- Fix monitoring sidebar compatibility, the terminal context menu React update loop, and repeated updates in connection forms and popups.

See the [comparison between v2.2.21 and v2.2.22](https://github.com/St0ff3l/fileterm/compare/v2.2.21...v2.2.22) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
