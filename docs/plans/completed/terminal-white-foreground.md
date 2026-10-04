# 终端配色、UI 对比度与 2.2.21 崩溃日志核对

日期：2026-10-04（Asia/Kuala_Lumpur）

状态：实现完成，自动化验证结果见下；用户 Windows/原生三平台复测仍待完成。

## 问题与证据

用户截图显示 Claude Code 提问标题在亮色主题下消失、暗色主题下为纯白。内置 FileTerm/Codex 的亮色 ANSI white/brightWhite 仍是 `#f1f5f9` / `#ffffff`。本机 Claude Code 的提问组件使用 `text` 色，其暗色 RGB 模式为 `rgb(255,255,255)`，暗色 ANSI 模式为 `ansi:whiteBright`；截图本身不能判定该次输出具体使用哪种模式。

亮色 Codex + 实际 xterm 浏览器对照夹具重现了粗体中文 RGB 白色标题显示为 `rgb(255,255,255)`。终端不是按内容含义分类着色：ANSI 索引使用主题调色板，RGB 真彩色直接使用程序指定值。

另外用户 ERROR.log 的单条 Windows 2.2.21 React #185，通过同版本发布包中的 JS 资源精确定位为设置页 DropdownSelect 的布局 state 更新。该位置已随 2.2.22 修复，详见 [React 更新循环排查](../../quality/react-update-loop-audit.md)。本次无需再次修改该组件，单条日志也不足以判断版本总体崩溃率。

## 实现边界

- `packages/core`：修正内置亮色 ANSI white/brightWhite 为 `#374151` / `#111827`，经现有主题变量链传给 xterm。不修改暗色默认值、导入主题或自定义 ANSI 配置。
- `renderer/app/terminal-foreground-adapter.ts`：xterm 解析后，只将默认背景、非反色 cell 上的 RGB 纯白前景改为默认前景模式。保留字符、宽度、粗体等样式；不动其他 RGB 色、显式背景配对、反色模式、远端输出、原始 transcript、日志或 parser 的当前 SGR 属性。
- `terminal-lifecycle-core.ts`：注册/释放适配器。适配正常/备用屏幕、写入、滚动、resize 与切换 buffer；只扫描可见行，历史行在滚入视口时适配。切换主题后现有文字由 xterm 默认前景自动更新。
- 不改变 Rust command/event、bridge、PTY、终端输入或 React state；行为适用于所有平台。

缓冲区写入沿用现有 TerminalLogColorizer 对 xterm 6 私有 `_line._data` 的受控访问方式，检查 TypedArray 和 cell 存储布局；升级 xterm 时必须重新跑该回归。RGB 纯白并没有携带“普通文字”语义，此规则会使所有符合上述条件的纯白前景跟随主题；若程序需要保留白色配对，应显式设置背景。这里只做纯白兼容，不对近白、灰色或其他真彩色执行自动分类。

## 补充对比度检查与修复

用户进一步要求检查“黑底暗字、浅底浅字”。检查 FileTerm、Codex、六种 iTerm2 preset 的 16 个明暗变体，覆盖普通/次要/弱化文字与 panel、输入框、菜单、主/危险按钮的默认和 hover token，以及终端调色板、选区和搜索色对。该检查针对共享配色与终端，不代表所有业务页面和交互状态的完整可访问性审核。

发现并修复：

- 终端暗色 ANSI black、亮色 brightYellow/brightCyan，以及固定 RGB 黑色/浅灰色，与当前底色过近。`terminal-lifecycle-core.ts` 开启 xterm 自带的 `minimumContrastRatio: 4.5`。保护正常文字显示，覆盖 16/256 色与 RGB、显式背景、反色、选区和搜索；不会改写 SGR 原始属性。dim 仍按 xterm 规则弱化，hidden 和图形块背景不被强制变为普通文字。
- 7 个 preset token 文件中共 12 组低对比度选区/搜索配对。Nord 暗色选区由 1.17 提升为 10.84；Catppuccin 暗色选区由 1.14 提升为 12.95；Gruvbox 亮色选区由 1.00 提升为 10.22。Solarized 亮色搜索原先前景与背景均为 `#657b83`，Tokyo Night 亮色搜索仅 1.21。修正这些色对及其余低于 4.5 的 preset 配对，保留选区/搜索背景。
- Codex 亮色主按钮为白字配浅蓝，原始对比度约 2.86。新增 `action-text-color.ts` 为主按钮和危险按钮选择更易读的白字或深字；保留已有背景色，支持 3/4/6/8 位 hex 和 alpha。参考层、语义变量与历史按钮/确认弹窗 alias 使用同一结果；hover/active 保持同一文字色。
- 暗色皮肤全局控件 `color: inherit` 的优先级高于公用 Button，实际夹具重现自定义白底按钮继承 `#fcfcfc`，对比度约 1.03。将该原生控件兜底改为 `:where` 的零优先级规则，恢复公用组件自己的语义颜色，没有新增 `!important`。

按钮选择以背景和 panel surface 为依据，保留配色配置；不对自定义主题的所有文字、任意透明层叠或每个业务组件执行自动重写。按钮 hover/active 的既有背景可以使对比度变化，回归验证其文字色稳定且没有重新落入近色不可读状态，不宣称所有 UI 状态均达到 4.5。

## 本地终端浅色左边界

用户后续截图显示本地终端浅色主题的左留边不可辨识。`LocalTerminalFrame` 的左留边原来使用 `border-left`，线画在窗口外缘；右留边却使用 `border-left`，线画在终端内缘。暗色底色和阴影掩盖了这种不对称，浅色底色接近时则只剩右侧分隔线。

将左留边改为 `border-right`，两侧均在靠终端的一边画分隔线，继续使用现有 `--terminal-right-frame-accent`。frame 的相关样式从遗留 `session.css` 移入 `features/workspace/local-terminal-frame.css`，由组件引入；保留 15px 留边、底部 frame 和终端尺寸。不修改主题背景、Tauri 窗口或导航侧栏。

临时浏览器夹具使用实际 `LocalTerminalFrame`、完整主题 CSS 和运行时主题配置，先以原规则复现缺少左内侧线，再验证修改后左右线色一致、均位于终端边缘、15px 留边和终端坐标不变。Chromium/WebKit 各通过 16 个主题变体 × macOS/Windows/Linux 的 48 组样式检查；这是浏览器中的平台样式验证，不是三端原生窗口手测。修改后重新通过类型检查、Lint、Prettier、CSS contract 和 Renderer 生产构建。Rust 源码没有改变，沿用本任务此前的测试与 Clippy 结果。

## 验证

- `tests/browser/terminal-theme-foreground.mjs`：实际 xterm + 主题 CSS/runtime + 日志着色器；对照重现白底纯白，检查 RGB/ANSI、分片/冒号 SGR、粗体中文、正常/备用屏幕、暗亮切换与自定义前景、显式背景/反色/灰色/红色、滚动历史和 dispose。
- `tests/browser/popup-update-stability.mjs`：复核已修复的下拉框定位重复更新。夹具等待初始 effect/自适应箭头的 ResizeObserver 更新停止后再计数；最多等待 12 帧，不能用无限等待掩盖更新循环。打开菜单和 100 次父更新的提交次数断言保持不变。
- 配色与下拉框夹具均在 Chromium 和 WebKit 通过。
- `tests/browser/terminal-sidebar-interactions.mjs`：Chromium/WebKit 均通过实际 xterm 的 40 次粘贴、360 次菜单打开期间的父更新及 40 次侧栏切换，没有 React 错误。
- `tests/browser/terminal-contrast.mjs`：实际 production terminal runtime（非复制构造选项），在 Chromium/WebKit 各通过 9,218 个对比度检查；覆盖 16 个变体、正常/备用屏幕、全部 16 ANSI 与 256 索引色、黑白灰 RGB、显式/反色同色背景、选区、SearchAddon 和无新输出的自定义主题切换。ratio=1 的关闭保护对照先复现同色不可见；检查原始 RGB 属性未改写，以及 dim/hidden/块字符保留。
- `tests/browser/action-text-contrast.mjs`：真实公用 Button + 完整主题 CSS/runtime，在 Chromium/WebKit 各通过 66 项；包括 Codex 明暗、白/黄/透明/黑自定义背景、主/危险按钮默认/hover/active。正常状态验证约 4.5 的对比度，hover/active 至少 3，文字色不随状态跳变。
- 补充修复后重新通过类型检查、Lint、Prettier、36 个 Renderer 测试与 Renderer 生产构建。683 个 Rust 单元测试与 31 个集成/contract 测试、Clippy（locked/all-targets/all-features/-D warnings）在本任务首次修复后通过；补充修复没有改变 Rust 源码，沿用该结果。
- CSS contract 通过；原有 11 处 `!important` 和约 1 处遗留直接颜色警告未增加。
- 浏览器验证不能替代 Windows/macOS/Linux 原生打包应用与用户实际 Claude Code 场景；尚未发版，仍待实际环境复测。

## 文件规模

TerminalLifecycleCore 为非豁免业务文件，307 → 314 行；ThemeConfig 为主题映射聚合，950 → 953 行；新增纯白适配器 51 行、按钮文字色 helper 34 行。LocalTerminalFrame 为 12 → 13 行，样式从 session.css 迁出 75 行并与组件共址。`packages/core/src/index.ts` 为类型与常量聚合文件，2927 → 2929 行，属于豁免类别；样式、测试和文档也属于豁免类别。没有触发需要拆分的业务源文件。
