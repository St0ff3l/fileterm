# 简化远程 exec 与 sudo/su 凭据自动化

状态：已完成（2026-08-14）

## 最终边界

FileTerm 只保留两类执行通道：

```text
可见 SSH tab PTY       用户直接操作，承载登录、MFA、验证码、确认和 REPL
普通后台 exec channel   独立 SSH exec，返回有限结果，不写可见 terminal transcript
```

普通 `exec` 不再拥有 generic interactive fallback。检测到需要通用输入时，返回稳定错误码 `REMOTE_INTERACTIVE_INPUT_REQUIRED` 和有限的 `inputKind` 提示；不会创建临时 PTY、不会注入可见终端、不会等待用户输入。

## sudo/su 三层凭据来源

普通 exec 执行以 `sudo` 或 `su` 开头的命令时按以下顺序取值：

1. 用户明确提供的一次性参数 `sudo_password` / `su_password`；
2. profile 中的加密 `ftsec:v1:` secret；
3. FileTerm 主窗口安全输入，可选“保存到连接管理器”；缺少凭据时自动恢复、解除最小化并聚焦主窗口后展示。

Copilot 对话区和工具活动会显示等待前台输入，工具回合保持等待，直到用户完成安全输入；主窗口/renderer 不可用时返回 `SUDO_PASSWORD_NEEDED` / `SU_PASSWORD_NEEDED`，用户取消或超时返回对应的 `*_PASSWORD_CANCELLED`。Copilot 和 MCP 可以在 `*_PASSWORD_NEEDED` 后询问 sudo/su 密码，并把用户明确提供的值作为单次字段重试；取消结果不自动重试。`save_*` 只有和显式密码同时出现才生效。密码通过 SSH stdin 发送，不进入命令文本、结果或日志。

通用 MFA、验证码、安装器提示、确认回答和 REPL 不属于 sudo/su 三层凭据，必须在可见 SSH tab 完成。

## 已删除的旧链路

- `interactive-exec` CLI 子命令；
- `fileterm_execute_interactive_remote_command` MCP tool；
- `app_execute_interactive_remote_command`、交互输入 resolve/renderer-ready IPC；
- `InteractiveRemoteExec*` 类型、worker command、临时 PTY task、audit JSONL 和安全输入弹窗；
- CI/package smoke 中的 `interactive-exec --help` 检查。

保留 `SshInteraction` 只用于连接自身的 host-key、登录和 SSH keyboard-interactive 流程；保留 sudo/su 的主窗口安全 prompt，不把两者重新合并成 generic exec 输入通道。

## 验收

- [x] profile secret 读取、一次性参数和主窗口 sudo/su prompt。
- [x] 普通 exec 独立 channel、有限输出、超时/截断和输入提示。
- [x] generic interactive-exec 的 Rust、bridge、renderer、MCP、CLI、audit 和 CI 入口删除。
- [x] `SUDO_PASSWORD_NEEDED`、`SU_PASSWORD_NEEDED`、`REMOTE_INTERACTIVE_INPUT_REQUIRED` 稳定错误契约。
- [x] Copilot/MCP tool schema 支持用户明确提供的 sudo/su 一次性字段和可选保存。
- [x] Rust check/test compile、Tauri typecheck、MCP schema smoke 同步。

真实三平台打包和远端生产环境回归仍归入 `docs/quality/ai-copilot-platform-regression.md`，不再重新引入 interactive-exec 作为兼容前提。
