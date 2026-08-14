# 本地终端与 Agent MCP 接入

## 目标

提供与远程会话并列的本地终端标签页。用户可在每个本地终端直接运行 `claude`、`codex` 等本机 CLI；FileTerm MCP 访问已经由桌面应用管理的远程连接。连接、凭据、审批和远程执行仍归 FileTerm 主进程管理。

## 已确认的边界

- 本地终端是 runtime-only 的本机 PTY，不是 `ConnectionProfile`；每个 tab/pane 拥有独立进程、输入通道、输出缓冲、CWD 和取消令牌。
- 本地终端可复用 pane tree，但同一分屏树不能混合 local 与 SSH pane。
- Rust 负责本机 PTY、进程生命周期、输入和 resize，renderer 复用 `TerminalView`。
- MCP/CLI 的普通远程 exec 使用独立 SSH exec channel，不写入可见 SSH tab transcript。
- 可见 SSH tab PTY 是用户操作通道，承载登录、MFA、验证码、确认和 REPL；普通 exec 检测到通用输入时返回 `REMOTE_INTERACTIVE_INPUT_REQUIRED`，不创建临时 interactive PTY。
- sudo/su 使用一次性显式密码、加密 profile 或 FileTerm 主窗口安全输入；Copilot/MCP 在缺少 sudo/su 凭据时可以询问用户后用一次性字段重试。
- FTP、Telnet、Serial 不伪装成支持远程 exec。

## 已完成

- [x] 本地 PTY tab、独立 pane/runtime、关闭和取消清理。
- [x] 本地终端与 SSH workspace 的 pane 类型隔离。
- [x] 仅监听 loopback 的 MCP runtime、owner-only descriptor 和 token 校验。
- [x] Claude Code / Codex CLI 注册命令生成；设置页不自动修改外部客户端配置。
- [x] MCP 连接、会话、目录、文件、传输和隧道工具及审批边界。
- [x] `fileterm_execute_remote_command` 独立 exec channel、有限输出、超时/截断和输入提示。
- [x] sudo/su 三层凭据来源、`SUDO_PASSWORD_NEEDED` / `SU_PASSWORD_NEEDED` 和用户明确的一次性字段。
- [x] 删除 `fileterm_execute_interactive_remote_command`、`fileterm interactive-exec`、临时 PTY task、interactive audit 和对应 renderer/IPC。
- [x] MCP schema、CLI help、package smoke 与普通 exec 失败边界同步。

## MCP 使用方式

启动 FileTerm 后，在“设置 → Agent / MCP”复制命令并由用户主动执行：

```sh
claude mcp add --scope user fileterm -- /Applications/FileTerm.app/Contents/MacOS/fileterm mcp
codex mcp add fileterm -- /Applications/FileTerm.app/Contents/MacOS/fileterm mcp
```

MCP 修改、执行、传输和隧道操作仍等待 FileTerm 审批；CLI 是用户显式启动的 JSON 接口，不重复弹审批。MCP/CLI 不返回连接凭据或 terminal transcript。

## 验收重点

1. 保持一个非敏感 SSH tab 已连接，确认 MCP 能看到已打开会话。
2. 运行 sudo/su 测试命令，验证 profile、主窗口安全输入和用户明确的一次性字段均按预期工作；密码不进入命令文本或 tool result。
3. 运行需要 MFA、确认或 REPL 的命令，验证普通 exec 返回 `REMOTE_INTERACTIVE_INPUT_REQUIRED`，不污染可见终端，也不启动隐藏 task PTY。
4. 在 macOS、Windows、Linux 打包产物中验证默认 shell、PTY resize、复制粘贴、字体、快捷键和 `fileterm mcp` 路径。

真实客户端、三平台打包和生产环境的证据记录在 `docs/quality/ai-copilot-platform-regression.md`；本计划不再引用已废弃的 interactive-exec 迁移方案。
