## FileTerm 2.2.10

FileTerm 2.2.10 聚焦终端 CJK 字符排版稳定性，以及字体加载后的列宽重新测量。

### 2.2.10 更新重点

- **终端 CJK 排版**：修复 Chromium/xterm DOM 渲染中中文标点被压缩或扩展导致的列宽漂移，保持等宽终端网格与远端文本对齐。
- **字体加载稳定性**：选定字体或字体异步加载完成后，自动触发终端网格重新测量；字体加载失败和组件销毁路径也会正确清理监听器。
- **回归覆盖**：新增多字体、多字号和不同设备像素比的终端排版回归，以及字体异步加载生命周期测试。

### 本版本包含的主要 PR 和问题修复

- [PR #255](https://github.com/St0ff3l/fileterm/pull/255)：修复终端中文标点符号宽度与字体加载后的列宽测量问题，并补充浏览器回归测试。
- [Issue #254](https://github.com/St0ff3l/fileterm/issues/254)：跟踪终端字体中文标点符号宽度异常。

完整变更记录请查看 [v2.2.9 与 v2.2.10 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.9...v2.2.10)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.10

FileTerm 2.2.10 focuses on stable CJK terminal typography and remeasuring terminal columns after fonts finish loading.

### Highlights

- **CJK terminal typography**: Fix column-width drift caused by compressed or expanded Chinese punctuation in Chromium/xterm DOM rendering, keeping the terminal grid aligned with remote text.
- **Font-loading stability**: Remeasure the terminal grid after a selected font or an asynchronously loaded font becomes available, while also cleaning up listeners on font-loading errors and component disposal.
- **Regression coverage**: Add terminal typography regressions across multiple fonts, sizes, and device pixel ratios, together with an asynchronous font-loading lifecycle test.

### Main PRs and issues

- [PR #255](https://github.com/St0ff3l/fileterm/pull/255): Fix Chinese punctuation widths and post-load terminal column measurement, with browser regression coverage.
- [Issue #254](https://github.com/St0ff3l/fileterm/issues/254): Track the terminal font Chinese punctuation width issue.

See the [comparison between v2.2.9 and v2.2.10](https://github.com/St0ff3l/fileterm/compare/v2.2.9...v2.2.10) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
