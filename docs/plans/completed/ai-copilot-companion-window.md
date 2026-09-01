# AI Copilot 同窗 UI 草案

状态：已完成

## 当前决策

AI Copilot 不创建 Tauri 原生子窗口，也不使用透明桌面间隙。它是主窗口内由 renderer 模拟的右侧栏：打开后压缩现有工作区，并与标题栏、工作区和传输任务栏形成统一的同窗布局。

## 布局

- AI 面板和终端/文件工作区都位于同一个 FileTerm 主窗口内；标题栏、终端区域和传输任务栏会同步为右侧栏预留宽度。
- AI 面板最终采用与左侧系统栏一致的整合式侧栏关系，只保留 1px 分隔线，不再使用外挂卡片、透明间隙、窄轨道或胶囊把手。
- 大多数桌面宽度下，工作区优先缩窄，AI 面板保持约 320–368px；窄窗口降为覆盖式面板，避免把终端挤到不可用。
- AI 侧栏复用主窗口右上角和右下角圆角，不额外增加完整外框或阴影。

## UI 范围

- 仅实现 AI 面板、未配置 Provider 空状态、禁用态输入框和设置里的 Provider 表单视觉。
- 设置表单仅保存于当前 renderer state，用于查看布局；不持久化 API Key、不发送网络请求、不接模型。
- 不读取/上传终端输出、不生成真实回答、不执行或写入终端命令。

## 后续实现边界

真实 Provider、凭据存储、上下文授权、命令提议与写入终端已转入 [AI Copilot 功能集成计划](ai-copilot-integration.md)，并通过 `core → Rust commands/events → tauri-api → renderer` 接入。

## 验收

- [x] 打开 AI 后不创建任何 Tauri 子窗口。
- [x] 同窗内右侧 AI 侧栏与主工作区无透明缝隙，仅保留稳定分隔线。
- [x] AI 设置页明确标记为界面预览，API Key 不会保存或发起请求。
- [x] TypeScript、Rust tests、Clippy、lint 和 formatter 全部通过。
