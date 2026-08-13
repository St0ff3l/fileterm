# AI Copilot 跨平台发行验收

本清单记录 AI Copilot 不能由当前开发机或纯 Rust fixture 伪造的发行前验证。勾选前需保存平台、FileTerm commit、Provider 协议族/版本、网络环境和失败日志。现有 GitHub Actions 的 macOS、Windows、Linux 生命周期矩阵仍是基础回归，但不等同于打包桌面应用的 AI 验收。

## 准备

- 使用打包后的 Tauri 应用，而非仅运行 `cargo test` 或开发服务器。
- 在三个平台分别配置一个可控的 OpenAI-compatible Provider；如使用本地 mock，必须确认 Base URL、模型名和 API Key 均为测试值。
- 为代理与断网用例准备可开关的 HTTP CONNECT 或 SOCKS5 代理，并确保不会在测试日志中保存真实 API Key 或终端输出。
- 准备一个非敏感 SSH 测试目标和可重复的短命令；不要在生产主机上测试 Review Mode 或上下文上传。

### 可控本地 Provider（推荐）

仓库内提供仅监听 loopback 的 OpenAI-compatible fixture，用于把 L0 流式、停止、断连重试、usage 和命令卡的验收变成可重复操作：

```bash
npm run qa:ai-copilot-fixture
```

在打包应用的 AI Provider 设置中填写以下 **测试专用** 值：

| 字段            | 值                         |
| --------------- | -------------------------- |
| Provider 类型   | `OpenAI-compatible Chat`   |
| Base URL        | `http://127.0.0.1:9419/v1` |
| 模型            | `fileterm-fixture`         |
| API Key         | `fileterm-fixture-key`     |
| 允许不安全 HTTP | 开启                       |
| 无 API Key      | 关闭                       |

fixture 只记录请求模式和长度，绝不记录 prompt 或 `Authorization` 内容。可用下列消息触发确定性行为：

- `fixture:hello`：普通 SSE 回答和 usage。
- `fixture:slow`：持续流式输出，便于验证“停止”与关闭面板/窗口后的取消。
- `fixture:fail-once`：首个请求返回 HTTP 503；对同一消息点击“重试”后成功。
- `fixture:disconnect-once`：首个请求在首个 SSE chunk 后断开；对同一消息重试后成功。
- `fixture:markdown`：流式返回标题、列表、表格、代码块和链接，并带有应被忽略的原始 HTML、图片和 `javascript:` 链接。
- `fixture:tool`：返回一次 `fileterm_execute_remote_command` 的 `id -u` tool call；Provider 收到 Rust 工具结果后返回最终回答，可验证半自动审批和全自动工具循环。
- `fixture:tool-sudo`：返回一次 `sudo id -u` tool call；在测试 profile 未配置 sudo 复用时，可验证 FileTerm 任务专属 sudo 密码弹窗和一次性执行。
- 迁移期 command-proposal 兼容回归：在已授权 L2 上下文发送 `fixture:command` 或 `fixture:multiline`，分别返回只读 `pwd` 卡和多行卡；新 UI 不再提供“命令建议”切换，三模式选择器直接决定新回合语义。

该 fixture 故意绑定 `127.0.0.1`，而应用会对 loopback Provider 禁用系统代理，避免本机 API Key 被意外转发。因此它**不能**替代 HTTP CONNECT / SOCKS5 验收；代理项仍须使用一个受控的非 loopback Provider 或相应测试网络。

### 已自动化的 CI 证据

- `npm run qa:ai-copilot-fixture-smoke` 会启动随机 loopback 端口，真实发送 OpenAI-compatible 请求，验证“先普通回答、再切到命令卡模式并只输入‘重新来’”仍返回严格命令卡 JSON、一次 503 后的重试恢复，以及 tool-call / sudo tool-call 契约。
- PR CI 的 `tauri-socket-lifecycle` macOS、Windows、Linux 矩阵会额外运行 AI Copilot 的三类 Provider 解析/schema、历史回放、模式边界、自动护栏，以及 `action_review`、profile secret、PTY 密码提示契约测试；这补足跨平台编译与纯逻辑回归，但仍不替代下面的真实 Provider、桌面 UI 或远端 SSH 验收。
- PR CI 的 `tauri-package-smoke` 会在 macOS、Windows、Linux 生成无签名包，检查 `.app/.dmg`、NSIS installer、`.deb/.AppImage`，运行 release binary 的 `mcp --help` 与 `interactive-exec --help`，并通过 `scripts/mcp-stdio-smoke.mjs` 真实完成 MCP `initialize`、`tools/list` 和交互/提权 schema 校验。这证明打包产物内的 MCP runtime 可被 stdio 客户端握手，但不代表签名、公证、真实桌面交互或真实 Claude/Codex 模型验收已完成。

### 真实 Claude/Codex 接入命令

设置页只生成命令，不自动改写外部客户端配置。以 macOS 安装包为例，两个客户端都使用 stdio，并指向同一个 FileTerm 可执行文件：

```sh
claude mcp add --scope user fileterm -- /Applications/FileTerm.app/Contents/MacOS/fileterm mcp
codex mcp add fileterm -- /Applications/FileTerm.app/Contents/MacOS/fileterm mcp
```

设置页当前为 Claude 生成 `--scope user`；用户若只想写入当前项目，可自行改为 Claude 支持的 `--scope local`。真实接入验收时，先确保 FileTerm 主窗口和一个非敏感 SSH tab 保持打开，再让 Agent 调用交互式执行 tool。需要密码、MFA 或确认输入时，输入必须由 FileTerm 的任务专属安全弹窗收集；Agent 不应要求用户写入可见终端、聊天或 MCP 参数。密码提交后 Agent 只能获得脱敏结果，取消、超时、切换 tab 或断线应安全结束任务。不要使用生产凭据或生产主机。

## 每个平台：macOS、Windows、Linux

- [ ] Provider 配置保存后重新打开设置：只显示 `hasApiKey`，不得回填 Key；默认 Provider、禁用和删除状态正确。
- [ ] L0 聊天连续发送两条消息，确认 Provider/model、usage、错误重试和本地历史搜索、重命名、删除均可用。
- [ ] 使用 `fixture:markdown`：标题、列表、表格、代码块和 HTTP(S) 链接应正常显示；原始 HTML、远程图片和非 HTTP(S) 链接不得渲染或触发请求，外链只能经系统浏览器打开。
- [ ] 流式输出中点击“停止”，确认请求停止、对话可继续且没有残留忙碌状态；随后关闭 AI 面板或整个窗口，确认不会崩溃或继续向已关闭窗口写事件。
- [ ] 经 HTTP CONNECT 或 SOCKS5 代理完成一轮流式聊天；停止代理后确认出现可重试连接错误，恢复网络后重试成功。
- [ ] 设备睡眠后唤醒，再发送一条消息；确认明确失败并可重试，或直接恢复，不得静默卡在生成中。
- [ ] 在 SSH tab 预览 L2 上下文后切换 tab、分屏、CWD 或身份；旧预览和迁移期历史命令卡必须显示“终端目标已变化”，且不能写入输入框。
- [ ] 命令卡只能复制或写入受控输入框，写入后不自动回车；多行、危险或已过期目标的命令不得一键写入。
- [ ] 对非敏感 SSH 测试命令点击“审核并运行”：确认框展示 host、CWD、完整命令、风险和超时；拒绝、关闭或超时都不启动远端 exec。批准后验证执行不写入交互式 PTY、结果/退出码/超时/截断显示为本地审核记录，且正在审核的对话不能被删除。

## 记录格式

每个平台至少附一条通过记录，包含：

```text
platform: macOS 15 / Windows 11 / Ubuntu 24.04
fileterm commit: <commit>
provider: <protocol + non-sensitive endpoint label>
network: direct / HTTP CONNECT / SOCKS5 / offline recovery
result: pass | fail
notes: <stream cancel, sleep recovery, close behavior, retry result>
```

任何失败都应保留脱敏日志，并在修复后重新跑该平台对应条目；不要用另一个平台的成功结果替代。

## 已执行记录（未完成发行签收）

### 2026-08-10 — macOS 本地包，部分通过

```text
platform: macOS 27.0 / arm64
fileterm commit: 86821841
provider: OpenAI-compatible Chat / loopback QA fixture
network: direct loopback（不经过系统代理）
result: pass（L0 + local L1 子集）
notes:
  - 用 npm run build -w @fileterm/tauri 生成 FileTerm.app，并使用隔离 HOME 启动，未触及日常应用数据。
  - Provider 保存后可被重启后的包体读取；Key 只显示“已保存”状态，不回填到设置表单。
  - L0 连续消息、usage、本地历史、503 错误重试均正常。
  - fixture:slow 的“停止生成”与关闭 AI 面板都会中断 SSE；服务端没有记录 stream-completed，面板可立刻继续发送且不显示连接失败。
  - 本地终端的 L1 元数据预览可生成；fixture:command 只产生可审查的 pwd 命令卡，本地目标的写入和审核动作保持禁用。
```

这不是完整发行签收：该产物由本机普通 build 生成，不是 release 配置下的签名/公证包；Windows、Linux、代理、睡眠恢复、SSH 目标变化和 SSH Review Mode 仍必须按上方清单在对应环境完成。

### 2026-08-10 — macOS 本地包，SSH host key 与 Review Mode

```text
platform: macOS 27.0 / arm64
fileterm commit: ad5d00c1
target: disposable local Docker sshd (127.0.0.1:2222)
provider: OpenAI-compatible Chat / loopback QA fixture
result: pass（SSH host key + L1 + Review Mode）
notes:
  - 空 trustedHostFingerprint 会按“尚未信任”处理，不会被误判为已保存指纹不匹配。
  - 首次 host key 弹窗出现后，人工等待超过 35 秒再选择“只接受本次”，SSH 仍能成功认证；网络握手 30 秒与人工确认 300 秒分别计时。
  - 会话元数据预览、结构化 pwd 命令卡和风险标识均正常生成。
  - “写入当前终端”只留下 pwd 输入，不自动回车、没有产生交互终端输出。
  - “审核并运行”先显示目标、工作目录、风险、30 秒超时与完整命令；确认后经独立 SSH exec 通道执行一次，审计记录输出 /home/filetermqa。
```

本条仅覆盖本机普通 build + 一次性 localhost 容器，不等同于签名/公证发行包验证；Windows、Linux、代理、睡眠恢复、SSH 目标变更与真实远端环境仍需按清单验收。

### 2026-08-13 — macOS release 构建产物

```text
platform: macOS / arm64
fileterm commit: f1be6ad4 + local working tree
artifact: release FileTerm.app + FileTerm_2.1.8_aarch64.dmg
result: pass（production bundle）
notes:
  - `npm run release:mac -w @fileterm/tauri` 完整通过，包含 renderer production build、Rust release 编译、adhoc 签名和 DMG 打包。
  - 未执行 notarization：本机未设置 Apple 发布凭据。这不影响开发/QA 构建结果，但不能替代正式发布公证。
  - 本条只证明 release 打包链路；Provider 流式、代理、睡眠恢复和三端 UI 行为仍按上方清单在对应环境验收。
```

### 2026-08-14 — macOS QA bundle，三模式工具循环与 sudo 安全输入

```text
platform: macOS / arm64
fileterm commit: 3449bc55（fixture tool-call）；Copilot/Rust 基线为 0f3ebf4d
artifact: debug FileTerm QA.app（唯一 bundle id com.fileterm.qa）
provider: OpenAI-compatible Chat / loopback QA fixture（fixture:tool、fixture:tool-sudo）
target: disposable Debian 13-slim sshd（127.0.0.1:22222，测试用户 filetermqa）
network: direct loopback
result: pass（macOS 本机 QA 子集）
notes:
  - Provider 设置连接测试成功；重开设置只显示 Key 已保存状态，不回填 Key。
  - 纯对话完成真实 SSE Markdown 流式渲染；原始 HTML、远程图片和 javascript: 链接未成为可执行内容。
  - 半自动 `fixture:tool` 显示 host / CWD / 风险 / 30 秒超时审批框，批准后独立 exec 返回 `1000`；可见终端未被写入。
  - 全自动 `fixture:tool` 在 Rust 护栏通过后直接执行，界面计数从 `0/20` 增至 `1/20`，没有逐次审批框。
  - 关闭 profile 的 sudoSameAsLogin 后，半自动 `fixture:tool-sudo` 显示任务专属 sudo 密码弹窗；选择“仅本次执行”后返回 `0`，密码只显示为掩码且未进入 Provider 结果。
```

本条补齐 macOS 当前源码的 Provider、三模式工具循环、真实 disposable SSH 和 sudo 安全输入证据；Windows/Linux、真实外部 Provider、代理、睡眠恢复和 Claude/Codex 仍未签收。

### 2026-08-14 — macOS QA bundle，su PTY 密码竞态回归

```text
platform: macOS / arm64
fileterm commit: local PTY prompt-gated stdin fix
artifact: debug FileTerm QA.app（唯一 bundle id com.fileterm.qa）
provider: n/a（fileterm CLI / Rust action-review exec path）
target: disposable Debian 13-slim sshd（127.0.0.1:22222，测试用户 filetermqa）
network: direct loopback
result: pass（macOS 本机远程 exec 回归）
notes:
  - `su -c 'id -u'` 使用一次性密码返回退出码 0、输出 root uid 0，且不再出现 PTY stdin 竞态导致的超时。
  - 错误 su 密码稳定返回 `SU_AUTH_FAILURE`，没有残留 su 进程；普通 `sudo id -u` 仍返回退出码 0。
  - 修复后 PTY 输入仅在检测到密码提示后发送，并保留终端 VEOF；非 PTY 的 sudo stdin 路径不变。
```

本条验证了真实远端 `su` / `sudo` 的一次性安全输入与认证失败收敛；Windows/Linux 打包、真实外部 Provider、代理、睡眠恢复和 Claude/Codex 仍未签收。

### 2026-08-14 — macOS QA bundle，interactive-exec 安全输入与取消清理

```text
platform: macOS / arm64
artifact: 本地 debug FileTerm QA bundle（隔离 HOME；非生产连接数据）
provider: n/a（FileTerm CLI / MCP runtime bridge）
target: disposable Debian 13-slim sshd（127.0.0.1:22223，测试用户 filetermqa）
network: direct loopback
result: pass（macOS 本机 interactive-exec 子集）
notes:
  - `fileterm interactive-exec --tab-id ... --expected-session-revision ... --command "su -c 'id -u'"` 在 CLI 等待期间由 FileTerm 主窗口弹出任务级 masked `Password:` 输入框；提交后返回 `interactionCount: 1`、退出状态 0 和 root uid 0，结果未包含输入原文。
  - 取消同一任务后 CLI 快速结束，后续无交互 `pgrep` 未发现残留 `su` 进程；可见 SSH 终端仍保持连接。
  - 回归期间修复了 russh interactive channel 仅 drop 不发送 `SSH_MSG_CHANNEL_CLOSE` 的 PTY 清理缺口；成功、取消和超时路径现在统一执行有界 channel close。
```

本条补齐 macOS 的真实 FileTerm CLI → 安全输入框 → 同一 SSH task channel 验收；内置 Provider 真实对话、Windows/Linux 桌面手测、代理、睡眠恢复和生产环境仍未签收。

### 2026-08-14 — Claude/Codex MCP 只读 tool 调用

```text
platform: macOS / arm64
fileterm commit: 1dc35a85
provider: n/a（MCP stdio，fileterm_list_connections）
network: local stdio
result: partial（客户端实际 tool 调用通过；未执行远程命令）
notes:
  - Claude Code 使用 `--bare`、临时内联 MCP 配置和只读 tool allowlist，实际调用 `fileterm_list_connections`，返回 `total=12`。
  - Codex CLI 使用 `--ephemeral` 与 `-c mcp_servers.fileterm=...` 临时注册 release binary，实际调用同一只读 tool，返回 `total=12`。
  - 两次调用均未写入 Claude/Codex 持久配置、未打开连接、未执行远程命令，也未暴露 profile secret。
  - 本条只读调用不能替代需要已连接 SSH session 和用户可见任务级安全输入的 interactive-exec 端到端验收。
```

### 2026-08-14 — macOS 隔离 QA bundle，Provider 明文 HTTP 安全门禁

```text
platform: macOS / arm64
artifact: 临时复制并重新签名的 FileTerm QA.app（bundle id com.fileterm.qa）
provider: OpenAI-compatible Chat / loopback QA fixture
network: direct loopback
result: pass（安全门禁；未执行 Provider 对话）
notes:
  - 在隔离应用数据目录中填写测试 Provider、模型和 API Key；Key 在设置界面保持掩码显示。
  - 未勾选“允许不安全 HTTP”时，点击 Provider 连接测试在发送请求前稳定返回 `AI_PROVIDER_INSECURE_HTTP`；没有通过测试流程替用户打开该安全选项。
  - 本条只证明明文 HTTP 的显式授权门禁，不把 loopback fixture 当成真实 Provider，也不宣称三模式桌面运行时回归已由本条完成。
  - QA bundle、临时 fixture、隔离应用数据和测试 Key 已清理，日常 `com.fileterm.desktop` 数据未修改。
```

### 2026-08-14 — Claude/Codex MCP interactive-exec 真实调用

```text
platform: macOS / arm64
fileterm commit: 22d0fd78（本地 release bundle）
provider: Claude Code 2.1.229 / Codex CLI 0.147.0-alpha.6.5（MCP stdio）
target: disposable loopback SSH session（临时 profile/key/sshd）
network: local stdio + direct loopback
result: pass（两个真实客户端均完成一次交互）
notes:
  - Claude Code 与 Codex CLI 均实际调用 `fileterm_execute_interactive_remote_command`；FileTerm 先弹出 MCP action approval，再弹出任务级 masked 输入框。
  - 两次调用均完成 `interactionCount=1`，最终结果只包含脱敏后的命令输出，`inputRequired=false`，输入原文不在客户端结果中。
  - Codex 对一次会回显 secret-like 输入的测试命令先执行了客户端安全拒绝；改用不回显输入的安全命令后，FileTerm 交互和结果回传均通过。
  - 测试使用 disposable localhost 目标和合成输入；完成后已关闭 tab，删除临时 profile/key/私钥并停止临时 sshd。
```

### 2026-08-14 — Claude MCP 普通 exec 的 sudo/su 凭据边界

```text
platform: macOS / arm64
fileterm commit: local release bundle（当前分支基线 d4438978）
provider: Claude Code 2.1.229（MCP stdio）
target: disposable loopback SSH forced-command fixture（127.0.0.1:22225；非真实 root 主机）
network: local stdio + direct loopback
result: pass（外部调用边界子集；不等同生产提权）
notes:
  - Claude Code 实际调用 `fileterm_execute_remote_command`，分别执行 `sudo id -u` 与 `su -c "id -u"`；请求没有携带密码参数。
  - FileTerm 对两次 MCP mutation 都先显示 action approval；批准后从 profile 的加密 secret store 读取合成测试凭据，sudo 返回退出码 0 / uid 0，su 返回退出码 0 / uid 0。
  - 外部客户端只收到结构化执行结果，密码未进入 command 文本、聊天提示或 MCP 审计内容；错误 sudo 参数的稳定 `SUDO_AUTH_FAILURE` 也已由 FileTerm bridge 直接回归。
  - SSHD 使用仅用于验证 FileTerm 包装、stdin/PTY、审批和结果回传的确定性 forced-command fixture，不提供真实系统 root 权限；真实 Linux sudo/su、Windows/Linux 打包和三层凭据完整验收仍保持未完成。
  - 测试完成后已关闭 tab，删除临时 profile、加密 sudo/su secret、私钥和 SSHD。
```
