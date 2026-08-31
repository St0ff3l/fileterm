# MCP / CLI interactive-exec 后续迁移

状态：已废弃（2026-08-14，方案被产品边界取代）

本计划原本设计一个任务专属临时 PTY，用于让 MCP/CLI 代替用户处理 MFA、验证码、确认和 REPL。经过实现评估，FileTerm 最终只保留两套通道：

- 可见 SSH tab PTY：用户直接操作，承载所有通用交互输入；
- 普通后台 exec：独立 SSH exec channel，不写 terminal transcript，遇到通用输入返回 `REMOTE_INTERACTIVE_INPUT_REQUIRED`。

因此不再实现或保留以下入口：

- `fileterm_execute_interactive_remote_command`；
- `fileterm interactive-exec`；
- `app_execute_interactive_remote_command`、输入 resolve/renderer-ready IPC；
- 临时 interactive PTY、`interactive_exec_audit` 和对应 renderer 弹窗。

## 替代方案

sudo/su 不需要 generic interactive-exec，统一使用普通 exec 的三层凭据入口：

1. 用户明确提供的一次性 `sudo_password` / `su_password`；
2. profile 加密存储；
3. FileTerm 主窗口安全输入，可选保存到连接管理器。

内置 Copilot 在主窗口/renderer 不可用时停止本轮并等待用户明确重试，不接收或转发一次性密码字段；MCP/CLI 才会返回 `SUDO_PASSWORD_NEEDED` / `SU_PASSWORD_NEEDED`，再由外部 Agent 询问用户并用一次性字段重试。主窗口可用时 FileTerm 会自动恢复、聚焦并展示安全输入，Copilot 对话区和工具活动显示等待前台输入。用户取消或超时返回对应的 `*_PASSWORD_CANCELLED`，不自动重试。MFA、验证码、确认和 REPL 则由用户在可见 SSH tab 完成。

## 关闭记录

代码、MCP/CLI schema、CI package smoke、架构文档和质量清单已同步到上述双通道边界。该文件完成后移入 `docs/plans/completed/`，仅保留决策记录，不再作为待办计划。
