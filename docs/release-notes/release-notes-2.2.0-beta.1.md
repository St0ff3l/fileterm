## FileTerm 2.2.0 Beta 1

这是一个以 AI Copilot 侧边栏为核心的测试版本，重点验证 AI 对话与 SSH 工作区的协同体验。Copilot 提供纯对话、半自动和全自动三种模式；执行统一显示为工具活动，并由审批或本地护栏控制。

### Beta 更新重点

- **AI Copilot 侧边栏**：在当前 SSH 工作区旁提供独立的 AI 对话页面，支持会话列表、标题、历史对话和当前会话切换，不需要打开额外子窗口。
- **安全的终端参考**：参考终端是显式开关，关闭时 AI 不读取终端输出；开启后仅向下一条消息附加当前终端上下文，并在界面中明确展示状态。
- **三种 Copilot 模式**：纯对话只回答；半自动逐次审批工具调用；全自动通过危险命令限制和目标绑定后执行。全自动不显示累计次数上限或 `0/20`。
- **统一工具活动**：工具提案、审批、执行结果、退出码、超时和截断状态在对话中以内联活动展示，不再生成独立命令卡或 Review 记录。
- **双通道执行边界**：可见 SSH tab 供用户处理 MFA、验证码、确认和 REPL；普通后台 exec 使用独立 SSH channel，不写入 terminal transcript。sudo/su 可通过用户明确的一次性密码、加密 profile 或主窗口安全输入完成。
- **本地对话与上下文**：对话历史保存在本地；纯对话默认只发送用户输入和本地历史，半自动/全自动或用户主动开启参考终端时才附带一次性 L2 上下文。
- **远程备份安全**：WebDAV/S3 远程备份使用加密备份包；上传时需要二次确认密码，下载时单次输入即可。密码至少 8 个字符并包含大小写字母，用户主动导出的 JSON 格式保持不变。
- **跨平台与界面细节**：继续完善 macOS、Windows、Linux 的 AI 侧边栏、终端工作区、滚动条、按钮、图标和复制操作一致性。
- **Windows 便携版**：Beta 发布同时提供 x64 免安装 portable `.exe`；它不写入安装器注册信息，运行前需要系统已有 WebView2 Runtime。

### 本版本包含的主要 PR 和问题修复

- [PR #187](https://github.com/St0ff3l/fileterm/pull/187)：稳定 AI Copilot 侧边栏并完善远程备份安全链路。
- [PR #190](https://github.com/St0ff3l/fileterm/pull/190)：增加远程备份密码确认和下载密码校验。

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

### 测试版说明

- 这是 Beta 测试版，AI 侧边栏的交互和提示词仍可能继续调整。
- AI 执行前请确认目标、权限、工作目录和副作用；半自动会逐次请求审批。
- sudo/su 缺少凭据时，可使用 profile、主窗口安全输入，或在用户明确同意后由 Agent 询问并传递一次性密码；不要把密码写进命令文本。

---

## FileTerm 2.2.0 Beta 1

FileTerm 2.2.0 Beta 1 is a test release centered on the AI Copilot sidebar and its collaboration with the SSH workspace. Copilot offers pure conversation, semi-automatic approval, and fully automatic guarded execution; tool activity remains visible in the conversation.

### Beta Highlights

- **AI Copilot sidebar**: Use a dedicated AI conversation page beside the current SSH workspace, with conversation lists, titles, local history, and session switching without opening a separate child window.
- **Explicit terminal reference**: Terminal context is opt-in. When disabled, the AI does not read terminal output; when enabled, the latest terminal context is attached only to the next message and the state is visible in the UI.
- **Three Copilot modes**: Pure conversation only answers; semi-automatic asks for approval for each tool call; fully automatic executes only after local guardrails and target binding. Fully automatic has no cumulative counter or `0/20` display.
- **Unified tool activity**: Tool proposals, approval, execution status, exit code, timeout, and truncation are rendered inline instead of as command-card or Review compatibility records.
- **Two execution channels**: The visible SSH tab handles MFA, codes, confirmations, and REPLs. Background exec uses an independent SSH channel and never writes the terminal transcript. Sudo/su can use an explicit one-shot password, encrypted profile, or secure main-window prompt.
- **Local conversation and context**: Conversation history is stored locally. Pure chat defaults to user input and local history only; semi/full modes or an explicit terminal reference attach one-time L2 context.
- **Cross-platform UI polish**: Continue refining the AI sidebar and terminal workspace across macOS, Windows, and Linux, including scrollbars, buttons, icons, and copy interactions.
- **Windows portable build**: The Beta release also provides an x64 portable `.exe` that does not install registration entries; the system must already have the WebView2 Runtime available.

### Main PRs

- [PR #187](https://github.com/St0ff3l/fileterm/pull/187): Stabilize the AI Copilot sidebar and harden the remote backup security flow.
- [PR #190](https://github.com/St0ff3l/fileterm/pull/190): Add remote backup password confirmation and download password validation.

### Beta Notes

- This is a Beta release; AI sidebar interactions, prompts, and visual details may continue to change.
- Review the target, permissions, working directory, and side effects of AI tool calls; semi-automatic mode asks before each call.
- For sudo/su, use a profile secret, the secure prompt, or an explicitly user-provided one-shot password. Never put a password in the command text.
- For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your operating system, FileTerm version, connection type, reproduction steps, and redacted logs.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81), or join QQ group `534418986`.
