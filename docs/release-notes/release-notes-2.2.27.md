## FileTerm 2.2.27

FileTerm 2.2.27 improves desktop window startup and the monitoring guidance shown after an unexpected SSH disconnect.

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.2.27 更新重点

- **窗口启动与标题栏**：窗口在初始尺寸和原生标题栏布局就绪后再显示，减少启动时的半透明或空白闪现，以及 macOS 红绿灯按钮从左上角跳到目标位置的过程；调整大小或退出全屏时也会重新校准位置。Windows 和 Linux 同样会等主界面首帧绘制完成后再显示窗口。
- **SSH 断开后的监控提示**：意外断开时使用中性状态提示，并移除会额外重连整条 SSH 会话的宽按钮；SSH 恢复后监控会自动恢复，如仍需启动，可点击右上角电源按钮。

### 本版本包含的主要 PR 和问题修复

- [PR #301](https://github.com/St0ff3l/fileterm/pull/301)：等待窗口和原生标题栏布局初始化完成后再显示主窗口，减少启动闪现和 macOS 红绿灯按钮位置跳动。
- [PR #302](https://github.com/St0ff3l/fileterm/pull/302)：更新 SSH 意外断开后的监控提示，移除重复重连入口并说明自动恢复行为。

完整变更记录请查看 [v2.2.26 与 v2.2.27 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.26...v2.2.27)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.27

FileTerm 2.2.27 improves desktop window startup and monitoring guidance after unexpected SSH disconnects.

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **Window startup and title bar**: Windows appear after their initial size and native title bar layout are ready, reducing translucent or blank startup flashes and the macOS traffic lights jumping from the upper-left corner to their target position. Their layout is recalibrated after resizing and exiting fullscreen. Windows and Linux also wait for the first main-view frame before showing the window.
- **Monitoring after SSH disconnects**: Unexpected disconnects use a neutral status message and no longer show a wide button that starts a duplicate reconnect of the whole SSH session. Monitoring resumes automatically when SSH reconnects; if needed, start it with the power button in the upper-right corner.

### Main PRs and issues

- [PR #301](https://github.com/St0ff3l/fileterm/pull/301): Delays showing the main window until its native title bar layout is initialized, reducing startup flashes and macOS traffic-light movement.
- [PR #302](https://github.com/St0ff3l/fileterm/pull/302): Updates the monitoring notice for unexpected SSH disconnects, removes the duplicate reconnect action, and explains automatic recovery.

See the [comparison between v2.2.26 and v2.2.27](https://github.com/St0ff3l/fileterm/compare/v2.2.26...v2.2.27) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
