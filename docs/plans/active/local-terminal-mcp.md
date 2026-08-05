# 本地终端与 Agent MCP 接入

## 目标

提供与远程会话并列的本地终端标签页。用户可在每个本地终端直接运行 `claude`、`codex` 等本机 CLI；后续通过 FileTerm MCP 访问已经由桌面应用管理的远程连接，而不暴露连接凭据或模拟用户对 SSH PTY 的键盘输入。

## 已确认的边界

- 本地终端是本机 PTY，不是 `ConnectionProfile`；它作为 runtime-only 的 `WorkspaceTab(sessionType: 'local')` 出现。
- 每次创建本地终端都会分配新的 tab ID、PTY、输入通道和取消令牌；标签之间不共享进程、输出缓冲或工作目录。
- 本地终端不进入远程文件、传输、资源监控、分屏等 SSH workspace 能力边界。
- Rust 负责本机 PTY、进程生命周期、输入和 resize；renderer 复用 `TerminalView`。
- 终端数据沿用 `Rust runtime → Tauri Channel → bridge → TerminalView`，不从 renderer 直接启动 shell。
- 本地 Agent 通过 stdio MCP 调用 FileTerm；MCP adapter 仅作协议转换，连接、凭据、审批和审计仍归 FileTerm 主程序。
- Agent 的远程操作走独立 SSH exec / SFTP / transfer service，绝不写入正在显示的交互式 SSH PTY。

## 当前实现

- ✅ 本地 PTY：从首页快捷入口创建普通 `local` 标签，复用 `TerminalView`。每个标签有独立的 shell、PTY、输入通道和运行实例 ID；关闭或重启一个标签不会影响其他标签。
- ✅ 三边终端壳：本地终端内容区的左、右、下方固定保留 `10px`，颜色复用标题栏 token；顶部保持与标签栏直接相接。
- ✅ MCP runtime：桌面应用启动一个仅限 `127.0.0.1` 的随机端口，向 owner-only 的运行描述文件写入单次随机 token。`fileterm mcp` 以 stdio JSON-RPC 对接 MCP client，再以 token 访问该本地端口。
- ✅ 首批只读工具：`fileterm_list_connections`、`fileterm_get_session_context`、`fileterm_list_remote_directory`。目录工具只使用已打开且有文件能力的 workspace session。
- ⏳ 后续只读工具：读取远程文件、系统信息。
- ⏳ 受控写入：`fileterm_execute_remote_command`、写文件、传输；每项动作必须在 UI 进行确定性审批并写入审计记录。
- ⏳ Agent 启动体验：在应用内协助配置 Claude/Codex MCP server，并提供连接范围选择。

## MCP 使用方式

启动 FileTerm 桌面应用后，把安装包内的可执行文件注册为 stdio MCP server。以 macOS 安装包为例：

```sh
codex mcp add fileterm -- /Applications/FileTerm.app/Contents/MacOS/fileterm mcp
```

其他 MCP client 使用同一 command/args 组合：command 为 FileTerm 的桌面可执行文件，args 为 `mcp`。开发环境则可使用构建产物 `apps/tauri/src-tauri/target/debug/fileterm mcp`。FileTerm 未运行、退出或重启后，工具会返回可重试的本地应用不可用错误。

## 验收

- macOS、Windows、Linux 都能启动原生默认 shell，且 resize、复制粘贴、终端字体和快捷键遵循现有 `TerminalView` 规则。
- 多个本地 terminal tab 的 shell、输出和关闭彼此隔离；本地 shell 退出不会影响远程 session，应用退出会停止全部本机 PTY。
- stdio MCP 的 stdout 只输出 JSON-RPC，日志写 stderr；工具名带 `fileterm_` 前缀并标注读写风险。
- MCP 进程无法读取明文 credential，且没有运行中的 FileTerm 时给出明确、无敏感信息的错误。
