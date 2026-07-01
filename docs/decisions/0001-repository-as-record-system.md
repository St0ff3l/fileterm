# ADR-0001: 代码仓库作为记录系统

## 状态

Accepted

## 背景

FileTerm 的架构涉及 Electron `main / preload / renderer`、协议接入、工作区状态、文件传输、主题系统和多个 packages。如果把所有信息都写进一个巨大的 `AGENTS.md`，会挤占上下文、降低可维护性，并让智能体难以判断哪些规则仍然有效。

## 决策

将代码仓库本身作为记录系统：

- `AGENTS.md` 只作为入口地图和硬性约束，保持短小。
- `docs/architecture.md` 记录稳定架构事实和当前实现状态。
- `docs/roadmap.md` 记录阶段目标。
- `docs/plans/active/` 记录正在推进的跨层执行计划。
- `docs/plans/completed/` 归档已完成计划。
- `docs/decisions/` 记录架构决策。
- `.agents/extensions/` 保留为协作草案和扩展设计区，不放生产运行代码。

## 影响

- 智能体从小入口开始，再按任务需要渐进式读取 deeper docs。
- 跨轮协作不依赖外部上下文，计划和决策随代码一起版本化。
- 文档需要随架构和行为变化同步维护，避免 `docs/` 变成陈旧规则来源。
