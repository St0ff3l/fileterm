# FileTerm Integration Inventory

本文归总 FileTerm 当前已经接入的核心第三方项目、采用理由、实现位置和维护边界。它不是依赖清单的替代品；精确版本以 `apps/tauri/package.json`、`apps/tauri/src-tauri/Cargo.toml` 和 `package-lock.json` 为准。迁移前实现及其专用依赖已从仓库移除，本文不再列出。

## 1. 终端：xterm.js

### 已采用包

| 包                       | 当前用途                                    | 实现位置                                               | 维护结论                                                                                      |
| ------------------------ | ------------------------------------------- | ------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| `@xterm/xterm`           | 终端主体渲染、输入、selection、控制序列解析 | `apps/tauri/src/renderer/components/terminal-view.tsx` | 实时 PTY 数据必须原样交给 xterm 解析，renderer 不改写 `\r` / `\n` 控制流。                    |
| `@xterm/addon-fit`       | 根据容器尺寸计算行列数                      | `apps/tauri/src/renderer/components/terminal-view.tsx` | 配合 `ResizeObserver` 使用；resize 后必须把同一套 `cols/rows` 同步给后端 PTY。                |
| `@xterm/addon-search`    | 终端内搜索                                  | `apps/tauri/src/renderer/components/terminal-view.tsx` | 绑定 `Ctrl+Shift+F` 的终端搜索 UI；`Ctrl+F` 保留给远程 CLI，支持上一条/下一条、大小写和正则。 |
| `@xterm/addon-unicode11` | Unicode 11 宽字符支持                       | `apps/tauri/src/renderer/components/terminal-view.tsx` | 用于中文、Emoji、Powerline / Oh My Zsh 字符宽度计算，减少光标错位。                           |
| `@xterm/addon-web-links` | HTTP/HTTPS 链接识别                         | `apps/tauri/src/renderer/components/terminal-view.tsx` | 终端输出中的链接可点击并通过浏览器打开。                                                      |

### 当前终端实例配置

当前终端初始化保留这些关键配置：

```ts
allowProposedApi: true
scrollback: 6000
reflowCursorLine: false
scrollOnEraseInDisplay: false
// 仅本地 Windows/ConPTY 会设置：
windowsPty: {
  backend: 'conpty'
}
```

维护结论：

- `scrollback` 不能设为 `0`，否则历史输出和用户回看体验会退化。
- `allowProposedApi` 已保留，便于 xterm 内部 reflow / viewport 能力正常工作。
- `reflowCursorLine: false` 用于降低 readline 当前输入行在 resize 时被重新折行污染的概率。
- 本地 Windows PTY 使用 ConPTY 时显式设置 `windowsPty.backend`，避免 xterm 在恢复标签页或调整尺寸时丢失 ConPTY 重绘行；远程 SSH 即使登录的是 Windows，也不套用此配置。
- `scrollOnEraseInDisplay: false` 保持全屏 TUI 的 ED2 擦屏在当前视口完成，不把旧画面伪装成新的 scrollback。

标签页切换约束：

- 顶层本地和远程终端都由工作区 keep-alive 保留，不因切换标签页销毁 xterm 实例。
- 切回标签页时，先等待布局恢复，再强制同步尺寸、清理 texture atlas、刷新并聚焦；滚动条读取 xterm buffer，而不是读取已经被隐藏或替换的旧 DOM scrollTop。
- 普通滚轮事件交给 xterm 决定是 scrollback、alternate-screen 光标滚动还是远程 mouse-reporting，renderer 不为某一个 CLI 硬编码控制序列。

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

| 包 / 能力                | 当前状态           | 原因                                                                                                                 |
| ------------------------ | ------------------ | -------------------------------------------------------------------------------------------------------------------- |
| `@xterm/addon-webgl`     | 已撤回，不默认加载 | 本轮验证中会放大 selection、resize、TUI 重绘问题。先保证 PTY 控制流和尺寸同步正确，再单独评估硬件加速。              |
| `@xterm/addon-canvas`    | 未采用             | WebGL 未默认启用，因此暂不需要 Canvas fallback。                                                                     |
| `@xterm/addon-clipboard` | 未采用             | 当前复制/粘贴走 Tauri 暴露的剪贴板 API 与 xterm 自身 paste/selection 行为。                                          |
| `xterm-addon-zmodem`     | 未采用             | SSH shell 仍不把 `rz/sz` 二进制流混入终端通道；串口文件传输由 Rust 的独立 `zmodem2` 适配器负责，不依赖 xterm addon。 |

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
- 终端搜索、普通 selection 和标签页切换后的滚动恢复

## 2. 文件编辑器：Monaco Editor

### 已采用包

| 包                     | 当前用途   | 实现位置                                                       | 维护结论                                                   |
| ---------------------- | ---------- | -------------------------------------------------------------- | ---------------------------------------------------------- |
| `monaco-editor`        | 编辑器核心 | `apps/tauri/src/renderer/features/files/file-editor-modal.tsx` | 用于远程文件编辑，提供语言模式、查找、快捷键和编辑器主题。 |
| `@monaco-editor/react` | React 封装 | `apps/tauri/src/renderer/features/files/file-editor-modal.tsx` | 用 React 组件管理 Monaco 生命周期和 mount 回调。           |
| `opencc-js`            | 简繁转换   | `apps/tauri/src/renderer/features/files/file-editor-modal.tsx` | 对选中文本执行简体/繁体转换，不在协议层处理文本转换。      |

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
- 文本编码、文件保存、权限提升仍通过 Rust commands 暴露的文件能力，renderer 不直接访问远程协议 client。

## 3. 桌面壳与前端运行时

| 包                              | 当前用途                                       | 实现位置                                        | 维护结论                                                                   |
| ------------------------------- | ---------------------------------------------- | ----------------------------------------------- | -------------------------------------------------------------------------- |
| `@tauri-apps/api` / `tauri`     | Tauri 桌面窗口、Rust commands/events、系统能力 | `apps/tauri/src-tauri`, `apps/tauri/src/bridge` | renderer 只能经 `tauri-api.ts` 调用，不在 feature 中散落 `invoke/listen`。 |
| `react` / `react-dom`           | Tauri Renderer UI                              | `apps/tauri/src/renderer`                       | 仓库只有一套 renderer；不要在 feature 组件里散落运行时判断。               |
| `vite` / `@vitejs/plugin-react` | Renderer 构建与开发服务器                      | `apps/tauri/vite.config.ts`                     | 开发服务器固定 5188。                                                      |
| `typescript`                    | 类型检查与构建                                 | `apps/tauri/tsconfig.json`                      | 改动必须至少跑 `npm run typecheck`，再运行受影响 app 的测试/构建。         |

### 桌面壳资源和布局约定

- 桌面主图标的唯一源文件为 `apps/tauri/assets/icons/fileterm-1024.png`；Tauri 原生图标产物统一放在 `apps/tauri/src-tauri/icons/`，WebView 使用 `apps/tauri/public/icon.png` 副本。
- macOS 菜单栏托盘图标由 Tauri 维护：使用 `apps/tauri/src-tauri/icons/trayTemplate*.png` 与 Rust tray API；可编辑源文件位于 `apps/tauri/assets/icons/trayTemplate.svg`。
- Windows 应用图标由 `apps/tauri/src-tauri/icons/icon.ico` 提供；不要把 Windows app icon 缩放后当作 macOS menu bar template。
- 顶部标签栏、工作区焦点模式、侧栏收起状态和文件面板抽屉都是 renderer UI 状态；不要把这些布局状态扩散到 main service。
- 工作区切换动效复用 `page-card-in-up/down` 节奏，并通过 `prefers-reduced-motion` 关闭动画。
- 终端命令输入条是覆盖在 shell 区域上的半透明悬浮控件，终端内容区域不为它预留固定底部 padding。

## 4. 远程协议与文件传输

| 包                     | 当前用途                                                         | 实现位置                                       | 维护结论                                                                                                                       |
| ---------------------- | ---------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `russh` / `russh-sftp` | SSH shell、SFTP、远程命令与文件能力、SFTP offset 续传            | `apps/tauri/src-tauri/src/sessions/ssh/`       | 协议层只在 Rust 侧运行；renderer 经 `tauri-api.ts` 调用。注意 `vendor/russh` 保留了旧 Comware 兼容分支，升级时必须保留该边界。 |
| `suppaftp`             | FTP / 显式 FTPS / 隐式 FTPS 会话、文件操作和断点续传             | `apps/tauri/src-tauri/src/sessions/ftp/mod.rs` | FTP 与 SSH/SFTP 在 session/protocol 层保持物理分离，不做伪统一。                                                               |
| `iconv-lite`           | 文件内容编码处理                                                 | 文件读写相关链路                               | 编码处理属于文件读写链路，不放进 UI 组件零散处理。                                                                             |
| `tokio-serial`         | Windows COM、macOS/Linux `/dev/*` 串口打开与读写、调制解调器状态 | `apps/tauri/src-tauri/src/sessions/serial/`    | 设备参数、权限、句柄生命周期和 X/Y/ZMODEM、Kermit 传输都在 Rust；renderer 仅经 bridge 接收终端字节和传输进度。                 |

### SSH 终端约定

当前 SSH shell 创建时使用：

```ts
term: 'xterm-256color'
```

维护结论：

- 后端 PTY resize 需要和前端 xterm resize 保持同一套 `cols/rows`。
- 如果后续补 `COLORTERM=truecolor`，应在 SSH shell / 会话环境边界统一处理，并记录到本文件。
- 不要为了 SSH 文件传输把 zmodem 二进制流塞进 shell 通道；SSH 优先使用已有 SFTP/FTP transfer system。串口文件传输只从 Serial 的专用传输面板进入。
- SOCKS5 / HTTP CONNECT 代理 socket 在 Rust SSH session 的 `transport/proxy.rs` 中创建，由 `transport.rs` facade 组装。认证密码绝不进入 renderer snapshot。
- Telnet/Serial 是 terminal-only session，不能接入 SFTP、exec、CWD、sudo 或资源监控。

## 5. 工作区内部包

| 包                  | 当前用途           | 维护结论                                                  |
| ------------------- | ------------------ | --------------------------------------------------------- |
| `@fileterm/core`    | 领域类型和核心模型 | 新状态优先进入 core，再下沉到 main services 和 renderer。 |
| `@fileterm/storage` | 存储抽象           | 敏感信息和持久化策略不要散落在 UI 组件。                  |
| `@fileterm/shared`  | 共享常量与轻量工具 | 只放跨层稳定共享内容，避免变成杂物包。                    |

## 6. 新依赖准入规则

新增或替换第三方项目时，至少补齐这些信息：

1. 在对应 `package.json` 添加依赖。
2. 在本文件登记用途、实现位置、维护边界。
3. 如果涉及终端、文件传输、协议、安全或发布，补充 `docs/quality/` 下的回归清单。
4. 如果改变 `Rust commands/events -> bridge -> renderer` 边界，同步更新 `docs/architecture.md` 或 `docs/decisions/`。
5. 跑 `npm run typecheck`；涉及 Rust 后端时再跑 `npm run test:tauri`，以及 Tauri 的生产构建。
