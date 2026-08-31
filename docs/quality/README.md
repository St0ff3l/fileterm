# Quality

本目录用于沉淀测试、回归、安全和发布检查清单。

当前最小质量要求：

- 跨层改动后运行 TypeScript 检查。
- 提交到 GitHub 的分支与 Pull Request 默认通过常规 CI，至少覆盖 `npm ci`、`npm run typecheck` 与 `npm run build`。
- 跨层改动需要确认 `Rust commands/events -> bridge -> renderer` 三层类型和调用保持一致。
- 协议相关改动需要覆盖 SSH/SFTP 与 FTP 的差异路径。
- SSH/FTP session 的连接生命周期、平台门控、输入输出、断线重连与操作串行化由 Rust 测试覆盖（`npm run test:tauri`），测试不访问公网。
- 文件传输相关改动需要确认任务状态、进度、取消或失败提示不回退。
- 主题相关改动需要确认明暗主题、终端配色和高频组件外观一致。
- 桌面壳和布局相关改动需要同时检查 macOS 标题栏避让、明暗主题、收起态侧栏、终端输入条和文件面板抽屉。

专项记录：

- `../integration-inventory.md`：记录当前已接入第三方项目、采用理由、实现位置和维护边界。
- `.github/workflows/ci.yml`：常规分支 / PR 检查，当前覆盖安装依赖、TypeScript 检查与构建。
- `git-branch-release-convention.md`：记录 `main / release/* / feat|feature|fix / hotfix/*` 的分支职责、合并流转和 tag 约定。
- `release-notes/`：按版本保存自定义 GitHub Release 正文，文件名格式为 `release-notes-<version>.md`。
- `terminal-layout-notes.md`：记录 xterm 终端留白、挂载容器与 fit 尺寸之间的排查和修复方法。
- `desktop-ui-regression-checklist.md`：记录顶部标签栏、首页侧栏、系统监控摘要、终端悬浮输入条、文件面板抽屉、文件编辑器和平台图标的 UI 回归清单。
- `ai-copilot-platform-regression.md`：记录 AI Provider、流式取消、代理、断网/睡眠恢复和目标绑定的 macOS、Windows、Linux 发行验收。
- `release-beta-mac.md`：记录 `v0.1.0-beta.1` 的 mac-only unsigned release 约定与发布流程。
