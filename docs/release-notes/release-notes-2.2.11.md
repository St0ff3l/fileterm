## FileTerm 2.2.11

FileTerm 2.2.11 修复 Linux 桌面包在 tray 初始化阶段的启动崩溃。

### 2.2.11 更新重点

- **Linux AppImage / deb 启动稳定性**：修复 tray 图标接收 16-bit RGBA 原始像素缓冲时的字节数不匹配，避免应用在启动 setup hook 中崩溃。
- **跨平台图标边界**：Linux tray 现在显式使用 8-bit RGBA 图标；macOS Template tray 图标和 Windows ICO 路径保持独立。
- **回归覆盖**：为 Linux bundle 图标增加位深与 RGBA 格式检查，防止资产重新导出后引入相同问题。

### 本版本包含的主要 PR 和问题修复

- [PR #258](https://github.com/St0ff3l/fileterm/pull/258)：修复 Linux AppImage / deb 的 tray 图标启动崩溃，并补充平台图标格式回归测试与维护文档。

完整变更记录请查看 [v2.2.10 与 v2.2.11 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.10...v2.2.11)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.11

FileTerm 2.2.11 fixes a Linux desktop-package startup crash during tray initialization.

### Highlights

- **Linux AppImage / deb startup stability**: Fix a tray-icon byte-count mismatch from 16-bit RGBA raw pixels, preventing an application crash in the startup setup hook.
- **Cross-platform icon boundaries**: Linux tray now explicitly uses an 8-bit RGBA icon, while the macOS Template tray icon and Windows ICO paths remain separate.
- **Regression coverage**: Add bit-depth and RGBA-format checks for Linux bundle icons to prevent the same issue when assets are re-exported.

### Main PRs and issues

- [PR #258](https://github.com/St0ff3l/fileterm/pull/258): Fix the Linux AppImage / deb tray-icon startup crash and add platform-icon format regression coverage and maintenance documentation.

See the [comparison between v2.2.10 and v2.2.11](https://github.com/St0ff3l/fileterm/compare/v2.2.10...v2.2.11) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
