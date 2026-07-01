# 多平台系统信息与监控能力计划

## 背景

FileTerm 当前已经有 SSH 会话的系统信息页，但实现明显偏向 `Linux over SSH`：

- 采集脚本主要依赖 `/proc`、`df`、`ps`、`hostname`、`ifconfig`、`lspci`
- renderer 里已经承担了一部分格式化职责
- 旧快照或旧字段格式仍可能把中文运行时字符串带到英文界面

如果继续直接在现有脚本上堆分支，后续接入 NAS、软路由、Windows 时会越来越难维护。

## 目标

1. 让系统信息页的展示层完全本地化，不再依赖后端拼接中文或英文文案。
2. 让主进程采集链从“Linux-only 大脚本”逐步整理成“平台探测 + 原始指标归一化”。
3. 为标准 Linux、BusyBox/OpenWrt、常见 NAS、Windows SSH 目标机建立清晰的兼容路线。

## 非目标

- 这轮不做完整告警系统。
- 这轮不接入 Prometheus、SNMP、WMI 等外部监控协议。
- 这轮不承诺所有 NAS 发行版一次覆盖。

## 当前问题

### 展示层

- `uptime` 历史上直接由后端拼接成 `8 天` 这类字符串，导致英文界面残留中文。
- 英文语言包存在系统信息页 key 漏配时，会继承中文默认值。

### 采集层

- CPU/GPU/网络/文件系统采集都堆在 `session-file-utils.ts` 的单段 shell 中。
- 对 GNU 用户态假设较强，BusyBox/OpenWrt 环境可能缺少完整参数。
- 原生 Windows 主机目前没有兼容路径。

## 架构方向

### 统一原则

- `packages/core` 只承载原始指标与归一化结构。
- `main/services/sessions/*` 做平台探测、采集、回退、归一化。
- `workspace-session-runtime` 做轮询、合并、广播。
- `renderer/features/system/*` 只做展示和本地化。

### 推荐采集分层

1. platform probe
2. collector selection
3. raw command execution
4. normalized metrics parsing
5. runtime merge and history retention
6. localized rendering

## 平台分阶段规划

### Phase A: Linux 基线稳定

范围：

- Debian / Ubuntu / CentOS / Rocky / Alma / Arch 等标准 Linux
- 常见 x86 NAS Linux 发行版

交付：

- `uptime`、CPU、GPU、内存、swap、网卡、文件系统字段统一走原始值
- renderer 不再依赖中文 fallback
- CPU/GPU 采集回退链更稳

验收：

- Debian 与 Synology 已测机器显示一致
- 英文界面无中文系统信息残留

### Phase B: BusyBox / OpenWrt 兼容

范围：

- OpenWrt
- BusyBox 为主的软路由系统

交付：

- 为 `ps`、`df`、`ip/ifconfig`、`uptime` 增加 BusyBox 兼容分支
- 明确哪些卡片允许缺项但页面不崩

验收：

- 页面至少能稳定显示概览、CPU、内存、网卡、文件系统基础信息

### Phase C: Windows SSH 目标机评估

范围：

- OpenSSH Server + PowerShell 主机

交付：

- 增加 Windows 平台探测
- 新建 PowerShell collector，覆盖：
  - OS / kernel / host
  - CPU
  - memory / swap-like 指标
  - network adapters
  - filesystem volumes

验收：

- 在不破坏 Linux collector 的前提下可输出同一份 `SystemMetrics`

## 拆分建议

建议后续将 `session-file-utils.ts` 中的系统信息逻辑拆到：

- `system-metrics/platform-probe.ts`
- `system-metrics/linux-collector.ts`
- `system-metrics/busybox-collector.ts`
- `system-metrics/windows-collector.ts`
- `system-metrics/parse-system-metrics.ts`

## 风险

- 不同平台命令输出差异非常大，硬编码正则容易越修越脆。
- 单次 SSH exec 时间过长会影响连接手感，需要控制采样开销。
- GPU 在 NAS/VM 环境里常常只有“尽力识别”，不能承诺硬件管理级精度。

## 近期落地顺序

1. 先清理 renderer 本地化与旧字符串 fallback。
2. 再把 Linux collector 的字段改成“原始值优先”。
3. 补 BusyBox/OpenWrt 兼容分支。
4. 最后评估并接入 Windows collector。

## 进度记录

- 2026-05-20：建立 active plan，并把系统信息能力明确为“raw metrics -> localized renderer”的跨层边界。
- 2026-06-30：renderer 侧新增侧栏收起态资源摘要，CPU、内存、交换空间以细竖向监控柱展示；该变化只属于展示层，不改变采集链路。
