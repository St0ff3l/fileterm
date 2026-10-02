## FileTerm 2.2.20

将本地工作区与监控界面的修复纳入正式版本，改善标签页状态保留、主题显示和设置导航。

### 2.2.20 更新重点

- **监控侧栏**：恢复侧栏布局，调整监控开关、停止与重连提示、资源列表滚动和磁盘数据显示；保留最新诊断日志。
- **主题显示**：隔离监控组件样式，修复亮色与自定义主题下的显示和控件对齐。
- **首页与设置**：已访问的首页标签切换后保留页面、设置区、草稿与滚动状态；跳转安全设置时清空搜索，确保目标页面可见。
- **传输兼容性**：兼容旧传输记录中的小数毫秒修改时间，避免读取记录失败。
- **开发版隔离**：本地开发启动为 FileTerm Dev，使用独立应用标识和数据目录，便于与安装版同时验证。

### 本版本包含的主要 PR 和问题修复

- 本次版本准备包含监控侧栏、主题、首页状态、安全设置导航和旧传输记录兼容修复。

完整变更记录请查看 [v2.2.19 与 v2.2.20 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.19...v2.2.20)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.20

Bring the local workspace and monitoring fixes into the release, improving tab state retention, theme rendering, and settings navigation.

### Highlights

- **Monitoring sidebar**: Restore the sidebar layout and refine monitoring controls, stopped and reconnecting states, resource scrolling, and filesystem rows while retaining diagnostic logging.
- **Themes**: Isolate monitoring styles and fix rendering and control alignment in light and custom themes.
- **Home and settings**: Keep visited home tabs mounted to retain their page, settings section, drafts, and scroll position. Clear settings search when navigating to security settings so the destination stays visible.
- **Transfer compatibility**: Accept fractional millisecond modification times in older transfer records to prevent loading failures.
- **Development isolation**: Start local development as FileTerm Dev with a separate application identity and data directory for side-by-side verification.

### Main PRs and issues

- This version preparation includes monitoring, theme, home state, security navigation, and legacy transfer record fixes.

See the [comparison between v2.2.19 and v2.2.20](https://github.com/St0ff3l/fileterm/compare/v2.2.19...v2.2.20) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
