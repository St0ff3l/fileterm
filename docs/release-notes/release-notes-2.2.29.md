## FileTerm 2.2.29

FileTerm 2.2.29 improves automatic monitoring updates and terminal output scheduling, and smooths terminal line-drawing glyphs.

**安装指南**

[![简体中文安装指南](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-%E5%AE%89%E8%A3%85%E6%8C%87%E5%8D%97-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/zh-CN.md)

### 2.2.29 更新重点

- **系统监控侧栏**：指标更新直接进入 React 状态，不再等待低优先级过渡任务；更新不依赖键盘或鼠标操作。增加事件接收、更新调度、侧栏提交 revision 和样本年龄日志。
- **终端输出与绘制**：空闲终端立即提交首批输出，后续按 xterm 解析回调分批排空，不依赖动画帧继续消费积压。线框横线按终端单元格几何绘制，修复字符宽度与相邻线段接缝问题；保留特殊字符适配、ANSI 样式、选区和中日韩字符处理。
- **流量图与卡顿诊断**：初始基线连接到第一个真实网络样本。可见前端主线程卡顿恢复后记录 `event-loop-gap`；诊断只写运行状态和指标元数据，不记录终端文本。
- **验证范围**：合成压力与浏览器回归通过；真实 CC 端到端延迟对照、长时间原生会话和 Windows 原生回归仍待验证，因此本版本不宣称已确认解决所有长期无响应问题。`event-loop-gap` 也只能在前端恢复后写入。

### 本版本包含的主要 PR 和问题修复

- 监控和终端输出调度改进，并补充侧栏提交及前端事件循环诊断。
- 横线、特殊字符和网络图首个样本的连续绘制修复。
- 与 [Issue #259](https://github.com/St0ff3l/fileterm/issues/259) 相关：增加 CPU/监控数据显示链路日志，帮助后续确认侧栏停更位置；未将该问题标记为已解决。

完整变更记录请查看 [v2.2.28 与 v2.2.29 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.28...v2.2.29)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.29

FileTerm 2.2.29 improves automatic monitoring updates and terminal output scheduling, and smooths terminal line-drawing glyphs.

**Installation guides**

[![English Installation Guide](https://img.shields.io/badge/English-Installation_Guide-0969DA?style=for-the-badge&labelColor=555)](https://github.com/St0ff3l/fileterm/blob/main/docs/installation/en-US.md)

### Highlights

- **System monitoring sidebar**: Metric updates now enter React state without waiting behind low-priority transitions, and continue without keyboard or mouse activity. Logs record event receipt, scheduled updates, rendered revisions, and sample age.
- **Terminal output and drawing**: An idle terminal submits its first output immediately, then drains bounded chunks from xterm parse callbacks without relying on animation frames. Box-drawing rules use terminal-cell geometry to fix glyph-width and line-join seams while preserving symbol fitting, ANSI styling, selection, and CJK handling.
- **Network graph and stall diagnostics**: The initial baseline now joins the first real network sample. A visible renderer event-loop stall is logged as `event-loop-gap` after the main thread recovers. Diagnostics contain runtime and metric metadata, not terminal text.
- **Validation scope**: Synthetic load and browser regressions passed. An end-to-end CC latency comparison, long-running native sessions, and native Windows regression remain unverified, so this release does not claim to have confirmed a fix for every long-running freeze. `event-loop-gap` can only be written after the renderer recovers.

### Main PRs and issues

- Monitoring and terminal output scheduling improvements, with sidebar commit and renderer event-loop diagnostics.
- Continuous rendering fixes for horizontal rules, special characters, and the first network graph sample.
- Related to [Issue #259](https://github.com/St0ff3l/fileterm/issues/259): adds CPU/monitoring update-path logs to help locate a future sidebar stall; the issue is not marked resolved.

See the [comparison between v2.2.28 and v2.2.29](https://github.com/St0ff3l/fileterm/compare/v2.2.28...v2.2.29) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
