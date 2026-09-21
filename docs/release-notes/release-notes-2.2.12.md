## FileTerm 2.2.12

本版本改进远程系统资源监控中的进程排名与 Windows 本机 CLI/MCP 命令生成，帮助用户更准确地定位实际占用，并减少跨平台命令复制后的执行问题。

### 2.2.12 更新重点

- **进程排名**：CPU、内存和命令排序分别从全部可见进程中选取各自的前 40 条，不再把某一种排序的候选集误用于其他排序。
- **CPU 口径与采集稳定性**：进程 CPU 继续使用整机占比，与上方 CPU 仪表保持同一口径；修复脚本解释器进程被名称误过滤、PID 复用和进程采样期间退出造成的漏报风险。
- **Windows CLI/MCP**：Windows PowerShell 的直接 FileTerm CLI 命令使用 `&` 调用带空格路径的可执行文件；MCP 注册命令仍将可执行路径作为参数，不添加 `&`。macOS/Linux 命令格式保持不变。

### 本版本包含的主要 PR 和问题修复

- [PR #260](https://github.com/St0ff3l/fileterm/pull/260)：修复进程全局排序、整机 CPU 口径与跨平台进程采集，并完善 Windows CLI/MCP 命令生成。
- [Issue #259](https://github.com/St0ff3l/fileterm/issues/259)：针对 CPU 排序下高负载进程未被准确展示的问题完善采集与排名逻辑。

完整变更记录请查看 [v2.2.11 与 v2.2.12 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.11...v2.2.12)。

### 反馈与支持

> &bull;&nbsp;遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。
> &bull;&nbsp;也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

---

## FileTerm 2.2.12

This release improves remote process ranking and Windows local CLI/MCP command generation, making resource investigation more accurate and cross-platform command copying more reliable.

### Highlights

- **Process ranking**: CPU, memory, and command views each select their own global top 40 visible processes instead of reusing one sort's candidate set for another.
- **CPU scope and collection stability**: Per-process CPU remains a whole-machine percentage on the same scale as the CPU gauge above; script-interpreter workloads, PID reuse, and processes exiting during sampling are handled more reliably.
- **Windows CLI/MCP**: Direct FileTerm CLI commands in Windows PowerShell use `&` for executable paths containing spaces, while MCP registration keeps the executable path as an argument without `&`. macOS/Linux command formats are unchanged.

### Main PRs and issues

- [PR #260](https://github.com/St0ff3l/fileterm/pull/260): Correct global process ranking, whole-machine CPU scope, cross-platform collection, and Windows CLI/MCP command generation.
- [Issue #259](https://github.com/St0ff3l/fileterm/issues/259): Improve visibility of high-load processes when observing CPU usage.

See the [comparison between v2.2.11 and v2.2.12](https://github.com/St0ff3l/fileterm/compare/v2.2.11...v2.2.12) for the complete change set.

### Feedback & Support

> &bull;&nbsp;For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with the operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.
> &bull;&nbsp;Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
