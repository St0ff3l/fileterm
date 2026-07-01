# FileTerm 通用组件技能指南

## 概述

本指南详细介绍 FileTerm 项目中的通用组件架构、CSS 主题系统，以及独立窗口在亮暗主题下的落地方式。

---

## 一、通用组件架构

### 1.1 组件目录结构

```
renderer/
├── components/           # 核心通用组件
│   └── TerminalView.tsx  # 终端视图组件
└── features/
    └── common/           # 通用功能组件
        ├── AppIcon.tsx           # 图标组件
        ├── ContextMenu.tsx       # 右键菜单组件
        └── horizontal-scroll.ts  # 水平滚动工具函数
```

### 1.2 核心组件说明

#### TerminalView（终端组件）

**文件路径**：`apps/desktop/src/renderer/components/TerminalView.tsx`

**功能特性**：
- 基于 `@xterm/xterm` 构建的终端视图
- 支持主题颜色定制（通过 CSS 变量）
- 快捷键支持：复制(⌘C/Ctrl+Shift+C)、粘贴(⌘V/Ctrl+Shift+V)、查找(⌘F/Ctrl+F)
- 右键上下文菜单集成
- 自动适配容器尺寸
- 国际化文本支持

**关键依赖**：
```typescript
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { ContextMenu } from '../features/common/ContextMenu'
```

#### AppIcon（图标组件）

**文件路径**：`apps/desktop/src/renderer/features/common/AppIcon.tsx`

**支持的图标类型**：
| 图标名 | 用途 |
|--------|------|
| `grid` | 网格视图 |
| `menu` | 菜单按钮 |
| `server` | 服务器连接 |
| `connections` | 连接管理 |
| `folder` | 文件夹 |
| `file` | 文件 |
| `history` | 历史记录 |
| `refresh` | 刷新 |
| `upload` | 上传 |
| `download` | 下载 |
| `flash` | 快速操作 |

**使用方式**：
```tsx
<AppIcon name="folder" size={16} />
```

**特性**：
- 统一 SVG 图标系统
- 可自定义尺寸（默认 14px）
- 颜色继承父元素 `currentColor`

#### ContextMenu（右键菜单组件）

**文件路径**：`apps/desktop/src/renderer/features/common/ContextMenu.tsx`

**功能特性**：
- 点击外部区域自动关闭
- Esc 键关闭
- 位置自动调整（避免超出视口）
- 支持禁用状态和危险操作样式

**接口定义**：
```typescript
type ContextMenuEntry = {
  label?: string
  shortcut?: string
  disabled?: boolean
  danger?: boolean
  action?(): void
  separator?: boolean
}
```

**使用方式**：
```tsx
<ContextMenu
  items={[
    { label: '复制', shortcut: '⌘C', action: handleCopy },
    { separator: true },
    { label: '删除', danger: true, action: handleDelete }
  ]}
  onClose={closeMenu}
  position={{ x: 100, y: 100 }}
/>
```

#### handleHorizontalWheelScroll（水平滚动工具）

**文件路径**：`apps/desktop/src/renderer/features/common/horizontal-scroll.ts`

**功能**：支持鼠标滚轮水平滚动容器

**特性**：
- 兼容不同浏览器的 deltaMode
- 边界检测（不滚动超出内容范围）

**使用方式**：
```tsx
<div onWheel={handleHorizontalWheelScroll}>
  {/* 可水平滚动内容 */}
</div>
```

---

## 二、CSS 主题系统

### 2.1 样式目录结构

```
renderer/styles/
├── themes/               # 主题定义
│   ├── index.css         # 主题入口
│   ├── tokens.css        # 设计令牌
│   ├── default-dark.css  # 暗色主题
│   └── default-light.css # 亮色主题
├── features/             # 功能模块样式
│   ├── session.css       # 会话样式
│   ├── shell.css         # 终端样式
│   ├── home.css          # 首页样式
│   └── ...
├── app.css               # 应用主样式
├── global.css            # 全局样式
└── index.css             # 样式入口
```

### 2.2 设计令牌（Tokens）

主题样式采用 `token -> theme vars -> component skins` 的层级结构。

**核心 CSS 变量**：

| 变量名 | 用途 | 示例值 |
|--------|------|--------|
| `--terminal-bg` | 终端背景色 | `#1e1e1e` |
| `--terminal-text` | 终端文字色 | `#e0e0e0` |
| `--success` | 成功状态色 | `#39d98a` |
| `--accent-text` | 强调文字色 | `#c8d0da` |
| `--text-main` | 主文字色 | `#f1f5f9` |

**终端主题变量**：
```css
:root {
  --terminal-bg: #1e1e1e;
  --terminal-text: #e0e0e0;
  --terminal-cmd-bg: rgba(148, 163, 184, 0.24);
  --terminal-search-highlight: rgba(236, 255, 71, 0.82);
}
```

### 2.3 主题切换机制

项目支持亮色/暗色主题切换，但当前仓库不是通过 `.theme-dark/.theme-light` 类名控制，而是通过 `document.documentElement.dataset.theme` 和 Electron 窗口启动参数控制。

```css
/* 默认暗色：未声明主题时也会走暗色 */
:root:not([data-theme]),
:root[data-theme='default-dark'],
:root[data-theme='default'] {
  color-scheme: dark;
}

/* 亮色主题 */
:root[data-theme='default-light'] {
  color-scheme: light;
}
```

**实现方式**：
- 在 `html` 根元素上设置 `data-theme="default-dark|default-light"`
- 通过 `apps/desktop/src/renderer/hooks/useThemeMode.ts` 同步 `dataset.theme` 与 `color-scheme`
- 通过 `apps/desktop/src/renderer/App.tsx` 持久化 `theme` / `locale`
- 通过 `apps/desktop/src/main/main.ts` 把 `theme` / `locale` 注入独立窗口 query，并设置 `BrowserWindow.backgroundColor`
- 组件通过 CSS 变量获取颜色值，终端再从 CSS 变量映射到 xterm theme

### 2.3.1 启动阶段防闪色

独立窗口的亮色主题有一个额外约束：不能等 React 挂载后才决定背景色，否则白窗会先闪一下黑底。

当前仓库的处理链路是：

1. `apps/desktop/src/main/main.ts`
   - 读取持久化的 UI 偏好
   - 创建 `BrowserWindow` 时按主题设置 `backgroundColor`
   - 打开独立窗口时把 `theme`、`locale` 作为 query 传给 renderer
2. `apps/desktop/index.html`
   - 在脚本最早期读取 query / `localStorage`
   - 提前设置 `data-theme`、`lang`、启动背景色和 `color-scheme`
3. `apps/desktop/src/renderer/App.tsx`
   - 用 query 作为首选初值，再回退到本地存储

如果未来再加新独立窗口，必须沿用这条链路，否则很容易出现“窗口本体是亮色，但出生先黑一下”的问题。

### 2.4 样式规范

1. **避免硬编码**：颜色、阴影、圆角应通过 CSS 变量引用
2. **主题优先**：所有视觉属性应支持主题切换
3. **组件隔离**：功能模块样式应独立，避免全局污染
4. **确认按钮同高**：`flat-button` 与 `primary-button` 默认共享一套中号按钮尺寸，统一走 `tokens.css` 中的按钮变量；如果某处需要更小尺寸，必须显式使用 `compact` 等修饰类，而不是让蓝色确认按钮单独更高、更宽。
5. **异步确认必有反馈**：凡是会触发 IPC、网络、远端命令或文件写入的确认按钮，组件层必须提供 `isSubmitting`/`isBusy` 态，至少覆盖三件事：禁用重复提交、显示按钮内 spinner、保留并更新失败反馈区域。
6. **响应式设计**：使用 `@media` 查询适配不同屏幕尺寸

### 2.5 组件结构与 CSS 主题颜色分离

#### 分离原则

组件的结构样式（layout）与主题颜色（theme）必须分离，遵循以下规则：

| 分类 | 内容 | 示例 |
|------|------|------|
| **结构样式** | 布局、尺寸、间距、定位 | `display`, `flex`, `margin`, `padding`, `position` |
| **主题颜色** | 颜色、背景色、边框色 | `color`, `background-color`, `border-color` |

#### 分离实现

**结构样式**应直接写在组件的 CSS 文件中：

```css
/* Button.css */
.button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 8px 16px;
  border-radius: 6px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  border: none;
  outline: none;
  transition: all 0.2s ease;
}
```

**主题颜色**应通过 CSS 变量引用：

```css
/* Button.css */
.button {
  background-color: var(--primary-bg);
  color: var(--primary-text);
  border: 1px solid var(--primary-border);
}

.button:hover {
  background-color: var(--primary-hover);
}

.button:active {
  background-color: var(--primary-active);
}
```

#### 主题变量定义

主题颜色集中定义在 `apps/desktop/src/renderer/styles/themes/tokens.css` 中：

```css
/* themes/tokens.css */
:root {
  /* 主色调 */
  --primary-bg: #3b82f6;
  --primary-text: #ffffff;
  --primary-border: #2563eb;
  --primary-hover: #2563eb;
  --primary-active: #1d4ed8;
  
  /* 成功状态 */
  --success-bg: #22c55e;
  --success-text: #ffffff;
  
  /* 警告状态 */
  --warning-bg: #f59e0b;
  --warning-text: #ffffff;
  
  /* 危险状态 */
  --danger-bg: #ef4444;
  --danger-text: #ffffff;
}
```

#### 暗色主题覆盖

在暗色主题中优先覆盖颜色变量和组件皮肤，无需复制布局结构：

```css
/* apps/desktop/src/renderer/styles/themes/default-dark.css */
:root[data-theme='default-dark'] {
  --primary-bg: #60a5fa;
  --primary-text: #0f172a;
  --primary-border: #93c5fd;
  --primary-hover: #93c5fd;
  --primary-active: #bfdbfe;
}
```

#### 组件编写规范

1. **结构与颜色分离**：组件 CSS 文件中，结构样式直接写值，颜色样式使用 CSS 变量
2. **变量命名规范**：使用 `--[category]-[property]-[state]` 命名规则
3. **变量复用**：相同语义的颜色应复用同一个变量（如按钮禁用态统一使用 `--disabled-bg`）
4. **主题完整性**：确保所有主题变量在亮色/暗色主题中都有定义
5. **避免内联样式**：颜色值不应写在 JSX 中，应通过 CSS 类名或 CSS 变量引用

### 2.6 独立窗口主题约束

连接管理器、连接表单、命令管理器、命令编辑器、文件编辑器都存在 standalone 形态。处理这类窗口时，额外遵循：

1. **窗口出生底色**：优先在 `main.ts` 设置 `BrowserWindow.backgroundColor`，不要只靠 renderer CSS。
2. **入口预设主题**：如果窗口首屏可能是亮色，必须在 `apps/desktop/index.html` 挂载前预设 `data-theme`。
3. **基础层不要写死 dark**：像 `.standalone-shell`、`.command-editor-window` 这类壳层不要直接写 `#1b1b1b`，优先用 `var(--bg-main)` 或透明，再交给主题层覆盖。
4. **暗色专用类按需挂载**：类似 `file-editor-modal--dark` 只能在暗色主题下添加，不能在组件里永久写死。
5. **终端主题要主动重刷**：`TerminalView` 这类非纯 CSS 组件要监听根节点主题变化并主动同步内部渲染主题。

---

## 三、组件协作关系

```
TerminalView
    └── ContextMenu（右键菜单）
    └── xterm.js（终端渲染）

TabBar
    └── AppIcon（标签图标）
    └── ContextMenu（右键菜单）

FileManager
    └── AppIcon（文件/文件夹图标）
    └── ContextMenu（右键菜单）
    └── handleHorizontalWheelScroll（水平滚动）

CommandCenter
    └── AppIcon（命令图标）
```

---

## 四、扩展指南

### 4.0 系统指标条颜色约定

系统信息侧栏里的 CPU、交换、内存相关组件有一条现有约定，后续改 UI 时不要随手改回固定色：

1. `CPU` 条
   - 走风险阈值色，不是永久绿色
   - 阈值函数在 `apps/desktop/src/renderer/features/system/SystemSidebar.tsx` 的 `getMetricTone()`
   - 当前规则：
     - `< 60%` 绿色
     - `>= 60%` 黄色
     - `>= 85%` 红色
2. `交换` 条和点
   - 和 CPU 使用同一套 `getMetricTone()` 阈值
3. `内存` 点
   - 也走同一套 `getMetricTone()` 阈值
4. `内存` 条
   - 默认不是风险色条，而是 `app / cache / kernel` 的分段语义色
   - 如果要改成风险色，先明确是否保留分段信息，不要直接覆盖掉分段语义

如果未来抽公共组件，优先保留“风险色阈值函数”和“分段条语义色”这两个概念分离。

### 4.1 添加新图标

在 `AppIcon.tsx` 中添加新的图标类型：

```tsx
{name === 'new-icon' ? (
  <path {...commonProps} d="M..." />
) : null}
```

### 4.2 添加新主题

1. 在 `themes/` 目录下创建新的主题文件
2. 在 `themes/index.css` 中引入
3. 在 `useThemeMode`、`App.tsx`、窗口 query 初始化与 main 进程 UI 偏好持久化中补齐新主题分支
4. 如果新主题需要独立窗口支持，同时更新 `apps/desktop/index.html` 启动预设逻辑和 `main.ts` 的窗口背景色映射

### 4.3 创建新通用组件

1. 根据组件类型选择放置位置：
   - 核心视图组件 → `renderer/components/`
   - 功能辅助组件 → `renderer/features/common/`
2. 遵循现有组件的命名和编码规范
3. 添加相应的样式文件到 `styles/features/`
4. 如果组件会出现在独立窗口或使用内置渲染器（如 xterm、Monaco），额外检查主题切换和首屏加载是否会闪错色
