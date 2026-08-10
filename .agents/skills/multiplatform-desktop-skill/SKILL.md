---
name: multiplatform-desktop-skill
description: 为 FileTerm 的 Tauri + Rust + React + TypeScript 桌面应用处理 macOS、Windows、Linux 的 UI 差异、原生窗口与菜单、终端手势/字体、快捷键和功能归属冲突。用户提到跨平台、平台专用 UI、标题栏、菜单、终端快捷键、触控板/滚轮、字体、复制粘贴、窗口或标签关闭时使用。
---

# FileTerm 跨平台 UI 与快捷键技能

FileTerm 的生产链路是 **Tauri 2 + Rust + React + TypeScript**。当前分支已移除 `apps/electron`；不要引用 Electron 实现、目录或验证命令。

## 先定位归属

先分清用户要操作的对象；同一个按键或菜单名落在不同对象上，语义完全不同。

| 用户意图                         | 正确归属                               | 典型反例                            |
| -------------------------------- | -------------------------------------- | ----------------------------------- |
| 退出、最小化、原生菜单、系统窗口 | Rust / Tauri window command            | Renderer 直接调用系统 API           |
| 菜单项、标题栏布局、平台 CSS     | React renderer                         | 把 macOS 视觉留白套给 Windows/Linux |
| 终端复制、输入、选择、分屏、字号 | `TerminalView` / xterm                 | 用应用 WebView 缩放代替终端字号     |
| SSH / SFTP / FTP、远端 PTY 尺寸  | Rust command/event → bridge → renderer | Renderer 直接访问协议 client        |

实现前确认完整链路：`Rust command/event → apps/tauri/src/bridge/tauri-api.ts → packages/core 类型 → renderer`。新系统能力必须经过这条边界。

## 平台来源与 UI 分支

- 只使用 `window.fileterm.platform`；Tauri bridge 归一化为 `darwin`、`win32`、`linux` 或 `browser`。
- Renderer 启动时将该值写入 `document.documentElement.dataset.platform`；样式优先以 `:root[data-platform='…']` 分支。
- 禁止 `navigator.platform`、UA 猜测、硬编码平台布尔值，或用 CSS 媒体查询猜操作系统。
- macOS 使用原生窗口语义与 traffic lights；Windows/Linux 使用自绘 `WindowMenubar`。不要让任一方的标题栏、拖拽区或菜单结构泄漏到另一方。
- 平台视觉差异应写在 theme token / 平台样式层，而不是散落在业务 JSX 内。

## 详细 UI 实现规则

### 标题栏、窗口与布局

- **macOS**：保留系统 decorations 与 traffic lights，renderer 只为左上角控件留出避让空间。不要渲染 Windows/Linux 的菜单栏或自绘关闭按钮；不要把 traffic lights 当作普通 React button。
- **Windows/Linux**：主窗口由 renderer 提供紧凑的 `WindowMenubar` 与窗口控制。拖拽区必须使用 `data-tauri-drag-region`，菜单、关闭/最小化/最大化按钮及其他可点击控件必须排除拖拽行为。
- **独立窗口**：连接表单、命令表单和文件编辑器有自己的 frame 与关闭链路。检查主窗口的标题栏样式不会覆盖独立窗口，也不要把主窗口的菜单状态复用到独立窗口。
- **布局改动**：同时验证 macOS traffic lights 避让、Windows/Linux 自绘 menubar 高度、最大化状态、窄窗口换行和高 DPI。标题栏的一个像素变化会向下挤压整个工作区。

主要位置：

- Rust 窗口创建、原生菜单、tray、关闭生命周期：`apps/tauri/src-tauri/src/lib.rs`
- 窗口动作与命令边界：`apps/tauri/src-tauri/src/commands/mod.rs`
- 自绘菜单：`apps/tauri/src/renderer/features/layout/WindowMenubar.tsx`
- renderer 平台分支：`apps/tauri/src/renderer/App.tsx`、`apps/tauri/src/renderer/main.tsx`
- shell 与平台样式：`apps/tauri/src/renderer/styles/features/shell.css`、`apps/tauri/src/renderer/styles/features/workstation-skin.css`

### 原生菜单、context menu 与关闭链路

- 原生窗口动作只能走 `app_window_action` 或专用 Tauri command；renderer 不能直接猜测窗口状态或调用 Web API 代替。
- 同一能力只能有一个权威入口：菜单项、标题栏按钮、键盘快捷键和 tray 必须汇聚到同一关闭/退出决定链。
- 区分“关闭当前 terminal pane/tab”“关闭主窗口”“退出应用”“隐藏到 tray”。相同的“关闭”文案不代表相同行为。
- 菜单在某个平台不适用时应直接隐藏，而不是显示一个无效、没有 handler 或会抢终端输入的菜单项。
- debug-only 的 devtools 入口必须受 `debug_assertions` / `import.meta.env.DEV` 双端约束；生产构建不暴露。

### 主题、平台 CSS 与视觉密度

- 颜色、阴影、圆角和间距先进入 `token → theme vars → component skin`，不要在业务组件散落十六进制颜色或平台专用 magic number。
- 所有表单下拉框统一使用 `<DropdownSelect>`：macOS 下渲染 `.ft-select-shell` 原生样式外壳，Windows 和 Linux 下自动开启自绘 Popover 浮层菜单 (`dropdown-select-trigger` + `dropdown-select-menu`)，严禁直接写原生 `<select>` 标签。
- 优先在已有 `data-platform` 选择器中写差异。需要新增 CSS 时，写清楚为何只影响一个平台；不要用 `!important` 覆盖不清楚来源的规则。
- Windows/Linux 的 CJK 字体高度可能高于 macOS。检查按钮、表格行、地址栏、标签栏、状态栏和弹窗标题的 line-height，避免文字垂直截断。
- 高 DPI 下检查图标是否发虚、1px 边框是否稳定、紧凑布局是否仍可点击。不要只依赖开发机浏览器的像素结果。

### 字体、图标、tray 与离线资源

- 所有字体、图标和基础样式必须随应用打包；禁止运行时依赖外部 CDN。
- 终端字体使用 `FILETERM_MONO_FONT_FAMILY`，UI 字体必须提供 CJK fallback。改变字体族、字重、字号或行高后，要重新检查标题栏、表格和终端网格。
- `observeCanvasTextMetrics()` 是终端字体度量同步的一部分；不要删除后只凭 CSS 看起来“正常”。
- macOS tray 使用 template image；Dock 图标、tray 图标、窗口图标是不同资源和尺寸策略，不能互相缩放复用。
- 资源路径必须同时验证开发态与打包态。文件能在仓库中找到，不代表 Tauri bundle 一定包含它。

## 快捷键：先做冲突表，再写代码

每次新增或迁移快捷键，先列出目标、平台、优先级和既有拥有者；不要只搜索菜单标签。

| 优先级 | 所属                | 规则                                                                                               |
| ------ | ------------------- | -------------------------------------------------------------------------------------------------- |
| 1      | 操作系统 / 原生窗口 | 保留 macOS `Cmd+Q`、`Cmd+W` 等原生语义；Windows/Linux 退出使用 `Alt+F4`。                          |
| 2      | 浏览器/WebView      | 避免默认页面缩放、打印、开发者工具和系统预留键抢终端输入。                                         |
| 3      | xterm / 远端程序    | `Ctrl+W`、Vim 键、readline、TUI 鼠标协议默认应交给远端。                                           |
| 4      | FileTerm 终端功能   | 只在终端焦点内拦截，调用 `preventDefault()`、`stopPropagation()` 并让 xterm handler 返回 `false`。 |
| 5      | 菜单展示            | 菜单快捷键文案必须与实际 handler 一致；没有实际绑定就不要展示。                                    |

当前已确认的终端边界：

- macOS 复制/粘贴：`Cmd+C` / `Cmd+V`；Windows/Linux：`Ctrl+Shift+C` / `Ctrl+Shift+V`。
- 关闭当前终端 pane/tab：macOS `Cmd+W`；Windows/Linux `Ctrl+Shift+W`。不要绑定裸 `Ctrl+W`。
- 分屏和终端缩放必须检查修饰键组合，避免 Windows 的 `Alt+Shift±` 分屏与 `Ctrl+Shift±` 终端缩放混淆。
- 终端字号属于 xterm：macOS `Cmd±`，Windows/Linux `Ctrl+Shift±`；回到默认分别使用 `Cmd+0` / `Ctrl+Shift+0`。不要调用 Tauri WebView zoom。
- 触控板 pinch 需要双路径：Chromium/WebView2 通常发送 `ctrlKey` wheel，macOS WKWebView 使用 `gesturestart` / `gesturechange` 的 `scale`。两条路径都只缩放当前终端并阻止 WebView 页面缩放。

检查点：菜单、`attachCustomKeyEventHandler`、窗口捕获阶段的 `keydown`、原生 Rust menu accelerator，以及 xterm 实际发送给远端的输入必须一起核对。

## 终端交互与功能方向冲突

- 先区分 **xterm 本地文本选择** 与 **Vim/TUI Visual 选择**。Vim 开启 mouse tracking 后，xterm 的 `hasSelection()` 会为空；不能把两者当作同一份状态。
- 右键菜单只在 `contextmenu` 时读取选择状态。`pointerdown` / `mousedown` 只负责阻断 xterm 向远端上报右键，不能提前清空或写入选区 state。
- 焦点切换可能产生 xterm focus tracking 数据；不要把该数据误当普通输入而清除选区或关闭菜单。
- 改变 `terminal.options.fontSize` 后必须重新计算 xterm 网格，并通过现有 resize 链路同步远端 PTY 的 cols/rows；否则 Vim、readline、表格和光标会错位。
- 一个用户请求若同时涉及“窗口缩放”和“终端缩放”，优先确认目标：前者影响整张 WebView，后者只影响焦点终端及其远端网格。两者不要共用 handler。

## 字体、图标与离线资源

- 所有字体、图标和基础样式必须随应用打包；禁止运行时依赖外部 CDN。
- 终端字体使用 `FILETERM_MONO_FONT_FAMILY`，并保留 `observeCanvasTextMetrics()` 触发的度量与 resize 同步。
- 调整 UI 字体时同时检查 CJK 回退、高 DPI、不同字重、行高和标题栏高度；不要只在 macOS 或开发态浏览器中目测。
- macOS tray 使用 template image；Dock 图标、tray 图标和窗口图标是不同资源，不要互相缩放复用。

## 实施顺序

1. 明确行为对象和三端预期；列出冲突表。
2. 找到当前平台来源、菜单入口、快捷键 handler、终端/xterm handler 与 Rust command。
3. 先删除或迁移旧功能入口，再实现新入口；避免旧应用级行为仍在后台抢事件。
4. 经由 `packages/core` 和 Tauri bridge 收敛新增系统能力；纯终端交互留在 `TerminalView`。
5. 在三个平台分别检查：菜单可见性、快捷键文案、实际按键、触控板/滚轮、焦点在输入框与终端时的差异、字体与布局。

## 验证

代码改动至少运行：

```bash
npm run typecheck -w @fileterm/tauri
npm run lint
npx prettier --check apps/tauri packages/core packages/shared packages/storage
npm run test:tauri
cargo clippy --manifest-path apps/tauri/src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings
```

涉及窗口、菜单、终端手势或快捷键时，还要在 macOS、Windows、Linux 手测对应焦点场景。报告时说明：行为归属、影响平台、冲突结论、改动层级和验证结果。
