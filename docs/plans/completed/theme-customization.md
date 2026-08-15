# 主题、颜色与终端调色板自定义计划

状态：已完成（主题契约、变量分层、组件皮肤归位、精简设置编辑器和质量门禁均已落地）

关联：[设计规范](../../design.md)、[架构地图](../../architecture.md)、[GitHub issue #186](https://github.com/St0ff3l/fileterm/issues/186)

## 1. 目标与边界

本计划把设置页中的外观主题、界面颜色和终端颜色收敛到一条可持久化、可导入/导出的配置链路：

```text
ThemeConfig -> theme variables -> component tokens -> xterm / Monaco
```

目标包括：

- 保留现有 `default-dark` / `default-light` 两套主题的兼容性。
- 提供 Codex 风格的预设选择、导入配置、复制配置和即时预览。
- 允许自定义 accent、surface、ink、contrast、语义颜色和完整 ANSI 终端调色板。
- 通过少量表面层级控制侧栏、文件区域、顶部标签、控件和底部命令栏，不把每个组件都暴露成主题字段。
- 让终端主题变化即时作用于已打开的 terminal，而不是只影响新建 tab。
- 解决 issue #186：完整映射 ANSI 8 色和 bright 8 色，使日志、`ls` 等远端输出恢复可读的关键字/状态色。

本阶段不把终端输出解析成 HTML，也不在 renderer 里按字符串猜测日志关键字；颜色由远端程序发出的 ANSI 转义序列和 xterm 调色板共同决定。

## 2. 配置契约

持久化配置使用 Codex 风格的字段命名，配置字段和 CSS 变量不直接混用：

```ts
interface ThemeConfig {
  schemaVersion: 'codex-theme-v1'
  codeThemeId: string
  variant: 'dark' | 'light'
  theme: {
    accent: string
    contrast: number // 0-100
    fonts: { code: string | null; ui: string | null }
    ink: string
    opaqueWindows: boolean
    semanticColors: {
      diffAdded: string
      diffRemoved: string
      skill: string
      keyword: string
      secondary: string
      textSecondary: string
      info: string
      warning: string
      error: string
      success: string
    }
    surface: string
    surfaceSecondary: string
    surfaceElevated: string
    overrides?: Record<string, string>
    terminal: TerminalThemeConfig
  }
}

interface TerminalThemeConfig {
  background: string
  foreground: string
  cursor: string
  cursorAccent: string
  selectionBackground: string
  selectionForeground: string
  ansi: {
    black: string
    red: string
    green: string
    yellow: string
    blue: string
    magenta: string
    cyan: string
    white: string
    brightBlack: string
    brightRed: string
    brightGreen: string
    brightYellow: string
    brightBlue: string
    brightMagenta: string
    brightCyan: string
    brightWhite: string
  }
  search: {
    matchBackground: string
    matchRuler: string
    activeMatchBackground: string
    activeMatchText: string
    activeMatchBorder: string
    activeMatchRuler: string
  }
}
```

内置主题的 `codeThemeId` 只表示主题族（`fileterm` 或 `codex`），明暗只由 `variant` 表示；历史配置中的 `fileterm-dark`、`fileterm-light`、`codex-dark` 和 `codex-light` 会在归一化时自动转换为无后缀 ID。

Rust 负责读取旧格式、补默认值、限制颜色格式和 `contrast` 范围；renderer 只消费经过规范化的配置。导入未知字段时忽略，缺失字段使用当前 variant 的安全默认值。

## 3. CSS 变量分层与命名

### 3.1 新的主题原语

主题配置只持久化少量稳定的基础控制项，renderer 再把它们解析成现有组件需要的兼容变量：

| 配置含义   | 持久化字段                                                  | 解析后的职责                                           |
| ---------- | ----------------------------------------------------------- | ------------------------------------------------------ |
| 主色与交互 | `theme.accent`、`theme.contrast`                            | `--primary`、`--focus-outline`、按钮/选中/hover 色阶   |
| 主表面     | `theme.surface`                                             | 应用主画布、页面内容和基础背景                         |
| 次级表面   | `theme.surfaceSecondary`                                    | 应用/设置侧栏、连接后的文件区域和文件面板              |
| 抬升表面   | `theme.surfaceElevated`                                     | 顶部标签栏、文件标签、控件、弹出层和底部命令栏         |
| 表面与文字 | `theme.ink`                                                 | 主文本；设置页不再把文本误称为前景色                   |
| 文本层级   | `theme.semanticColors.textSecondary`                        | `--text-secondary`、`--text-muted` 等次级文本别名      |
| 次色与状态 | `theme.semanticColors.secondary/info/warning/error/success` | 次级交互色和信息、警告、错误、成功状态别名             |
| 代码语义   | `theme.semanticColors.diff*/skill/keyword`                  | 差异色、技能色和关键字色                               |
| 字体与窗口 | `theme.fonts.*`、`theme.opaqueWindows`                      | UI/代码字体、窗口透明度和侧栏 backdrop                 |
| 高级覆盖   | 可选 `theme.overrides`                                      | 仅接受 `--[a-z0-9-]+` CSS 自定义属性，普通用户无需理解 |
| 终端       | `theme.terminal`                                            | 基础终端色、搜索色和完整 16 色 ANSI 调色板             |

`contrast` 是 0-100 的数值配置，不命名为 `--contrast-color` 或直接当作颜色使用。`resolveCompactUiVariables()` 负责从 accent、三层 surface、ink、语义色和 variant 派生现有组件别名；内置 FileTerm 的明暗变体继续由原有 CSS token 和组件皮肤提供像素级稳定的默认外观，设置页将预设名称与明暗变体分离。

`default-dark.css` 和 `default-light.css` 只保存 token。原先散落在主题文件中的组件选择器已迁移到 `styles/features/component-skins.css`，保留原作用域、声明和暗/亮顺序；后续可以按功能逐步把其中的硬编码色值替换为语义变量，而不需要再次改动持久化配置。

### 3.2 现有变量的迁移策略

当前 `--bg-*`、`--text-*`、`--primary`、`--accent-*` 和 `--terminal-*` 已被大量组件使用，本分支不做一次性全量重命名。它们作为派生兼容层保留，并逐步由主题原语驱动：

| 当前变量                                                     | 迁移方向                                                                              |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------- |
| `--primary` / `--primary-hover`                              | 由 compact resolver 的 accent 色阶派生；新代码直接复用现有语义别名                    |
| `--accent-highlight` / `--accent-text`                       | 由 accent 和 ink 派生，不把 accent 当作终端蓝色别名                                   |
| `--focus-outline`                                            | 由 accent 派生，只用于焦点、选中和拖拽目标                                            |
| `--bg-main` / `--bg-sidebar` / `--bg-card` / `--bg-elevated` | 由三层 surface、variant 和 opaqueWindows 派生，作为组件兼容变量                       |
| `--text-main` / `--text-muted` / `--text-soft`               | 由 ink 与 surface 的对比度派生，继续承担现有组件文本层级                              |
| `--terminal-bg` / `--terminal-text`                          | 新代码使用 `--terminal-background` / `--terminal-foreground`；旧变量保留别名          |
| `--terminal-selection-bg` 和搜索变量                         | 新代码统一使用完整的 `*-background` / `*-foreground` / `*-ruler` 语义名，旧名只作兼容 |

不新增通用的 `--color-*` 大杂烩变量；颜色必须能从 `theme`、`surface`、`content`、`action`、`status`、`terminal` 或具体组件语义判断用途。

## 4. 设置页交互

外观设置沿用现有设置页结构，但改成“预设 + 基础颜色 + 按需展开”的单一面板：

- 预设使用 `DropdownSelect`，只显示 FileTerm、Codex 和 Custom；顶部独立的 Light / Dark 卡片负责切换明暗变体，不写原生 `<select>`。
- `Import` 从剪贴板读取 Codex 风格 JSON，校验后立即应用。
- `Copy theme` 将当前规范化配置以 `fileterm-theme-v1:` 紧凑载荷写入剪贴板；前缀已携带版本，因此载荷不重复写 `schemaVersion`。导入同时兼容已有的 `codex-theme-v1:` 前缀，便于跨设备或跨会话复用。
- 默认只展示主色、次色、三层界面表面、文字、字体和窗口透明度等基础控制。
- 代码语义色、可选高级 CSS overrides 和完整 ANSI 调色板按需展开，避免把实现细节平铺给普通用户。
- 所有颜色输入显示可复制的十六进制值；预设下拉框用于整套主题恢复，手动编辑会标记为 Custom。
- 终端背景、前景、光标、选区和搜索高亮保持可见；Normal ANSI / Bright ANSI 两组 8 色网格折叠在终端调色板中。
- 危险操作不使用原生 `window.confirm()`。

新建颜色字段复用统一的 `ThemeColorField`，按钮图标使用 `AppIcon`，不再新增 Material Symbols 字体依赖；主题 token 文件不再承载组件选择器。精简设置页只改变信息架构，不改变 `default-dark` / `default-light` 的默认配置值。

## 5. issue #186 的终端落地

`TerminalView` 当前只显式配置了 green、brightGreen、blue、brightBlue。实现时抽出纯函数 `buildTerminalTheme(config)`，完整填充 xterm 的：

- `black/red/green/yellow/blue/magenta/cyan/white`
- `brightBlack/brightRed/brightGreen/brightYellow/brightBlue/brightMagenta/brightCyan/brightWhite`
- background、foreground、cursor、selection 和搜索装饰色

这样 `tail`、Java 日志级别、异常类名、`ls` 权限/目录/文件名等已有 ANSI 输出无需额外解析即可正确显示。主题偏好事件到达时，所有 terminal 实例重新应用同一调色板并刷新可见行。

## 6. 实施顺序与验收

1. 在 `packages/core` 定义配置类型、默认值和规范化辅助函数。
2. 扩展 `UiPreferences` 与 Rust command 的读写，兼容旧 `ui-preferences.json`。
3. 增加主题变量应用层，保留现有两套主题的兼容别名。
4. 抽出 xterm/Monaco 的主题适配函数，接入完整终端 ANSI 调色板。
5. 重做设置页外观面板，实现预设、导入、复制、颜色字段和终端调色板。
6. 将主题文件中的组件选择器迁移到 `component-skins.css`，保持内置主题的原始作用域和级联顺序。
7. 增加纯函数与 Rust normalization 测试，覆盖坏颜色、越界 contrast、旧配置和完整 ANSI 颜色。
8. 运行 typecheck、lint、Prettier、Tauri tests 和 clippy，并在设置页切换深浅色、导入 Codex 配置、打开已有 terminal 回归。

验收重点：

- 旧用户的 `default-dark` / `default-light` 配置正常启动。
- 修改 accent、surface、ink、contrast 后，设置页、工作区、Monaco 和已打开终端同步变化。
- 导入用户给出的 Codex 配置不会丢失字段，复制出的 JSON 可再次导入。
- issue #186 的日志和 `ll` 输出能显示完整 ANSI 颜色，且颜色不会污染普通无 ANSI 输出。
