## FileTerm 2.2.1

FileTerm 2.2.1 聚焦终端标签切换和文件面板交互稳定性，修复历史终端回放期间可能把终端控制响应误写入远端 Shell 的问题，并更新社区交流二维码。

### 2.2.1 更新重点

- **终端标签切换安全性**：在回放终端历史内容时识别并过滤 xterm 生成的 CSI、OSC 和 DCS 响应，避免来回切换标签页时把类似 `2RR0;276;0c`、`rgb:...` 的控制序列自动输入远端终端。
- **终端与文件面板稳定性**：延续文件面板显示/隐藏时终端彩色内容闪烁的修复，降低工作区切换时的视觉抖动。
- **社区交流信息**：更新 README 中的微信群二维码，方便加入最新交流渠道。
- **跨平台质量**：通过 renderer/shared 检查、Rust/Tauri 质量检查、协议 fixture、三平台 socket 生命周期和三平台 unsigned package smoke 验证。

### 本版本包含的主要 PR

- [PR #206](https://github.com/St0ff3l/fileterm/pull/206)：修复终端标签切换时历史回放触发 xterm 控制响应并泄漏为远端 Shell 输入的问题。
- [PR #204](https://github.com/St0ff3l/fileterm/pull/204)：修复文件面板切换时终端颜色内容闪烁，并更新社区交流二维码。

完整变更记录请查看 [v2.2.0 与 v2.2.1 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.0...v2.2.1)。

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.1

FileTerm 2.2.1 focuses on terminal tab switching and file-panel stability. It prevents terminal control responses generated during transcript hydration from being written back to the remote shell and refreshes the community QR code.

### Highlights

- **Safer terminal tab switching**: Detect and filter xterm-generated CSI, OSC, and DCS responses while replaying terminal history, preventing sequences such as `2RR0;276;0c` and `rgb:...` from being injected into a remote shell when switching tabs.
- **Terminal and file-panel stability**: Retain the fix for colored terminal content flashing when showing or hiding the file panel, reducing visual jitter during workspace transitions.
- **Community information**: Update the WeChat group QR code in the README so users can reach the latest community channel.
- **Cross-platform quality**: Validate renderer/shared checks, Rust/Tauri quality checks, protocol fixtures, socket lifecycle tests on all three platforms, and unsigned package smoke tests on all three platforms.

### Main PRs

- [PR #206](https://github.com/St0ff3l/fileterm/pull/206): Prevent xterm control responses generated during terminal transcript hydration from leaking into the remote shell as input when switching tabs.
- [PR #204](https://github.com/St0ff3l/fileterm/pull/204): Fix terminal color flashing when toggling the file panel and update the community QR code.

See the [comparison between v2.2.0 and v2.2.1](https://github.com/St0ff3l/fileterm/compare/v2.2.0...v2.2.1) for the complete change set.

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not include passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
