# FileTerm Design Guide

本文档记录当前 FileTerm 工作台页面的设计语言。它不是营销页规范，而是桌面远程工作台的产品界面规范，用来指导后续终端、系统信息、文件管理、命令输入和多标签工作区的 UI 改动。

## 1. Design Direction

FileTerm 的视觉目标是：像原生桌面工具一样安静、清晰、密集、可靠。页面应优先服务长期使用和快速扫读，而不是展示感。

核心关键词：

- 桌面感：窗口、标签、侧栏、表格和状态栏都应贴近 macOS / Windows 桌面工具的秩序。
- 操作密集：终端、系统指标、文件列表、传输状态要能同时存在，不用大面积留白撑场面。
- 浅色主调：浅色模式以白色和浅灰为主体，边界靠 1px border、弱阴影和表头底色区分。
- 终端优先：主视区永远先保证终端可读性，其他面板为辅助信息。
- 状态克制：蓝、绿、黄、红只用于交互或状态含义，不做大面积装饰。

## 2. Layout

工作台采用三层桌面结构：

1. 顶部标签栏：高度约 48px，承载会话 tab、新建 tab、窗口级设置入口。
2. 左侧系统侧栏：宽度约 214-256px，展示连接信息、系统指标、进程、网络、磁盘。
3. 主工作区：上方为终端，下方为文件管理面板，中间通过可拖拽分隔条调整。

布局规则：

- 页面必须占满窗口，避免主体内容出现页面级滚动。
- 终端区域是主焦点，文件面板默认在底部，不能抢占终端视觉重心。
- 侧栏信息按「身份信息 -> 系统信息 -> 进程 -> 网络 -> 磁盘」排序。
- 底部状态栏高度保持低存在感，只显示任务运行状态、链接和系统状态。
- 多标签栏使用直角或小圆角的桌面 tab，不使用网页式胶囊导航。

## 3. Color System

颜色必须走主题变量，不允许在组件里散落硬编码色值。当前主题链路是：

```txt
tokens -> theme vars -> component skins -> terminal colors
```

浅色主题基准：

| Role | Token | Value / Usage |
| --- | --- | --- |
| App background | `--bg-main` | `#f4f5f7`，窗口和工作区底色 |
| Sidebar / chrome | `--bg-sidebar` | `#ffffff`，左侧栏和顶部栏 |
| Card / panel | `--bg-card` | `#ffffff`，终端外围、表格面板、弹层 |
| Hover | `--bg-hover` | `#eceff3`，导航 hover、轻按钮 hover |
| Active | `--bg-active` | `#dde2e8`，更强的选中背景 |
| Main text | `--text-main` | `#1f2933`，正文和关键标签 |
| Muted text | `--text-muted` | `#66717f`，辅助信息 |
| Soft text | `--text-soft` | `#7b8794`，弱辅助信息 |
| Border | `--border-light` | 低对比 1px 分割线 |
| Primary | `--primary` | 只用于主要动作、链接、焦点环 |

状态色规则：

- Success / online / SSH：使用 `--success` 或 `--type-ssh`，面积要小。
- Warning / memory pressure：使用 `--warning` 或 `--memory-warn`，只作为点、条、数字提示。
- Danger / destructive：使用 `--danger`、`--danger-text`、`--danger-surface`，只用于删除、关闭、错误。
- Info / action blue：使用 `--primary`、`--copy-link`、`--folder-accent`，不能随手写新的蓝色。
- 侧栏当前项在暗色模式使用白色强调，浅色模式使用主文本色，避免蓝色过度出现。

## 4. Typography

字体体系：

- UI 字体：`SF Pro Text`, `PingFang SC`, `Microsoft YaHei`, `Segoe UI`, sans-serif。
- 数字和终端周边：`SF Mono`, `JetBrains Mono`, `Menlo`, `Consolas`, monospace。
- 终端内容由 xterm 主题控制，必须保证等宽、行高稳定、选择态可见。

字号建议：

- 标签栏、按钮、表头：11-13px。
- 侧栏指标和文件列表：12-14px。
- 页面标题和品牌：15-21px。
- 终端和命令输入：以可读性优先，行高保持稳定，不随窗口宽度缩放。

排版规则：

- 表格和指标文字要能单行扫读，长文本省略，不挤压相邻列。
- 数字、路径、权限、owner/group 等使用等宽或接近等宽的显示节奏。
- 不使用负字距，不用 viewport width 驱动字号。

## 5. Surfaces And Borders

FileTerm 的层级主要靠边界，不靠重装饰。

- 外层区域：浅灰背景 + 1px 边线。
- 面板：白色背景 + 1px border，必要时使用非常弱的阴影。
- 表格：表头使用 `--surface-table-head`，行分割使用 `--border-light`。
- 浮层：使用 `--popover-bg`、`--popover-shadow`、`--modal-backdrop-bg`。
- 命令 Dock：使用玻璃感半透明表面，但只作为工具条，不做大面积毛玻璃背景。

圆角规则：

- 工具按钮：4-6px。
- 输入框、选择器、小弹层：6-8px。
- Overview、最近连接和统计入口可以使用 8-12px 圆角卡片，用来提供欢迎页的停靠感和点击热区。
- 终端、文件表格、系统监控和状态栏这类生产工作区应优先使用直线、分隔线和表格边界。
- 圆角卡片不能层层嵌套；一个信息块只需要一层边界。

## 6. Core Components

### Top Tabs

- tab 高度约 48px。
- 当前 tab 背景略深，非当前 tab 保持低对比。
- tab 标题可显示序号、会话名、关闭按钮。
- 新建按钮是窄按钮，不做大 CTA。

### Overview

- Overview 是入口页，可以比终端工作区更柔和，允许使用圆角面板和统计卡片。
- Hero 可以保留大圆角边界，但内部内容应直接、简洁，不再套第二层卡片。
- 统计卡片和最近连接卡片可以有圆角、弱边框和轻阴影，用于表达可点击区域。
- 卡片 hover 只做轻微抬升或边框增强，不要变成强烈的营销式动效。
- 如果页面进入真实工作流，例如终端、文件、监控、传输，就回到线性/表格化布局。

### System Sidebar

- 侧栏为信息仪表盘，不是菜单装饰区。
- IP、Access、Uptime、Load、CPU、Memory、Swap 等信息右对齐或分栏对齐。
- 进度条高度要低，颜色只表达状态。
- 表格区域保持紧凑，表头固定视觉节奏。
- 折叠按钮应低调，hover 时才增强。

### Terminal Area

- 终端占主面积，背景为 `--terminal-bg`。
- xterm selection 使用 `--terminal-selection-bg`。
- 终端搜索浮层出现在右上角，不能遮挡大量内容。
- 终端内部不使用卡片边框，边界由外层 frame shadow 和区域分隔承担。

### Command Dock

- 命令输入条浮在终端底部，左右留出 48px。
- 输入框文本提示可说明快捷键，但保持一行。
- 历史、选项、查找、复制粘贴、发送目标使用图标或短文本。
- 弹出面板向上展开，宽度受限，不压坏终端。

### File Panel

- 底部文件面板默认高度约 300px。
- 本地和远程文件列表左右分栏，列头固定浅灰背景。
- 路径、大小、类型、修改时间、权限、owner/group 均按表格列处理。
- 文件夹图标使用蓝色，但只作为文件类型识别，不扩大为主题色块。
- 拖拽上传提示放在远程面板标题区域，不能遮挡列表。

### Status Bar

- 状态栏高度约 24px。
- 文案低对比，右侧任务状态明确。
- 只放运行状态、Changelog、API Reference、Status 这类低频入口。

## 7. Interaction

交互应轻、快、可预测：

- hover：背景轻微变深，文字增强，不做夸张位移动画。
- active：当前项有明确边界或背景；不要仅依赖颜色。
- focus：使用可见焦点环，键盘操作必须能定位当前元素。
- resize：侧栏和文件面板拖拽时只改变尺寸，不重排主结构。
- context menu：顶部标签、文件页、终端右键菜单使用统一菜单组件。
- destructive action：必须有确认或清晰危险态，不只靠红色文字。

动画原则：

- 常规 transition 控制在 150-220ms。
- 不对表格行、终端内容、文件列表做重动画。
- 尊重 `prefers-reduced-motion`，后续动画应可降级。

## 8. Iconography

- 图标用于识别工具，不用于装饰。
- Material Symbols 和现有 `AppIcon` 都要保持线性、轻量、同尺寸。
- 顶部工具、文件操作、命令 Dock 应优先使用图标按钮，并通过 title/tooltip 表达含义。
- 只有主要动作可以使用文字按钮，例如「File」「Command」「user」这类模式切换。

## 9. Do And Don't

Do:

- 使用现有 theme token。
- 保持终端、文件表格、系统指标同屏可读。
- 保持 1px 边界、浅灰表头、紧凑行高。
- 在 Overview 这类入口页保留克制的圆角卡片，让主要操作更容易被识别。
- 新增状态色时先补 theme token，再接组件样式。
- 在浅色和暗色主题下同时检查。

Don't:

- 不做营销页 hero、装饰性渐变、光斑、漂浮大卡片。
- 不用新的硬编码蓝色、红色、绿色。
- 不把圆角卡片继续套圆角卡片，也不要把生产工作区改成卡片墙。
- 不让文件表格或终端工具条因为动态文案撑开布局。
- 不用只有颜色差异的状态表达，重要状态必须有文本或图标辅助。

## 10. Implementation Map

主要样式入口：

- `apps/desktop/src/renderer/styles/themes/tokens.css`
- `apps/desktop/src/renderer/styles/themes/default-light.css`
- `apps/desktop/src/renderer/styles/themes/default-dark.css`
- `apps/desktop/src/renderer/styles/features/shell.css`
- `apps/desktop/src/renderer/styles/features/session.css`
- `apps/desktop/src/renderer/styles/features/workstation-skin.css`

新增 UI 时按以下顺序落地：

1. 先确认是否已有 token 可复用。
2. 没有 token 时补到明暗主题变量。
3. 组件 CSS 只引用变量，不直接写主题色。
4. 终端和 Monaco 这类第三方视图从 CSS 变量读取颜色。
5. 最后做亮/暗主题和真实窗口尺寸检查。

## 11. Review Checklist

- 页面是否仍然像桌面工具，而不是网页落地页？
- 终端是否仍是第一视觉焦点？
- 侧栏、终端、文件面板是否能同屏扫读？
- 颜色是否全部来自 theme token？
- hover、active、focus 是否都有明确状态？
- 文本是否不会撑爆按钮、tab、表格列或命令输入条？
- 浅色/暗色主题是否都能保持对比度？
- 新增交互是否遵守 `main -> preload -> renderer` 的能力边界？
