# 多平台系统信息与监控能力计划 (Completed)

## 背景

FileTerm 当前已经有 SSH 会话的系统信息页，但实现明显偏向 `Linux over SSH`：

- 采集脚本主要依赖 `/proc`、`df`、`ps`、`hostname`、`ifconfig`、`lspci`
- renderer 里已经承担了一部分格式化职责
- 旧快照或旧字段格式仍可能把中文运行时字符串带到英文界面

如果继续直接在现有脚本上堆分支，后续接入 NAS、软路由、Windows 时会越来越难维护。

## 目标

1. 让系统信息页的展示层完全本地化，不再依赖后端拼接中文或英文文案（如 `天`、`分钟`）。
2. 让主进程采集链从“Linux-only 大脚本”逐步整理成“平台探测 + 原始指标归一化”。
3. 为标准 Linux、BusyBox/OpenWrt、常见 NAS、Windows SSH 目标机建立清晰的兼容路线。

## 待完成工作 (TODO)

### 1. 采集逻辑拆分与归一化

- [x] 从 `session-file-utils.ts` 中剥离系统监控采集脚本。
- [x] 新建 `system-metrics/` 目录并建立通用接口定义 `SystemMetrics`（存储原始指标数据，如字节数、秒数等）。
- [x] 移除脚本中直接拼装 `" 天"`、`"小时"` 等本地化字符串的代码，改为直接返回运行时长（秒数）。

### 2. 跨平台采集器实现

- [x] **Linux 采集器** (`linux-collector.ts`)：覆盖标准 Linux 发行版（Debian, Ubuntu, CentOS, Arch 等）。
- [x] **BusyBox / OpenWrt 兼容采集器** (`busybox-collector.ts`)：为软路由和 BusyBox 的 `df`、`uptime`、`ps` 等简化版指令提供兼容解析，允许缺项时不崩溃。
- [x] **Windows 采集器** (`windows-collector.ts`)：基于 PowerShell 脚本实现对 Windows SSH 目标主机的 CPU、内存、磁盘及网卡信息探测。

### 3. 主进程调度与路由

- [x] 实现平台自动探测器 (`platform-probe.ts`)：在会话建立后首先运行快速探测命令，判定远端主机 OS 类型。
- [x] 在 SSH 系统指标刷新链路中集成该路由；`workspace-session-runtime.ts` 继续负责轮询节流，SSH controller 根据探测结果调用对应的 Platform Collector。

---

## 已完成工作 (Completed)

- **展示层资源摘要栏**：Renderer 侧已支持在侧边栏收起时显示 CPU、内存、Swap 资源摘要（采用精美的细柱形实时指标图展示）。
- **展示层本地化边界**：已将监控页面的基础字段和键值抽离到 i18n 语言包中，为全本地化渲染做好了铺垫。
- **平台采集器拆分**：主进程已新增 `sessions/system-metrics/`，包含 `platform-probe.ts`、Linux/BusyBox/Windows collector 和 parser，输出 `uptimeSeconds` 与内存/网络等 raw bytes 字段。

---

## 进度记录

- 2026-05-20：建立 active plan，并把系统信息能力明确为“raw metrics -> localized renderer”的跨层边界。
- 2026-06-30：renderer 侧新增侧栏收起态资源摘要，CPU、内存、交换空间以细竖向监控柱展示；该变化只属于展示层，不改变采集链路。
- 2026-07-09：整理 active plan，剥离已完成部分，聚焦于主进程平台采集逻辑的拆分工作。
- 2026-07-09：完成主进程系统监控采集拆分；`session-file-utils.ts` 回归文件工具职责，SSH metrics 通过平台 probe 路由到 Linux、BusyBox/OpenWrt 或 Windows PowerShell collector。
- 2026-07-10：Windows 兼容性加固——parser.ts 入口统一 `replace(/\r\n?/g, '\n')` 归一化 CRLF 污染；windows-collector 新增多级 fallback（CIM 2s → WMI Job 2s → WMIC 进程 2s → .NET API → cmd 工具）与完整性标记 `__FILETERM_METRICS_COMPLETE__` 防半截输出；ssh-session-controller 新增 `supportsPosixShellSetup()` fail-closed 双重门控，Windows/unknown 平台不再注入 POSIX CWD 脚本。新增 platform-probe / windows-collector / parser CRLF 回归测试。
- 2026-07-10：完成采集稳定性收尾——平台探测与指标刷新 single-flight，POSIX 高风险命令增加硬超时与完整性标记，Windows 采集增加整体预算和 shell fallback 约束；首次指标不再等待 SFTP 初始化。Debian 实机验证平台识别、指标、CWD 与提示符均正常。
