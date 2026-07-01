# FileTerm Integration Inventory

本文归总 FileTerm 当前已经接入的核心第三方项目、采用理由、实现位置和维护边界。它不是依赖清单的替代品；精确版本仍以 `apps/desktop/package.json` 和 `package-lock.json` 为准。

## 1. 终端：xterm.js

### 已采用包

| 包 | 当前用途 | 实现位置 | 维护结论 |
| --- | --- | --- | --- |
| `@xterm/xterm` | 终端主体渲染、输入、selection、控制序列解析 | `apps/desktop/src/renderer/components/TerminalView.tsx` | 实时 PTY 数据必须原样交给 xterm 解析，renderer 不改写 `\r` / `\n` 控制流。 |
| `@xterm/addon-fit` | 根据容器尺寸计算行列数 | `TerminalView.tsx` | 配合 `ResizeObserver` 使用；resize 后必须把同一套 `cols/rows` 同步给后端 PTY。 |
| `@xterm/addon-search` | 终端内搜索 | `TerminalView.tsx` | 绑定 `Cmd/Ctrl+F` 的终端搜索 UI，支持上一条/下一条、大小写和正则。 |
| `@xterm/addon-unicode11` | Unicode 11 宽字符支持 | `TerminalView.tsx` | 用于中文、Emoji、Powerline / Oh My Zsh 字符宽度计算，减少光标错位。 |
| `@xterm/addon-web-links` | HTTP/HTTPS 链接识别 | `TerminalView.tsx` | 终端输出中的链接可点击并通过浏览器打开。 |

### 当前终端实例配置

当前终端初始化保留这些关键配置：

```ts
allowProposedApi: true
scrollback: 6000
reflowCursorLine: false
```

维护结论：

- `scrollback` 不能设为 `0`，否则历史输出和用户回看体验会退化。
- `allowProposedApi` 已保留，便于 xterm 内部 reflow / viewport 能力正常工作。
- `reflowCursorLine: false` 用于降低 readline 当前输入行在 resize 时被重新折行污染的概率。

### 尺寸同步原则

FileTerm 当前采用“拖拽期间冻结列数，稳定后同步真实宽度”的策略：

- 本地 `terminal.resize(cols, rows)` 和后端 PTY resize 必须使用同一个 `cols`。
- 平稳状态下，列数跟随 `fitAddon.proposeDimensions()` 的真实可见宽度计算，只保留少量 guard cols。
- 用户横向拖拽窗口时，暂时冻结上一帧 `cols`，避免 `nano/vim`、bash/readline、多行进度条在拖拽过程中连续重排。
- 拖拽停止后，再把真实宽度对应的 `cols` 一次性同步给本地 xterm 和后端 PTY。
- 行数继续来自 `fitAddon.proposeDimensions()`，但保留 1 行安全余量，避免 `nano/vim` 底部菜单和文件面板边界互相挤压。

这条边界非常重要。不要恢复成：

```txt
前端 xterm 一个 cols
后端 PTY 另一个 cols
```

这种分裂状态会导致 bash/readline 上下键历史记录“吃上去”，也会影响 `nano/vim` 的菜单、状态栏和光标定位。

### 未采用或已撤回项

| 包 / 能力 | 当前状态 | 原因 |
| --- | --- | --- |
| `@xterm/addon-webgl` | 已撤回，不默认加载 | 本轮验证中会放大 selection、resize、TUI 重绘问题。先保证 PTY 控制流和尺寸同步正确，再单独评估硬件加速。 |
| `@xterm/addon-canvas` | 未采用 | WebGL 未默认启用，因此暂不需要 Canvas fallback。 |
| `@xterm/addon-clipboard` | 未采用 | 当前复制/粘贴走 Electron preload 暴露的剪贴板 API 与 xterm 自身 paste/selection 行为。 |
| `xterm-addon-zmodem` | 未采用 | FileTerm 文件传输已走 SFTP/FTP 文件面板，不把 `rz/sz` 二进制流混入终端通道。 |

### 回归文档

终端相关改动必须参考：

- `docs/quality/terminal-regression-checklist.md`
- `docs/quality/terminal-layout-notes.md`

尤其要复测：

- `nano`
- `vim`
- 单行 `\r` 进度条
- 三行进度条 + 拖拽窗口
- bash/readline 上下键历史记录
- 终端搜索和普通 selection

## 2. 文件编辑器：Monaco Editor

### 已采用包

| 包 | 当前用途 | 实现位置 | 维护结论 |
| --- | --- | --- | --- |
| `monaco-editor` | 编辑器核心 | `apps/desktop/src/renderer/features/files/FileEditorModal.tsx` | 用于远程文件编辑，提供语言模式、查找、快捷键和编辑器主题。 |
| `@monaco-editor/react` | React 封装 | `FileEditorModal.tsx` | 用 React 组件管理 Monaco 生命周期和 mount 回调。 |
| `opencc-js` | 简繁转换 | `FileEditorModal.tsx` | 对选中文本执行简体/繁体转换，不在协议层处理文本转换。 |

### 当前 Monaco 能力

- 自定义 `fileterm-default-dark` 主题，与 FileTerm 深色界面保持一致。
- `Cmd/Ctrl+S` 保存当前文件内容。
- `Cmd/Ctrl+F` 触发 Monaco 自带查找，而不是终端搜索。
- 支持语言列表读取与 model language 切换。
- 支持行号开关、自动换行、空白字符显示、Tab size 等编辑器选项。
- 支持编码字段与保存时编码传递。
- 编辑器窗口采用左侧文件树、右侧编辑区布局，当前文件节点可聚焦回 Monaco。
- Monaco 主题颜色从 FileTerm CSS 变量读取，避免编辑器色值游离于主题系统之外。

维护结论：

- 终端搜索和文件编辑器搜索是两套不同入口：终端在 `TerminalView`，文件编辑器在 `FileEditorModal`。
- 不要把 Monaco 的 `Cmd/Ctrl+F` 交给全局终端搜索拦截。
- 文本编码、文件保存、权限提升仍通过 main/preload 暴露的文件能力，renderer 不直接访问远程协议 client。

## 3. 桌面壳与前端运行时

| 包 | 当前用途 | 实现位置 | 维护结论 |
| --- | --- | --- | --- |
| `electron` | 桌面窗口、main/preload/renderer 边界、剪贴板、系统能力 | `apps/desktop/src/main`, `apps/desktop/src/preload` | Renderer 不直接访问系统和协议能力；统一走 preload 暴露的安全 API。 |
| `electron-builder` | macOS / Windows 打包 | `apps/desktop/package.json`, `docs/quality/release-beta-mac.md` | 发布产物通过 `npm run release:mac` / `npm run release:win` 生成。 |
| `react` / `react-dom` | Renderer UI | `apps/desktop/src/renderer` | 当前 UI 主要由 React state 和 feature components 组织。 |
| `vite` / `@vitejs/plugin-react` | Renderer 构建与开发服务器 | `apps/desktop/vite.config.ts` | `npm run dev` 同时启动 Vite、main watch 和 Electron。 |
| `typescript` | 类型检查与构建 | `apps/desktop/tsconfig*.json` | 修改跨层 API 时必须跑 `npm run typecheck -w @fileterm/desktop`。 |

### 桌面壳资源和布局约定

- macOS 菜单栏托盘图标使用 `apps/desktop/build/trayTemplate.png` 与 `trayTemplate@2x.png`，并在 main process 中继续调用 `trayImage.setTemplateImage(true)`。
- Windows 应用图标继续使用 `apps/desktop/build/icon.ico`，不要把 Windows app icon 缩放后当作 macOS menu bar template。
- 顶部标签栏、工作区焦点模式、侧栏收起状态和文件面板抽屉都是 renderer UI 状态；不要把这些布局状态扩散到 main service。
- 工作区切换动效复用 `page-card-in-up/down` 节奏，并通过 `prefers-reduced-motion` 关闭动画。
- 终端命令输入条是覆盖在 shell 区域上的半透明悬浮控件，终端内容区域不为它预留固定底部 padding。

## 4. 远程协议与文件传输

| 包 | 当前用途 | 实现位置 | 维护结论 |
| --- | --- | --- | --- |
| `ssh2` | SSH shell、SFTP、远程命令/文件能力 | `apps/desktop/src/main/services/sessions/ssh-session-controller.ts` | SSH/SFTP 只在 main process 运行；renderer 通过 IPC 调用。 |
| `basic-ftp` | FTP 会话和文件操作 | `apps/desktop/src/main/services/sessions/ftp-session-controller.ts` | FTP 和 SSH/SFTP 在 controller/protocol 层保持分离。 |
| `iconv-lite` | 文件内容编码处理 | main / file service 相关链路 | 编码处理属于文件读写链路，不放进 UI 组件零散处理。 |

### SSH 终端约定

当前 SSH shell 创建时使用：

```ts
term: 'xterm-256color'
```

维护结论：

- 后端 PTY resize 需要和前端 xterm resize 保持同一套 `cols/rows`。
- 如果后续补 `COLORTERM=truecolor`，应在 SSH shell / 会话环境边界统一处理，并记录到本文件。
- 不要为了文件传输把 zmodem 二进制流塞进 shell 通道；优先使用已有 SFTP/FTP transfer system。

## 5. 工作区内部包

| 包 | 当前用途 | 维护结论 |
| --- | --- | --- |
| `@fileterm/core` | 领域类型和核心模型 | 新状态优先进入 core，再下沉到 main services 和 renderer。 |
| `@fileterm/storage` | 存储抽象 | 敏感信息和持久化策略不要散落在 UI 组件。 |
| `@fileterm/shared` | 共享常量与轻量工具 | 只放跨层稳定共享内容，避免变成杂物包。 |

## 6. 新依赖准入规则

新增或替换第三方项目时，至少补齐这些信息：

1. 在对应 `package.json` 添加依赖。
2. 在本文件登记用途、实现位置、维护边界。
3. 如果涉及终端、文件传输、协议、安全或发布，补充 `docs/quality/` 下的回归清单。
4. 如果改变 main/preload/renderer 边界，同步更新 `docs/architecture.md` 或 `docs/decisions/`。
5. 跑 `npm run typecheck -w @fileterm/desktop`；涉及打包或构建链路时再跑 `npm run build -w @fileterm/desktop`。
