# AI Copilot 跨平台发行验收

本清单记录 Copilot、普通后台 exec 和 MCP/CLI 在打包应用中的发行前验证。旧的 command-proposal、Review 和 generic interactive-exec 链路已关闭，不再作为验收项。

## 自动化证据

- Rust：`cargo check --locked --all-targets --all-features`、`cargo test --locked --no-run`，覆盖三模式、Provider tool schema、历史迁移、action approval、sudo/su 凭据和普通 exec 输入提示。
- Renderer：`npm run typecheck -w @fileterm/tauri`；CI 继续执行 lint、Prettier、Rust tests、生产构建和协议夹具。
- MCP stdio smoke：验证 `initialize`、`tools/list`、`fileterm_execute_remote_command` schema、sudo/su 一次性字段和 `REMOTE_INTERACTIVE_INPUT_REQUIRED` 描述；不再查找 interactive-exec。
- Package smoke：macOS、Windows、Linux 只运行 `mcp --help`、`exec`/`wait-transfer` 相关 CLI 检查和 MCP stdio smoke。

## Loopback Provider fixture

```bash
npm run qa:ai-copilot-fixture-smoke
```

fixture 覆盖普通 SSE、停止/重试、Markdown、tool-call、tool-result 和 sudo tool schema。它只监听 loopback、只记录请求模式和长度，不记录 prompt、API key 或远程输出。

## 每个平台的手工清单

- [ ] Provider 保存后重新打开只显示 `hasApiKey`，不回填 Key；默认、禁用和删除状态正确。
- [ ] 纯对话默认不附带 transcript；打开参考终端后只发送当前一次性 L2 快照。
- [ ] 半自动每个 tool call 都出现一次审批；拒绝、关闭、超时不执行。
- [ ] 全自动不出现 `0/20` 或累计次数栏；危险命令限制开关默认开启，关闭后仍保留目标 revision 和空命令校验。
- [ ] Copilot 工具活动显示 proposal、执行状态、退出码、截断标记和原因；不显示旧命令卡或 Review 操作。
- [ ] 普通 exec 的输出不混入可见 SSH tab transcript，也不改变用户正在看的 PTY。
- [ ] sudo/su 分别验证 profile 加密值、可见且未最小化时的主窗口安全输入、隐藏/最小化时的聊天回退、用户明确的一次性字段和可选保存；密码不出现在命令文本或 tool result。
- [ ] MFA、验证码、确认、安装器和 REPL 命令返回 `REMOTE_INTERACTIVE_INPUT_REQUIRED`；不创建临时 interactive PTY，不要求用户把输入粘贴到错误的终端。
- [ ] 切换 tab、分屏、CWD、登录用户、重连或关闭窗口时，旧 tool call 因目标 revision 变化而 fail closed。
- [ ] 代理、断网、睡眠恢复、窗口关闭和 Provider 重试不会留下流式请求或审批状态。
- [ ] MCP/CLI 可初始化并读取已打开会话；修改/执行/传输/隧道操作按 MCP 审批边界工作，CLI 仅代表用户显式调用。

## 记录格式

```text
platform: macOS 15 / Windows 11 / Ubuntu 24.04
fileterm commit: <commit>
artifact: signed / unsigned package label
provider: protocol + non-sensitive endpoint label
target: disposable SSH target label
result: pass | fail
notes: <mode, approval, exec isolation, credential, retry result>
```

不要使用生产主机或生产凭据；日志、截图和 issue 必须脱敏。

## 已完成的本地回归

2026-08-14 已完成 Rust/TypeScript 迁移验证：旧 `app_run_ai_review`、命令卡写入/审核和 generic interactive-exec 的 API、MCP、CLI、renderer、audit、CI 入口均已删除；旧历史由 Rust 读取器迁移为统一工具活动。剩余跨平台打包、真实 Provider 和远端生产环境回归按上方清单执行。
