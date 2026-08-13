## FileTerm 2.2.0 Beta 1

这是一个以 AI Copilot 侧边栏为核心的测试版本，重点验证 AI 对话与 SSH 工作区的协同体验。AI 默认只提供建议和解释，不会自动执行终端命令。

### Beta 更新重点

- **AI Copilot 侧边栏**：在当前 SSH 工作区旁提供独立的 AI 对话页面，支持会话列表、标题、历史对话和当前会话切换，不需要打开额外子窗口。
- **安全的终端参考**：参考终端是显式开关，关闭时 AI 不读取终端输出；开启后仅向下一条消息附加当前终端上下文，并在界面中明确展示状态。
- **对话模式与命令卡模式**：普通对话用于解释和排查；命令卡模式用于整理成可检查、可复制的命令建议。命令卡不会自动执行命令，用户可以复制或手动写入当前终端。
- **命令建议安全边界**：命令卡会标识只读、需权限和风险未知等状态，并保留检查、复制和写入终端等明确动作，避免把生成内容伪装成已执行结果。
- **执行记录与上下文一致性**：命令卡只锁定当前正在执行的命令；同一标签页、主机、用户和目录下的其他命令卡继续可用。终端输出变化不会让整批命令卡失效，真正切换连接或工作环境时才使旧卡失效。
- **本地对话隐私**：对话历史保存在本地；纯对话模式只发送用户输入和本地对话历史，不主动读取主机、路径、文件或终端输出。
- **远程备份安全**：WebDAV/S3 远程备份使用加密备份包；上传时需要二次确认密码，下载时单次输入即可。密码至少 8 个字符并包含大小写字母，用户主动导出的 JSON 格式保持不变。
- **跨平台与界面细节**：继续完善 macOS、Windows、Linux 的 AI 侧边栏、终端工作区、滚动条、按钮、图标和复制操作一致性。
- **Windows 便携版**：Beta 发布同时提供 x64 免安装 portable zip；它不写入安装器注册信息，运行前需要系统已有 WebView2 Runtime。

### 本版本包含的主要 PR 和问题修复

- [PR #187](https://github.com/St0ff3l/fileterm/pull/187)：稳定 AI Copilot 侧边栏并完善远程备份安全链路。
- [PR #190](https://github.com/St0ff3l/fileterm/pull/190)：增加远程备份密码确认和下载密码校验。

遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。

### 测试版说明

- 这是 Beta 测试版，AI 侧边栏的交互和提示词仍可能继续调整。
- AI 生成的命令请先阅读并确认目标、权限、工作目录和副作用，再手动执行。
- 不要把密码、私钥、token 或其他敏感信息直接发送给 AI。

---

## FileTerm 2.2.0 Beta 1

FileTerm 2.2.0 Beta 1 is a test release centered on the AI Copilot sidebar and its collaboration with the SSH workspace. The AI provides explanations and suggestions by default; it does not execute terminal commands automatically.

### Beta Highlights

- **AI Copilot sidebar**: Use a dedicated AI conversation page beside the current SSH workspace, with conversation lists, titles, local history, and session switching without opening a separate child window.
- **Explicit terminal reference**: Terminal context is opt-in. When disabled, the AI does not read terminal output; when enabled, the latest terminal context is attached only to the next message and the state is visible in the UI.
- **Chat mode and command-card mode**: Chat mode is for explanations and troubleshooting. Command-card mode formats responses into reviewable and copyable command suggestions. Command cards never execute commands automatically; users can copy them or write them into the current terminal manually.
- **Safe command suggestions**: Command cards identify read-only, privileged, and unknown-risk operations and keep review, copy, and terminal-write actions explicit instead of presenting generated content as already executed.
- **Execution and context consistency**: Only the command card currently being executed is locked. Other cards remain available while the tab, host, user, and working directory are unchanged. Terminal output changes alone do not invalidate the whole batch.
- **Local conversation privacy**: Conversation history is stored locally. Pure chat mode sends only the user input and local conversation history; it does not proactively read the host, path, files, or terminal output.
- **Cross-platform UI polish**: Continue refining the AI sidebar and terminal workspace across macOS, Windows, and Linux, including scrollbars, buttons, icons, and copy interactions.
- **Windows portable build**: The Beta release also provides an x64 portable zip that does not install registration entries; the system must already have the WebView2 Runtime available.

### Main PRs

- [PR #187](https://github.com/St0ff3l/fileterm/pull/187): Stabilize the AI Copilot sidebar and harden the remote backup security flow.
- [PR #190](https://github.com/St0ff3l/fileterm/pull/190): Add remote backup password confirmation and download password validation.

### Beta Notes

- This is a Beta release; AI sidebar interactions, prompts, and visual details may continue to change.
- Review the target, permissions, working directory, and side effects of every AI-generated command before running it manually.
- Do not send passwords, private keys, tokens, or other sensitive information to the AI.
- For problems, open a [GitHub Issue](https://github.com/St0ff3l/fileterm/issues) with your operating system, FileTerm version, connection type, reproduction steps, and redacted logs.

Join the community through the [README community section](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81), or join QQ group `534418986`.
