---
name: multiplatform-desktop-skill
description: 为 Electron + React + TypeScript 桌面应用处理多平台兼容性、标题栏/托盘行为、窗口关闭链路、平台字体与资源打包边界的实战技能。只要用户提到 macOS、Windows、Linux、跨平台兼容、系统托盘、Dock、原生标题栏、自定义标题栏、快捷键退出、平台差异、平台专用图标、字体在不同系统是否一致、或要求“检查这段改动会不会影响其他平台”，就应该使用这个 skill。尤其适用于 FileTerm 这类 main/preload/renderer 分层的 Electron 项目。
compatibility:
  tools:
    - shell
    - apply_patch
  patterns:
    - electron-main-preload-renderer
    - platform-aware-ui
    - packaged-resource-validation
---

# FileTerm 多平台开发技能

这个 skill 用来处理 FileTerm 这类 Electron 桌面应用里的跨平台细节，不是泛泛的“做适配”，而是把 `main / preload / renderer / assets / build` 这几层一起看，避免某个平台修好了、另一个平台悄悄回归。

## 先做什么

遇到多平台任务时，先确认这件事属于哪一层：

1. `main`
   - `BrowserWindow` 参数
   - `Tray` / `Dock` / 应用生命周期
   - 快捷键和关闭行为
   - 资源路径、打包资源、平台探测
2. `preload`
   - 暴露平台信息和系统能力
   - renderer 不能直接猜系统行为时，需要从这里过桥
3. `renderer`
   - 平台条件 UI
   - 标题栏、按钮、布局避让
   - 字体、图标、文案和交互反馈
4. `assets / build`
   - 托盘图标、Dock 图标、窗口图标
   - 字体来源和离线可用性
5. `docs`
   - 新增平台边界后，要把稳定事实写回文档，而不是只留在代码里

如果任务跨越多层，先读这些文件：

- `AGENTS.md`
- `docs/architecture.md`
- `docs/plans/active/multiplatform-system-observability.md`
- `apps/desktop/src/main/main.ts`
- `apps/desktop/src/preload/preload.cts`
- `apps/desktop/src/renderer/App.tsx`
- `apps/desktop/src/renderer/styles/features/shell.css`
- `apps/desktop/src/renderer/styles/features/workstation-skin.css`

## FileTerm 当前已经明确的多平台边界

### 1. 平台判断

- 统一从 `preload` 暴露的 `window.fileterm.platform` 读取平台。
- renderer 可以把平台同步到 `document.documentElement.dataset.platform`，再让 CSS 做差异化。
- 不要在 renderer 里散落 `navigator.platform`、UA 猜测、或手写字符串分支。
- 不要把平台布尔值硬编码成常量。近期就出现过 `isWindowsDesktop = false` 这种回归，导致一整条 Windows UI 分支失效。

### 2. 系统能力必须走 `main -> preload -> renderer`

- 托盘、Dock、窗口控制、原生关闭链路都属于系统能力。
- renderer 不直接控制系统 API，只通过 preload 暴露的能力调用。
- 关闭确认这类行为，真正的退出决定权必须在 `main`，renderer 只负责展示确认 UI。

### 3. macOS 与 Windows 标题栏不是一套东西

- macOS 主窗口优先走原生窗口语义：
  - `titleBarStyle: 'hiddenInset'`
  - `trafficLightPosition`
  - 左上角要为红黄绿按钮留避让空间
- Windows 可以用自定义标题栏或额外 menubar 区，但必须配合：
  - 拖拽区 `-webkit-app-region: drag`
  - 可点击按钮 `-webkit-app-region: no-drag`
  - 布局上给内容区重新排网格，避免 menubar 挤压主工作区
- 不要把 macOS 的留白规则直接套到 Windows，也不要把 Windows 的自绘 menubar 强加给 macOS。

### 4. 托盘图标和应用图标必须分开

- macOS 菜单栏托盘图标应使用单独的 template image。
- 大尺寸彩色 app icon 不能直接缩成菜单栏图标。
- 当前 FileTerm 的正确方向是：
  - `Dock icon` 继续用彩色应用图标
  - `Tray icon` 在 macOS 下优先走独立 `trayTemplate.png`
  - `trayImage.setTemplateImage(true)`
- 如果用户反馈“托盘图标发白”“看不清”，优先排查是不是误用了应用图标，而不是先怀疑系统主题。

### 5. 退出链路必须收敛成一条

- `Cmd+Q`
- 托盘菜单“退出”
- 关闭窗口按钮
- 其他可能触发 `app.quit()` 的入口

这些入口最终都应该汇聚到同一条退出决策链，而不是一部分直接 `app.quit()`、另一部分先确认。

在 FileTerm 里，退出确认应该遵守：

- `main` 拦截 `before-quit`
- `main` 通过事件通知 renderer 请求确认
- renderer 展示确认弹窗
- renderer 再调用 preload API 回传 `quit / hide / cancel`
- `main` 最终执行退出或隐藏

如果任意入口绕过这条链，就会出现“某个快捷键弹确认、某个菜单直接退出”的不一致。

## 字体与品牌字标的具体规则

### Windows 能不能保留 `Outfit`

可以，但要区分“能显示”和“可控可交付”。

当前 FileTerm 的 `Outfit` 来自 `apps/desktop/index.html` 里的 Google Fonts：

- 开发态和联网环境下，Windows 可以正常显示
- macOS 也可以
- 只要浏览器内核拿到了 webfont，平台本身是否预装 `Outfit` 不重要

但有三个现实边界必须记住：

1. 它不是系统内置字体
   - Windows 不会“保存”这款字体到系统
   - 只是 Electron 渲染这次页面时下载并使用
2. 它依赖外网字体服务
   - 离线打包
   - 公司网络限制
   - 中国大陆网络环境
   这些情况下都可能回退到 fallback 字体
3. 品牌一致性如果是硬要求，不能只靠 Google Fonts
   - 需要把字体文件随应用打包
   - 或至少准备本地 `@font-face`

### 什么时候必须改成本地打包字体

满足任意一条就应该改：

- 你希望 macOS / Windows / Linux 打包后视觉稳定一致
- 应用需要离线可用
- 目标用户网络环境不稳定
- 这是品牌字标，不希望 fallback 后气质跑掉

### 当前 FileTerm 对字体的正确理解

- `Outfit` 已经被引入，说明“品牌字标设计”方向是成立的
- 左上角 `FileTerm` 的字标样式曾经被后续兼容性改动覆盖过，说明：
  - 字体资源在，不代表视觉还在
  - 样式回归要同时检查 `font-family`、`font-weight`、`letter-spacing`、`margin-left`
- 只修 JSX 不够，品牌字标通常是“资源 + CSS + 平台留白”三件事一起成立

## 做多平台改动时的检查清单

每次改完，至少按下面顺序自查。

### A. 平台探测链

- `main` 是否只在一处定义平台行为
- `preload` 是否暴露了 renderer 真正需要的平台信息
- renderer 是否使用统一平台来源
- CSS 是否通过 `data-platform` 或稳定 class 区分平台

### B. 资源路径链

- 开发态能否找到资源
- 打包态能否找到资源
- 是否同时考虑了 `build/`、`public/`、`dist/`
- 是否为平台专用资源提供 fallback

### C. 退出与窗口行为

- `Cmd+Q` / `Ctrl+Q`
- 托盘退出
- 关闭主窗口
- 关闭子窗口
- `window-all-closed`
- `before-quit`

确认这些行为不会互相打架，不会出现重复弹窗、直接退出、或子窗口悬挂。

### D. 标题栏与布局

- macOS 左上角避让是否正确
- Windows 自绘标题栏是否把主布局挤坏
- 拖拽区和按钮点击区是否冲突
- 独立窗口和主窗口是否使用了不同但一致的标题栏语义

### E. 视觉与字体

- 品牌字标是否仍使用预期字体族
- fallback 字体出现时是否仍能接受
- 高 DPI 下图标和文字是否发虚
- 托盘、Dock、窗口图标是否各自适配

### F. 验证命令

最少跑：

```bash
npm run typecheck -w @fileterm/desktop
```

涉及构建、资源路径、打包资源时再跑：

```bash
npm run build -w @fileterm/desktop
```

如果改的是窗口、托盘、标题栏、退出链路，必须手测：

1. macOS 主窗口打开/隐藏/退出
2. macOS 托盘点击和菜单退出
3. `Cmd+Q`
4. Windows 标题栏按钮
5. Windows 自绘 menubar 是否压坏布局

## FileTerm 近期兼容性改动摘要

这是这个 skill 在处理相关需求时应该优先记住的上下文：

1. 托盘图标链路已开始分平台
   - macOS 托盘不再适合直接复用应用图标
   - 已新增独立 `trayTemplate` 资源方向
2. 退出确认链路已统一
   - `Cmd+Q` 和托盘退出不应再各走各的
3. 品牌字标样式发生过回归
   - `Outfit` 资源仍在
   - 但标题栏样式曾被 Windows 兼容改动覆盖
4. 标题栏兼容性改动很容易互相踩
   - 尤其是 macOS `hiddenInset`、Windows 自绘 menubar、独立窗口标题栏这三条线
5. 平台布尔值回归风险真实存在
   - 不要把 `isWindowsDesktop` 之类状态写死

## 写代码时的工作方式

处理这类任务时，按下面顺序推进：

1. 先定位平台差异发生在哪一层
2. 再确认是不是资源问题、样式问题，还是生命周期问题
3. 优先收敛共享链路
   - 一个退出入口
   - 一个平台探测来源
   - 一套平台样式开关
4. 保持平台专用资源分离
   - tray icon 不等于 app icon
   - macOS 标题栏不等于 Windows 标题栏
5. 最后补验证和文档

## 产出格式

当你使用这个 skill 回答时，优先给出：

1. 这是资源问题 / 样式问题 / 生命周期问题 / 平台分支问题中的哪一种
2. 影响平台
3. 建议修改层级
4. 具体注意事项
5. 需要跑的验证

如果需要做代码审查，优先指出：

- 哪个改动会伤到另一平台
- 哪个入口绕过了统一链路
- 哪个资源只在开发态能找到、打包态会丢
- 哪个 UI 是“看起来跨平台”，其实是把某个平台语义硬套到另一个平台

## 不要这样做

- 不要在 renderer 直接访问系统 API
- 不要把托盘、Dock、关闭逻辑散落到多个互不一致的入口
- 不要把平台差异写成组件里的零散魔法数字而不解释来源
- 不要默认 Google Fonts 在所有打包环境都稳定可用
- 不要把“开发机上看起来正常”当成“打包后跨平台稳定”
