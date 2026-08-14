# AI Copilot 功能集成计划

状态：已完成（2026-08-14）

## 完成结论

FileTerm Copilot 已收敛为统一的 `tool-call → 审批/护栏 → 独立 exec → tool-result` 链路。新回合只使用三种运行模式：纯对话、半自动、全自动；旧的普通对话/命令卡切换不再是运行入口。

本次迁移已完成：

- 删除 `app_run_ai_review`、`app_insert_ai_command` 及对应 bridge、core API、renderer 兼容入口。
- `AiMessage` 只保留 user/assistant 文本和 `toolActivities`；工具提案与结果统一为 `AiToolActivity`。
- 读取历史时兼容旧 `commands` / `review` 字段，转换为统一工具活动并原子写回；后续 renderer、Provider history 和新存储不再产生旧结构。
- 删除旧 Review/命令卡的执行、写入终端和审核路由；历史迁移只负责保留审计事实，不复活旧动作。
- 三类 Provider 共用 Rust-owned 工具循环、目标绑定、逐次审批和全自动护栏；工具结果以内联活动显示。

## 运行边界

```text
可见 SSH tab PTY  ── 用户直接操作、登录/MFA/验证码/REPL
普通 background exec ── 独立 SSH exec channel、有限输出、不写 terminal transcript
```

普通 exec 永不接管可见 PTY，也不自动切换到另一个交互通道。检测到通用输入提示时返回稳定错误 `REMOTE_INTERACTIVE_INPUT_REQUIRED`，由用户在可见 SSH tab 完成操作后重试。

sudo/su 是普通 exec 的受控特例，凭据按以下顺序解析：

1. 调用方明确提供的一次性 `sudo_password` / `su_password`；
2. 连接 profile 的加密 secret；
3. FileTerm 主窗口安全输入，可选择保存到连接管理器；只有主窗口可见且未最小化时才展示。

主窗口隐藏、最小化或 renderer 不可用时，不发送用户看不到的弹窗，而是返回 `SUDO_PASSWORD_NEEDED` 或 `SU_PASSWORD_NEEDED`；Copilot/MCP 可以向用户询问密码，并把用户明确提供的值作为一次性字段重试。密码不进入命令文本或工具结果。MFA、验证码、确认提示、安装器和 REPL 不走聊天输入，仍回到可见 SSH tab。

## 主要代码边界

- `packages/core`：`AiToolCallProposal`、`AiToolCallResult`、`AiToolActivity` 和统一 stream event。
- `services/ai.rs`：Provider 适配、旧历史迁移、工具循环和 tool activity 持久化。
- `services/action_review.rs`：普通 exec、sudo/su 凭据解析、目标 revision 校验和稳定交互输入结果码。
- `sessions/ssh.rs`：可见 shell PTY 与独立 exec channel；不再包含临时 interactive-exec worker。
- `services/mcp.rs`：只保留普通 exec MCP/CLI 路由；generic interactive-exec 工具已删除。
- renderer/bridge：只保留 Copilot 工具活动与 sudo/su 本地安全输入，不再注册 generic remote-exec 输入弹窗。

## 迁移规则

历史文件中的旧角色和字段只在 Rust 读取器中存在：

- `commands` 转换为 `AiToolCallProposal`；
- `review` 转换为 `AiToolCallResult` 并挂到匹配 proposal；
- 缺少 proposal 的 review 记录生成独立、只读的迁移活动；
- 转换失败或目标不完整时保留失败/目标变化状态，不执行远程操作；
- 迁移写回使用当前 schema，避免 renderer 或 Provider 再看到旧 envelope。

## 验收

- Rust `cargo check --locked --all-targets --all-features` 通过。
- Rust `cargo test --locked --no-run` 通过，覆盖历史迁移、Provider tool schema、工具结果和普通 exec 输入提示。
- Tauri typecheck 通过；CI/package smoke 只检查 `exec`、`wait-transfer`、`mcp`，不再检查 interactive-exec。
- 架构、MCP/CLI 和质量文档已同步；本计划完成后移入 `docs/plans/completed/`。

跨平台真实 Provider、打包和远端生产环境手工回归仍由 `docs/quality/ai-copilot-platform-regression.md` 跟踪，不再阻塞旧链路迁移计划的关闭。
