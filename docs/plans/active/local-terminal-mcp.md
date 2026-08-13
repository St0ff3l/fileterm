# 本地终端与 Agent MCP 接入

## 目标

提供与远程会话并列的本地终端标签页。用户可在每个本地终端直接运行 `claude`、`codex` 等本机 CLI；后续通过 FileTerm MCP 访问已经由桌面应用管理的远程连接，而不暴露连接凭据。默认远程操作不模拟用户对 SSH PTY 的键盘输入；需要用户回答密码或 MFA 的受限例外见[安全交互式远程执行计划](./mcp-cli-interactive-exec.md)。

## 已确认的边界

- 本地终端是本机 PTY，不是 `ConnectionProfile`；它作为 runtime-only 的 `WorkspaceTab(sessionType: 'local')` 出现。
- 每次创建本地终端都会分配新的 tab ID、PTY、输入通道和取消令牌；标签之间不共享进程、输出缓冲或工作目录。
- 本地终端不进入远程文件、传输和资源监控等 SSH workspace 能力边界；可复用通用 pane tree 做本地终端分屏，但同一分屏树只能包含 `local` pane。每个 pane 都是独立 PTY/runtime，不能与 SSH pane 混合或共享输入、输出、CWD。
- Rust 负责本机 PTY、进程生命周期、输入和 resize；renderer 复用 `TerminalView`。
- 终端数据沿用 `Rust runtime → Tauri Channel → bridge → TerminalView`，不从 renderer 直接启动 shell。
- 本地 Agent 通过 stdio MCP 调用 FileTerm；MCP adapter 仅作协议转换，连接、凭据、审批和审计仍归 FileTerm 主程序。
- Agent 的远程操作默认走独立 SSH exec / SFTP / transfer service，不写入正在显示的交互式 SSH PTY。唯一的受限例外是[安全交互式远程执行](./mcp-cli-interactive-exec.md)：在复用当前已认证 SSH transport 的临时隔离 channel 中运行命令；FileTerm 仅在该任务确实等待输入时向用户展示安全输入框，并把回答直接回送该任务，绝不通过可见终端 PTY 或 MCP/CLI 传递密码、MFA 或其他敏感值。

## 已完成

- ✅ 本地 PTY：从首页快捷入口创建普通 `local` 标签，复用 `TerminalView`。每个标签有独立的 shell、PTY、输入通道和运行实例 ID；关闭或重启一个标签不会影响其他标签。
- ✅ 三边终端壳：本地终端内容区的左、右、下方固定保留 `15px` 灰色外壳，顶部保持与标签栏直接相接；外壳已独立成组件。
- ✅ 本地终端分屏：本地 `local` tab 可通过现有快捷键或终端菜单拆分为新的本地 pane，复用通用 pane tree、焦点、尺寸权重和关闭逻辑。新 pane 继承当前 pane 已捕获的 CWD 与启动配置，但创建新的 worker、输入通道、取消令牌和 PTY；SSH 与本地 pane 不可混合。异步创建期间若源 pane 消失或挂树失败，会回收新 worker、PTY、tab 与运行时引用，不留下孤儿会话。
- ✅ MCP runtime：桌面应用启动一个仅限 `127.0.0.1` 的随机端口，向 owner-only 的运行描述文件写入单次随机 token。`fileterm mcp` 以 stdio JSON-RPC 对接 MCP client，再以 token 访问该本地端口。
- ✅ Agent 启动与范围控制：设置 → `Agent / MCP` 会只读检测本机 `claude` / `codex` 是否在 `PATH` 中，并提供可复制（不会自动执行或改写配置）的 stdio 注册命令；可选“全部已保存连接 / 当前活动会话 / 默认连接”以及“仅只读 / 经确认的操作”。范围同时收口在桌面 MCP bridge：连接列表、会话上下文和传输观察都按选择过滤，不能只靠 UI 隐藏。
- ✅ 应用内 Agent 入口：检测到客户端后，可从设置页在一个新建、可见且独立的本地 PTY tab 中启动 `claude` 或 `codex`。该 tab 只是常规本地终端，不存在后台代理、隐藏 stdin 或聊天中转；登录、密码、MFA 和后续 TUI 操作都直接在用户可见的终端中完成。用户点击启动的命令固定映射为受信任客户端标识，不能由 MCP 或 Agent 参数注入。
- ✅ 只读 MCP 工具：连接列表、会话上下文、命令模板、远程目录、远程文件、传输任务和 SSH 隧道列表。返回的会话摘要不含终端 transcript，连接列表使用去除 credential 的 public profile。
- ✅ 会话 MCP 工具：打开、激活、重连、断开和关闭会话。打开/重连/断开/关闭属于状态改变操作，统一进入审批队列。
- ✅ 远程执行 MCP 工具：`fileterm_execute_remote_command` 保持独立、非交互 SSH exec channel，命令输出不会混入用户正在看的交互式 PTY；超时仍保留已安全收集的部分输出并显式标注 `timedOut`。另有显式 `fileterm_execute_interactive_remote_command` / `fileterm interactive-exec`，只在需要密码、MFA 或短文本确认时复用当前已认证 SSH transport 的任务专属临时 PTY。用户输入通过 FileTerm 本地安全弹窗回送该任务，绝不要求用户写入可见终端或 Agent 聊天。命令长度、CWD 和超时均有上限；FTP/Telnet/Serial 不伪装成支持远程 exec。
- ✅ 远程文件 MCP 工具：读写、创建文件/目录、复制、移动、重命名、删除、权限修改和 user/root 文件访问模式切换。root 凭据不接受 MCP 参数，只能复用 FileTerm 已建立的授权状态。
- ✅ 传输 MCP 工具：上传、下载文件/目录、查看进度、暂停、继续、丢弃断点和清理传输记录；实际运行仍由 FileTerm transfer service 管理。
- ✅ SSH 隧道 MCP 工具：创建、启动、停止、删除隧道规则，并复用现有 SSH worker 的校验和生命周期。
- ✅ CLI：`fileterm connections`、`sessions`、`directory/ls`、`read/cat`、`exec`、`interactive-exec`、`write`、`mkdir`、`touch`、`copy`、`move`、`rename`、`delete`、`chmod`、`access`、`upload`、`download`、传输控制、隧道控制以及 `call ACTION --params-json JSON`。CLI 是用户显式启动的 JSON 接口，不重复弹应用内审批；MCP tools/call 则默认要求审批。
- ✅ 交互式执行最小审计：本地 JSONL 仅保留来源、公开 target、程序摘要 / hash、交互轮次和结果元数据；Unix 强制 `0600`，Windows 使用 per-user application-data ACL；不保存命令全文、prompt、用户输入或终端输出。
- ✅ MCP 审批：桌面主窗口收到 `mcp:approval-request` 后排队显示确认对话框；拒绝、窗口关闭、请求超时或 renderer 不可用都不会执行操作。审批详情只展示给用户，不写入日志。

## 待完成

以下是当前实现之后，仍需要补齐或验证的工作：

- ⏳ [安全交互式远程执行](./mcp-cli-interactive-exec.md) 三端与真实客户端验收：实现已保留默认独立 exec，并在需要密码、MFA 或确认回答时复用已打开 SSH tab 的同一认证 transport，在专属 channel 中等待 FileTerm 的安全输入；仍需验证真实 Agent 不会要求用户把回答输入到一个 Agent 无法读取的终端。
- ⏳ Agent 实际交互验收：在真实 Claude Code / Codex CLI 的本地 PTY 中验证首次登录、密码/MFA、TUI 输入、退出后返回 shell以及 MCP 注册命令的可用性。应用内入口已完成；不另做会吞掉交互输入的后台“聊天代理”。
- ⏳ 跨平台发行验证：在 macOS、Windows、Linux 的打包产物中验证默认 shell、PTY resize、复制粘贴、字体、快捷键以及 `fileterm mcp` 的可执行文件路径。
- ✅ PR CI 已加入 macOS、Windows、Linux 的无签名 Tauri package smoke，检查 `.app/.dmg`、NSIS installer、`.deb/.AppImage` 可生成；这不替代上述打包应用内的 shell、PTY、字体、快捷键和 `fileterm mcp` 运行验收。
- ⏳ Agent 实际接入验收：使用真实的 Claude Code、Codex CLI 和至少一个外部 MCP client 完成初始化、审批、远程执行、文件操作、传输和隧道的端到端验证。
- ✅ 远程执行结果增强：普通独立 exec 返回 `outputTruncated`、`timedOut` 与超时前已安全收集的部分输出，并在检测到受支持的输入提示时返回脱敏的 `inputRequired/inputKind` 路由提示；交互式 exec 保持同一结果形状，并将命令运行预算与安全输入等待 deadline 分开。Windows 远端 CWD 与 shell 命令兼容性仍列入跨平台发行验收。
- ✅ 传输观察能力：`fileterm_wait_for_transfer` / `fileterm wait-transfer` 在 FileTerm 本地等待任务进入终态（最长 120 秒）并返回最新任务快照；等待超时只表示任务仍在进行，不会取消传输或让 Agent 反复轮询 `list_transfers`。
- ✅ 稳定性与可维护性：关键读写、远程执行和传输等待工具发布 machine-readable `outputSchema`；错误结果带稳定 `error.code` 与 `retryable`。审批目标详情和交互式执行的最小本地审计已落地；可选的更完整本地审计历史仍可后续扩展。

## 暂不纳入本阶段

这些能力不是当前 CLI/MCP 最小完整闭环的一部分，后续若有明确需求再单独立项：

- 本机文件系统的 MCP/CLI 读写、删除、复制和传输；本地 Agent 已可通过本地 shell 完成这些操作。
- 连接 profile 的创建、编辑、删除、密码/密钥管理和凭据导出。
- 通过 MCP 直接读取终端 transcript、向交互式 PTY 注入任意/连续按键或改变 PTY resize；[安全交互式远程执行](./mcp-cli-interactive-exec.md)仅允许任务专属 channel 的受控 stdin 回答，不读取或污染可见终端 PTY。
- FTP、Telnet、Serial 的非交互式远程执行；这些协议仍只保留各自已有的连接能力。
- 跨设备同步、远程 MCP 服务、云端代理和无人值守的全局审批策略。

## MCP 使用方式

启动 FileTerm 桌面应用后，在“设置 → Agent / MCP”复制对应客户端的注册命令并在本机 shell 执行。设置页只生成命令，不会自动改写 Claude Code 或 Codex CLI 的配置。以 macOS 安装包为例：

```sh
claude mcp add --scope user fileterm -- /Applications/FileTerm.app/Contents/MacOS/fileterm mcp
codex mcp add fileterm -- /Applications/FileTerm.app/Contents/MacOS/fileterm mcp
```

其他 MCP client 使用同一 command/args 组合：command 为 FileTerm 的桌面可执行文件，args 为 `mcp`。当前设置页对 Claude Code 生成 `--scope user`，表示写入当前用户配置；如果需要只写入当前项目，可由用户主动改成 Claude 支持的 `--scope local`，FileTerm 不替用户选择范围。开发环境则可使用构建产物 `apps/tauri/src-tauri/target/debug/fileterm mcp`。FileTerm 未运行、退出或重启后，工具会返回可重试的本地应用不可用错误。

### 交互式命令的真实客户端验收

普通 MCP 请求默认仍使用独立的非交互 `fileterm_execute_remote_command`。只有明确需要密码、MFA、Y/N 或其他短文本输入时，Agent 才应调用 `fileterm_execute_interactive_remote_command`（或显式的 `fileterm interactive-exec`）。这条路径会复用当前已打开 SSH tab 的认证 transport，在 FileTerm 主窗口弹出任务专属安全输入框；用户的回答只回到该任务，不会进入 Agent 聊天、MCP 参数、可见终端或审计记录。

接入后的最小验收顺序：

1. 在 FileTerm 中保持一个非敏感 SSH tab 已连接，并确认主窗口可见。
2. 在 Claude Code 或 Codex CLI 中调用 `fileterm_list_connections`、`fileterm_get_session_context`，确认 MCP stdio 能看到当前 runtime。
3. 让 Agent 请求一个会等待输入的测试命令；确认输入框由 FileTerm 弹出，而不是 Agent 要求用户去可见终端或聊天中输入密码。
4. 提交测试输入后，确认 Agent 只收到脱敏的最终结果；取消、超时、切换 tab 或断开连接时任务应 fail closed。

真实客户端验收不得使用生产密码或生产主机。若 Agent 仍要求用户向可见终端输入，说明它调用了普通 `exec` 或没有遵循 MCP tool description，应停止重试并检查调用的 tool 名称；不要把密码粘贴到 Agent 聊天或 MCP/CLI 参数中。

## 验收状态

- ✅ 自动化门禁：Rust 单元/契约测试、Rust clippy、Tauri typecheck、lint 和 Prettier 已通过。
- ✅ MCP stdio 冒烟：2026-08-13 已对正在运行的桌面 runtime 验证 `initialize`、`tools/list`、`fileterm_list_connections` 与 `fileterm_get_session_context`；当前暴露 35 个 `fileterm_` 工具，返回连接摘要不含凭据或 terminal transcript。
- ✅ non-GUI CLI 契约冒烟：macOS、Windows、Linux CI 会直接运行 `interactive-exec --help`、`wait-transfer --help` 与 `mcp --help`，并在三端执行 MCP schema 与设备绑定凭据加解密单测；这些检查不会伪装成真实 Agent 或打包应用验收。
- ⏳ 跨平台手工验收：打包产物和真实 Claude/Codex 集成仍属于上面的待完成项。

## 验收标准

- macOS、Windows、Linux 都能启动原生默认 shell，且 resize、复制粘贴、终端字体和快捷键遵循现有 `TerminalView` 规则。
- 多个本地 terminal tab 的 shell、输出和关闭彼此隔离；本地 shell 退出不会影响远程 session，应用退出会停止全部本机 PTY。
- 本地 terminal 分屏中的每个 pane 都拥有独立 PTY/runtime；拆分、关闭、重连、聚焦与尺寸调整不会影响相邻 pane，且无法把 SSH 与 local pane 混入同一棵 pane tree。
- stdio MCP 的 stdout 只输出 JSON-RPC，日志写 stderr；工具名带 `fileterm_` 前缀并标注读写风险，响应错误使用 MCP tool result 的 `isError`。
- MCP 进程无法读取明文 credential，且没有运行中的 FileTerm 时给出明确、无敏感信息的错误。
- MCP 请求和响应有大小、超时和并发上限；目录/列表接口支持分页，远程文件读取会标记截断结果；远程执行、分页读取、文件读取和传输等待工具发布 machine-readable `outputSchema`，错误结果包含稳定 `error.code` 与 `retryable`。
- MCP 的修改、执行、传输和隧道操作均先等待 FileTerm 审批；CLI 的显式调用与 MCP 是同一套 action/鉴权边界，但不把 CLI 当作无人值守审批绕过入口。
