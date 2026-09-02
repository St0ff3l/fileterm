---
name: common-components-skill
description: FileTerm renderer 通用组件与主题 CSS 的强制规范。只要任务涉及新增或修改 React/TSX 组件、组件 CSS、styles/tokens、主题切换、按钮、输入框、下拉框、弹窗、状态指示器、独立窗口或 UI 颜色，就必须使用本 skill。严格执行 --ref-* 主题值 → 语义变量 → 组件 CSS 的边界，并在完成前运行 CSS contract check。
---

# FileTerm 通用组件与 CSS 主题规范

这是一份执行规范，不是设计建议。目标是让组件结构、主题颜色和平台行为各自有明确归属，并让自定义主题可以通过同一条变量链路生效。

## 0. 开始改动前

1. 先读取仓库根目录的 AGENTS.md，并以当前源码为准，不要沿用本文件中的历史路径。
2. 涉及颜色、阴影、圆角、按钮、输入框、弹窗、下拉框、状态点或独立窗口时，先查看 apps/tauri/src/renderer/styles/tokens/ 和对应组件。
3. 先运行一次：

   ```bash
   bash .agents/skills/common-components-skill/scripts/check-css-contract.sh
   ```

   记录已有债务；不要把旧文件中的问题误认为本次新增问题。

4. 修改完成后再次运行检查，并说明仍存在的历史债务或有明确理由的例外。

## 1. 当前真实目录和归属

Renderer 的规范目录如下：

```text
apps/tauri/src/renderer/
├── components/
│   └── common/
│       └── <component-name>/
│           ├── <component-name>.tsx
│           └── <component-name>.css
├── features/
│   ├── <feature>/
│   │   ├── <feature-component>.tsx
│   │   └── <feature-component>.css       # 新代码优先就地共址
│   └── common/                            # 行为工具、辅助组件和兼容 re-export
└── styles/
    ├── index.css                          # 全局入口
    ├── reset.css                          # 浏览器重置
    ├── tokens/
    │   ├── index.css                      # token 入口
    │   ├── foundations.css                # 字体、尺寸、圆角、阴影等基础值
    │   ├── semantic.css                   # 语义映射，不写具体色值
    │   ├── fileterm-dark.css              # FileTerm 暗色具体值
    │   ├── fileterm-light.css             # FileTerm 亮色具体值
    │   ├── codex-dark.css                 # Codex 暗色具体值
    │   └── codex-light.css                # Codex 亮色具体值
    └── features/                          # 现有遗留全局/皮肤样式，禁止新增组件样式
```

归属规则：

- 可被多个功能复用的视觉组件放在 components/common/<name>/。
- 只服务一个业务域的组件放在 features/<feature>/，新 CSS 与组件共址。
- features/common/ 中已有的行为工具、辅助组件和兼容导出可以继续存在；新建的 Button、Card、Dialog、Input、DropdownSelect 等视觉组件以 components/common/ 为唯一实现。
- styles/features/ 中的旧皮肤和全局样式属于迁移区。修改旧文件时顺手迁移触及的规则，但不要继续往里面新增一套组件皮肤。

## 2. 三层颜色模型

颜色必须按下面顺序流动：

```text
主题配置或主题文件的具体值
        ↓
--ref-* 参考值层
        ↓
semantic.css 的语义变量
        ↓
组件和 feature CSS
```

### 2.1 参考值层：--ref-*

参考值层保存真正的颜色、透明色和主题相关派生值。允许出现 hex、rgb、rgba 或 color-mix。

静态内置主题的位置：

- styles/tokens/fileterm-dark.css
- styles/tokens/fileterm-light.css
- styles/tokens/codex-dark.css
- styles/tokens/codex-light.css

运行时自定义主题的位置：

- apps/tauri/src/renderer/app/theme-config.ts
- 主题配置类型和默认值在 packages/core/src/index.ts

示例：

```css
/* fileterm-dark.css */
:root {
  --ref-accent: #1687e8;
}
```

### 2.2 语义层：semantic.css

语义层回答“这个颜色用于什么”，不回答“颜色具体是多少”。

```css
/* semantic.css */
:root {
  --action-primary-bg: var(--ref-accent);
  --action-primary-hover: var(--ref-accent-hover);
  --text-primary: var(--ref-text-primary);
  --surface-card: var(--ref-surface-card);
}
```

semantic.css 禁止出现具体 hex、rgb、rgba。也不要在这里写一个具体颜色作为 fallback。

### 2.3 组件层

组件只使用语义变量：

```css
/* components/common/button/button.css */
.button--primary {
  background: var(--action-primary-bg);
  color: var(--action-primary-text);
}
```

组件不得直接使用 --ref-accent、--ref-surface-card 等参考值，因为这样会让组件知道主题实现细节。

## 3. 变量命名和选择规则

### 3.1 常用参考值

- 主品牌色：--ref-accent
- 主品牌 hover/active：--ref-accent-hover、--ref-accent-active
- 辅助品牌色：--ref-accent-secondary
- 焦点光环：--ref-accent-focus、--ref-focus-outline
- 画布和表面：--ref-surface-canvas、--ref-surface-panel、--ref-surface-card、--ref-surface-input
- 文字：--ref-text-primary、--ref-text-secondary、--ref-text-muted
- 边框：--ref-border-subtle、--ref-border-default、--ref-border-strong
- 状态：--ref-status-success、--ref-status-warning、--ref-status-danger、--ref-status-info
- 功能颜色：--ref-color-folder、--ref-color-skill、--ref-color-total 等

### 3.2 常用语义变量

- 表面：--surface-*
- 文字：--text-*
- 边框：--border-*
- 操作：--action-primary-_、--action-secondary-_、--action-ghost-_、--action-danger-_
- 状态：--status-success*、--status-warning*、--status-danger*、--status-info*
- 焦点：--focus-outline、--accent-focus-ring

不要使用含义不清的 --secondary。以下概念必须分开：

- theme.accent：主题主色
- theme.semanticColors.secondary：辅助品牌/焦点色
- theme.semanticColors.textSecondary：次要文字色
- --action-secondary-*：次要按钮层级，不等于辅助品牌色

## 4. 新颜色的处理流程

遇到一个新颜色，例如 #1265d8，必须先问“它是什么用途”：

1. 如果它是主题主色，使用已有的 --ref-accent，不要新建 --ref-abcdccent。
2. 如果它是文件夹颜色，使用或新增 --ref-color-folder。
3. 如果它是危险状态，使用或新增 --ref-status-danger。
4. 如果它只是某个组件的主按钮背景，仍然通过 --action-primary-bg 使用，不为每个组件创建一个颜色变量。
5. 如果确实是全新的可复用用途，按用途命名新的 --ref-*，同时：
   - 在四个内置主题文件中补齐值；
   - 在 theme-config.ts 的运行时映射中补齐自定义主题值；
   - 在 semantic.css 中增加语义映射；
   - 组件只使用这个语义变量。

不要按色号、随机缩写或某个页面命名，例如 --ref-blue2、--ref-abcdccent。

## 5. 自定义主题规则

ThemeConfig.theme.accent 已经是主题配置的一部分。用户自定义 #1265d8 时，正确链路是：

```text
theme.accent = '#1265d8'
        ↓
runtime 写入 --ref-accent
        ↓
--action-primary-bg: var(--ref-accent)
        ↓
Button 使用 --action-primary-bg
```

运行时应把规范化后的主题值写到根元素：

```ts
root.style.setProperty('--ref-accent', normalized.theme.accent)
```

同时写入由主色派生的 --ref-accent-hover、--ref-accent-active、--ref-accent-focus 等参考值。

如果用户没有提供 accent，使用 normalizeThemeConfig 的基础主题 fallback；不要在 Button.css 或 feature CSS 中偷偷补一个颜色。

旧的 --primary、--bg-card 等变量只允许作为 semantic.css 中的临时兼容别名。新代码不得继续使用它们。

## 6. CSS 允许和禁止的写法

### 6.1 允许

- 在组件 CSS 中直接写布局、尺寸、间距、定位和排版数值。
- 在组件 CSS 中使用 --surface-_、--text-_、--border-_、--action-_、--status-*。
- 使用语义变量参与 color-mix，前提是不能混入具体 hex/rgb。
- 使用状态 class、data attribute 或修饰 class 表达 hover、active、disabled、error 等状态。

### 6.2 禁止

```css
/* 禁止：组件直接写具体色值 */
background: #1265d8;

/* 禁止：组件跳过语义层 */
background: var(--ref-accent);

/* 禁止：新代码使用旧兼容别名 */
background: var(--primary);
color: var(--text-main);
```

- 组件和 feature CSS 不得直接写 hex、rgb、rgba。
- JSX/TSX 的 inline style 不得写颜色 fallback 或具体色值。
- 新代码不得使用 var(--ref-*)，参考值层和语义层本身除外。
- 不要用全局元素选择器给业务组件着色；组件样式必须有组件根 class。
- 不要通过复制一整份暗色布局来实现亮色主题；只覆盖主题值或必要的皮肤差异。
- 新增 !important 前必须证明是原生控件兼容问题，并留下行内注释。DropdownSelect 的原生 select reset 可作为例外；普通 Input 校验状态应优先改进 selector。

## 7. 通用组件规范

### Button

- 统一使用 components/common/button/。
- 变体使用 primary、secondary、ghost、danger 等语义命名。
- 同一操作组的按钮必须共享高度、圆角和内边距。
- 主按钮使用 --action-primary-*，不能用 --focus-outline 填充。
- 异步操作必须有 busy/submitting 状态：禁用重复提交、显示 spinner、保留失败反馈。

### DropdownSelect

- 所有表单下拉框必须使用 DropdownSelect。
- 禁止在业务组件中新增原生 select。
- macOS 原生外壳和 Windows/Linux 自绘 Portal 都必须保持同一套语义变量和自适应箭头尺寸。

### Input

- 背景、边框、文字和错误状态只使用 --surface-input、--border-_、--text-_、--status-danger*。
- 不要用 !important 解决普通校验状态的 selector 问题。

### Dialog 和危险操作

- 删除、清空、覆盖、断开等破坏性操作必须使用 ConfirmActionDialog。
- 禁止在桌面 WebView 中使用 window.confirm()。
- Dialog 的 surface、边框、阴影和按钮都通过语义变量获得。

### 图标和滚动

- 图标优先使用离线 SVG 的 AppIcon，禁止新增 material-symbols-outlined WebFont 依赖。
- 新增纵向滚动区域默认复用 features/common/vertical-scrollbar.tsx，并隐藏容器原生纵向滚动条。
- 横向滚动、第三方编辑器内部滚动和协议组件自带滚动可以保留专用实现。

### StatusIndicator 和系统指标

- StatusIndicator 的状态、尺寸和可访问性语义必须保持一致；装饰性状态点使用 aria-hidden。
- CPU、交换、内存风险阈值与内存分段色是两个概念，不要用一套颜色覆盖另一套信息。

## 8. Feature 样式和遗留皮肤迁移

- 新 feature 组件的 CSS 与组件共址，根 class 使用 feature 前缀，避免全局污染。
- styles/features/ 中的旧 CSS 只作为迁移对象，不得把新功能继续堆进去。
- 修改旧 CSS 时，先把直接色值搬到对应 --ref-*，再把使用点改成语义变量。
- 不要机械地把所有 var(--primary) 替换成 --action-primary-bg。如果原用法是边框、文字、焦点或状态，必须按视觉用途选择对应语义变量。

常见迁移方向：

```text
--bg-main       → --surface-canvas
--bg-card       → --surface-card 或 --surface-panel
--bg-hover      → --surface-hover
--text-main     → --text-primary
--primary       → 按用途选择 --action-primary-bg、--border-focus 或 --accent-highlight
--danger        → --status-danger
--success       → --status-success
--warning       → --status-warning
--info          → --status-info
```

兼容别名只保留在 semantic.css，并在迁移完成后删除。完成标准是新代码不再产生旧变量引用，再逐步清理桥接。

## 9. 主题切换和独立窗口

- 主题由 document.documentElement.dataset.theme 控制，当前值包括 fileterm-dark、fileterm-light、codex-dark、codex-light 等。
- main.tsx 必须在 React 挂载前设置首屏主题，避免独立亮色窗口先闪黑。
- theme-config.ts 负责运行时主题变量和自定义主题映射；不要再按旧 styles/themes 路径增加分支。
- TerminalView、xterm、Monaco 等非纯 CSS 渲染器必须监听主题变化并主动刷新内部主题。
- 独立窗口的壳层不得写死暗色背景，使用语义 surface 或透明背景。

## 10. 验收检查

完成 CSS 或组件任务后，必须执行：

```bash
bash .agents/skills/common-components-skill/scripts/check-css-contract.sh
npx prettier --check apps/tauri/src/renderer packages/core
```

涉及 TypeScript 或组件行为时，再执行项目要求的 typecheck、lint 和相关测试。

CSS contract check 至少验证：

1. semantic.css 没有直接 hex/rgb。
2. components/common/ 和新 feature CSS 没有直接颜色值。
3. 组件没有直接使用 --ref-*。
4. 新组件没有使用旧兼容变量。
5. 四个内置主题文件仍然存在。
6. !important 只作为报告项，新增例外必须有理由。

## 11. 完成定义

提交前逐项确认：

- [ ] 组件放在正确的 components/common/ 或 features/<feature>/ 目录。
- [ ] 结构样式与主题颜色分开。
- [ ] 组件只使用语义变量，不使用具体色值或 --ref-*。
- [ ] 新颜色按用途命名，并补齐静态主题和运行时映射。
- [ ] 自定义主题的 theme.accent 可以流到 --ref-accent。
- [ ] 没有新增旧兼容变量、原生 select、material WebFont 或危险操作的原生 confirm。
- [ ] 运行 CSS contract check，并记录遗留债务或合法例外。
