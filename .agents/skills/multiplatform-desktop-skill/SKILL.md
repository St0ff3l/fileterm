---
name: multiplatform-desktop-skill
description: 修改 FileTerm 的平台差异、原生窗口/菜单、终端快捷键、手势或字体度量时使用。
---

# FileTerm 跨平台桌面行为

先确定行为属于原生窗口、renderer 还是 xterm/远端程序。同名“关闭”和“缩放”不能共用错误的处理链。

## 平台与归属

- 平台唯一来源是 `window.fileterm.platform`（`darwin` / `win32` / `linux` / `browser`）；CSS 使用根节点 `data-platform`，不用 UA 或 `navigator.platform` 猜测。
- 原生窗口/菜单走 Rust commands；renderer 经 `tauri-api.ts` 调用。终端选择、查找和字号留在 TerminalView/xterm。
- macOS 保留原生 traffic lights；Windows/Linux 用自绘 WindowMenubar。拖拽区域与可点击控件分离；主窗口和独立窗口保持各自 frame/关闭链路。
- 菜单、快捷键、标题栏和 tray 汇聚到同一关闭/退出决策链。区分关闭 pane/tab、关闭窗口、退出、隐藏到 tray。
- 新增/迁移快捷键时核对平台、焦点、既有拥有者及 xterm 远端输入；不抢裸 `Ctrl+W` 等远端按键。
- 终端字号变化重新计算网格并同步远端 PTY；保留字体度量校准，不用 WebView zoom 代替终端字号。

## 按需细节

- 窗口、菜单、复制粘贴、选择、快捷键、pinch 或字体变更：读 [窗口与终端交互](references/windows-and-terminal.md) 对应章节。
- UI 控件和 CSS：使用 [通用组件 skill](../common-components-skill/SKILL.md)，不另立目录或颜色规范。
- Dock/tray、CWD 或远端平台探测：读 [平台与协议约束](../../../docs/quality/agent-platform-contracts.md)。
- macOS 标题栏几何、Windows 建窗线程及启动显窗行为：按需查 `docs/design.md` 与 `docs/architecture.md` 对应章节，保留原生校准算法。

## 验证

遵守 AGENTS.md 的代码门禁。窗口布局检查 traffic lights/menubar、窄窗口、最大化、高 DPI 与 CJK 字体截断；快捷键/手势检查输入框和终端焦点、菜单文案与实际 handler。

对应行为仍需 macOS、Windows、Linux 实测；没有平台环境时如实报告，不能把浏览器模拟视为原生平台通过。桌面截图与操作先按 `docs/quality/desktop-ui-preview.md` 识别目标开发进程。
