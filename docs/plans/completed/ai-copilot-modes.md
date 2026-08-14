# AI Copilot 三模式与上下文级别简化计划

状态：已完成（2026-08-14）

关联：[AI Copilot 功能集成计划](./ai-copilot-integration.md)、[简化远程 exec 与 sudo 凭据自动化](./simplify-exec-sudo-credentials.md)、[架构地图](../../architecture.md)

## 模式矩阵

| 模式   | 上下文                 | 工具调用 | 执行语义               |
| ------ | ---------------------- | -------- | ---------------------- |
| 纯对话 | 默认 L0，可按次附加 L2 | 不允许   | 只生成普通回答         |
| 半自动 | 强制 L2                | 允许     | 每次工具调用逐次审批   |
| 全自动 | 强制 L2                | 允许     | 通过本地护栏后直接执行 |

三模式选择器直接替换旧的“普通对话 / 生成命令卡”选择。全自动不显示 `0/20`，也不维护会话累计次数、风险次数或累计时长；Rust 侧只保留单轮工具循环的防死循环边界、目标绑定和危险命令限制。

全自动的“危险命令限制”默认开启，可由用户关闭。开启时由 Rust 护栏拦截 `rm -rf /`、`mkfs`、`reboot` 等危险命令以及未获允许的破坏性/提权命令；关闭时只跳过这组命令限制，空命令、目标 session revision 和 channel 边界仍不可关闭。

## 上下文与执行

- L0 不读取主机、路径和 transcript。
- L2 由 Rust 从当前 SSH runtime 生成一次性快照，绑定 leaf/root、CWD、用户、Provider、窗口和 session revision。
- 半自动/全自动通过统一 Copilot tool activity 展示 proposal、状态、结果、退出码、截断和原因。
- 普通 exec 使用独立 SSH exec channel，永不写入可见 PTY transcript。
- generic interactive-exec 已删除。MFA、验证码、确认提示和 REPL 返回 `REMOTE_INTERACTIVE_INPUT_REQUIRED`，用户在可见 SSH tab 完成。

## 凭据策略

sudo/su 的入口统一为：

1. 用户或受信调用方明确给出的单次 `sudo_password` / `su_password`；
2. profile 加密存储；
3. FileTerm 主窗口安全输入，可选保存到连接管理器；
4. 如果 Agent 没有可用值，可以向用户询问 sudo/su 密码，随后以一次性字段重试。

半自动在主窗口可见且未最小化时使用本地安全输入；全自动不等待本地弹窗，缺少凭据直接把稳定错误交回 Agent 询问用户。主窗口隐藏、最小化或 renderer 不可用时，所有允许聊天回退的调用都走同一稳定错误路径。

密码不写入 command 文本或 tool result。通用 MFA/验证码/REPL 不通过聊天值回填普通 exec。

## Legacy 收口

- 旧 `responseMode=command-proposal` 不再由 renderer 发送。
- `app_run_ai_review`、`app_insert_ai_command`、旧 Review/命令卡动作和 generic interactive-exec API 已删除。
- 旧会话的 `commands` / `review` 字段仅由 Rust 读取器转换到 `AiToolActivity`，迁移后以当前 schema 写回。
- `ActionApprovalSource` 只保留 `mcp` 和 `ai-copilot`。

## 完成项

- [x] L0/L2 与三模式状态、模式确认和危险命令限制开关。
- [x] 三类 Provider 的工具 schema、SSE tool-call 重组、Rust-owned tool loop 和结果回传。
- [x] 半自动逐次审批、全自动目标绑定与护栏。
- [x] 统一工具活动 UI，移除旧命令卡/Review renderer 入口。
- [x] 普通 exec 双通道边界与 `REMOTE_INTERACTIVE_INPUT_REQUIRED`。
- [x] sudo/su 一次性参数、加密 profile、主窗口安全输入和可选保存。
- [x] 历史迁移、架构/MCP/CLI/CI/质量文档同步。

跨平台桌面、真实 Provider 和远端生产环境的手工回归继续记录在质量清单，不再作为旧 Copilot 链路兼容层的前置条件。
