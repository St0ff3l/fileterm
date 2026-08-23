## FileTerm 2.2.3

FileTerm 2.2.3 聚焦 Windows 远程系统监控准确性与 macOS 打包版本机 Agent CLI 识别稳定性。

### 2.2.3 更新重点

- **Windows 系统监控**：修正 CPU 使用率采样，避免与 Windows 任务管理器显示明显偏差；改进 NVIDIA GPU 使用率、显存、温度和功耗采集，并优先使用运行时显存总量，避免 8 GB 显卡被 WDDM 信息截断为 4 GB。
- **跨平台显示稳定性**：移除无法可靠获取的 CPU 温度指标；固定进程列表行高，避免中文命令导致行高被撑开。
- **本机 Agent CLI 识别**：支持 DMG/Finder 启动时从常见用户安装目录发现 Claude Code；排除 ChatGPT 桌面端内置的 `codex` 二进制，避免把 Codex 桌面端误报为 Codex CLI。
- **验证**：补充 Windows GPU 单位、运行时显存优先级、CPU 采样和桌面端内置 Codex 排除测试。

### 本版本包含的主要 PR 和问题修复

- [PR #210](https://github.com/St0ff3l/fileterm/pull/210)：Windows 远程系统监控、GPU 显存识别和本机 Agent CLI 检测修复。

完整变更记录请查看 [v2.2.2 与 v2.2.3 的比较](https://github.com/St0ff3l/fileterm/compare/v2.2.2...v2.2.3)。

### 反馈与支持

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 加入社区。

---

## FileTerm 2.2.3

FileTerm 2.2.3 focuses on accurate Windows remote system metrics and reliable local Agent CLI detection in packaged macOS builds.

### Highlights

- **Windows system monitoring**: Corrected CPU utilization sampling to avoid large differences from Windows Task Manager; improved NVIDIA GPU utilization, VRAM, temperature, and power collection, preferring runtime VRAM totals so an 8 GB GPU is not truncated to 4 GB by WDDM metadata.
- **Cross-platform display stability**: Removed the CPU temperature metric because it cannot be collected reliably across the supported platforms; fixed process-list row heights so Chinese commands do not expand the layout.
- **Local Agent CLI detection**: Packaged DMG/Finder launches can discover Claude Code from common user install directories; ChatGPT Desktop's bundled `codex` binary is excluded so the desktop app is not reported as Codex CLI.
- **Validation**: Added coverage for Windows GPU units, runtime VRAM precedence, CPU sampling, and exclusion of desktop-bundled Codex.

### Main PRs and issues

- [PR #210](https://github.com/St0ff3l/fileterm/pull/210): Windows remote system metrics, GPU VRAM detection, and local Agent CLI detection fixes.

See the [comparison between v2.2.2 and v2.2.3](https://github.com/St0ff3l/fileterm/compare/v2.2.2...v2.2.3) for the complete change set.

### Feedback & Support

For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your operating system, FileTerm version, connection type, reproduction steps, and redacted logs. Do not submit passwords, private keys, or tokens.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81).
