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
- ✅ 三边终端壳：本地终端内容区的左、右、下方固定保留 `15px` 灰色外壳，顶部保持与标签栏直接相接；外壳已独立成组件。
- ✅ MCP runtime：桌面应用启动一个仅限 `127.0.0.1` 的随机端口，向 owner-only 的运行描述文件写入单次随机 token。`fileterm mcp` 以 stdio JSON-RPC 对接 MCP client，再以 token 访问该本地端口。
- ✅ 只读 MCP 工具：连接列表、会话上下文、命令模板、远程目录、远程文件、传输任务和 SSH 隧道列表。返回的会话摘要不含终端 transcript，连接列表使用去除 credential 的 public profile。
- ✅ 会话 MCP 工具：打开、激活、重连、断开和关闭会话。打开/重连/断开/关闭属于状态改变操作，统一进入审批队列。
- ✅ 远程执行 MCP 工具：`fileterm_execute_remote_command` 使用独立 SSH exec channel，命令输出不会混入用户正在看的交互式 PTY；命令长度、CWD 和超时均有上限。FTP/Telnet/Serial 不伪装成支持远程 exec。
- ✅ 远程文件 MCP 工具：读写、创建文件/目录、复制、移动、重命名、删除、权限修改和 user/root 文件访问模式切换。root 凭据不接受 MCP 参数，只能复用 FileTerm 已建立的授权状态。
- ✅ 传输 MCP 工具：上传、下载文件/目录、查看进度、暂停、继续、丢弃断点和清理传输记录；实际运行仍由 FileTerm transfer service 管理。
- ✅ SSH 隧道 MCP 工具：创建、启动、停止、删除隧道规则，并复用现有 SSH worker 的校验和生命周期。
- ✅ CLI：`fileterm connections`、`sessions`、`directory/ls`、`read/cat`、`exec`、`write`、`mkdir`、`touch`、`copy`、`move`、`rename`、`delete`、`chmod`、`access`、`upload`、`download`、传输控制、隧道控制以及 `call ACTION --params-json JSON`。CLI 是用户显式启动的 JSON 接口，不重复弹应用内审批；MCP tools/call 则默认要求审批。
- ✅ MCP 审批：桌面主窗口收到 `mcp:approval-request` 后排队显示确认对话框；拒绝、窗口关闭、请求超时或 renderer 不可用都不会执行操作。审批详情只展示给用户，不写入日志。
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
- stdio MCP 的 stdout 只输出 JSON-RPC，日志写 stderr；工具名带 `fileterm_` 前缀并标注读写风险，响应错误使用 MCP tool result 的 `isError`。
- MCP 进程无法读取明文 credential，且没有运行中的 FileTerm 时给出明确、无敏感信息的错误。
- MCP 请求和响应有大小、超时和并发上限；目录/列表接口支持分页，远程文件读取会标记截断结果。
- MCP 的修改、执行、传输和隧道操作均先等待 FileTerm 审批；CLI 的显式调用与 MCP 是同一套 action/鉴权边界，但不把 CLI 当作无人值守审批绕过入口。
