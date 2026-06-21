---
name: ui-ux-pro-max-zh
description: 支持可搜索数据库的 UI/UX 设计智能。
---
# ui-ux-pro-max-zh

面向 Web 和移动端应用的全面设计指南。包含跨 16 个技术栈的 67 种 UI 风格、161 个色板、57 组字体配对、99 条 UX 指南以及 25 种图表类型。支持基于优先级的推荐和可搜索数据库。


## 开发前提条件 (Prerequisites)

检查开发环境是否已安装 Python 3.x：

```bash
python3 --version || python --version
```

如果未安装 Python，请根据您的操作系统执行相应命令安装：

**macOS:**
```bash
brew install python3
```

**Ubuntu/Debian:**
```bash
sudo apt update && sudo apt install python3
```

**Windows:**
```powershell
winget install Python.Python.3.12
```

---

## 使用指南 (How to Use)

在以下业务开发场景中，可直接调用此技能：

| 场景 (Scenario) | 触发示例 (Trigger Examples) | 推荐起点 (Start From) |
|----------|-----------------|------------|
| **全新项目 / 页面开发** | "做一个 landing page"、"Build a dashboard" | 步骤 1 → 步骤 2（生成设计系统） |
| **新组件设计与还原** | "Create a pricing card"、"Add a modal" | 步骤 3（专项检索：style, ux） |
| **风格定义 / 调性设计 / 字体配对** | "What style fits a fintech app?"、"推荐配色" | 步骤 2（生成设计系统） |
| **UI 走查与无障碍审查** | "Review this page for UX issues"、"检查无障碍" | 上方的快速参考指南 (Quick Reference) |
| **UI 还原度缺陷及交互异常修复** | "Button hover is broken"、"Layout shifts on load" | 快速参考指南 → 匹配章节调优 |
| **体验升级与性能优化** | "Make this faster"、"Improve mobile experience" | 步骤 3（专项检索：ux, react） |
| **适配暗黑模式** | "Add dark mode support" | 步骤 3（专项检索：style "dark mode"） |
| **引入图表与数据可视化** | "Add an analytics dashboard chart" | 步骤 3（专项检索：chart） |
| **特定技术栈的最佳开发实践** | "React performance tips" | 步骤 4（框架指令检索） |

请遵循以下标准开发流：

### 步骤 1：深入分析用户需求

从用户请求中精准提取以下决策因子：
- **产品定位/类型**：娱乐社交（社交、视频、音乐、游戏）、生产力工具（扫描仪、编辑器、转换器）、效率协作（任务管理、笔记、日历）或复合型平台
- **目标受众画像**：C 端消费者用户；考虑年龄段、使用场景（通勤、闲暇、工作）
- **风格关键词**：活泼趣味 (playful)、动感活力 (vibrant)、极致极简 (minimal)、暗黑 OLED (dark mode)、内容优先 (content-first)、沉浸式 (immersive) 等
- **框架技术栈**：确认当前项目所采用的框架与技术栈

### 步骤 2：生成设计系统规范（核心前置步骤，必须执行）

**请务必首先执行 `--design-system` 命令**，以获取包含严密推理逻辑的完整设计系统方案：

```bash
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "<product_type> <industry> <keywords>" --design-system [-p "Project Name"]
```

底层执行逻辑：
1. **多维度并行检索**：并行检索产品定位 (product)、UI 风格 (style)、色彩搭配 (color)、布局模式 (landing) 以及字体排版 (typography) 数据库
2. **智能规则推理**：基于 `ui-reasoning.csv` 决策模型，智能匹配并输出最佳方案
3. **输出标准化设计系统**：提供版式布局、视觉风格、色板、字体调性及动效交互说明
4. **输出反模式避坑清单**：列出当前行业需规避的交互/视觉雷区

**示例：**
```bash
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "beauty spa wellness service" --design-system -p "Serenity Spa"
```

### 步骤 2b：持久化设计系统（全局主配置 + 页面覆盖模式）

若需保存生成的配置以支持跨会话的**层级上下文检索**，请追加 `--persist` 参数：

```bash
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "<query>" --design-system --persist -p "Project Name"
```

这将在项目根目录下自动创建：
- `design-system/MASTER.md` —— 全局唯一事实来源 (Single Source of Truth，包含全局通用规范)
- `design-system/pages/` —— 存放特定页面覆盖规则的文件夹

**生成特定页面覆盖规则示例：**
```bash
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "<query>" --design-system --persist -p "Project Name" --page "dashboard"
```

这还会额外创建：
- `design-system/pages/dashboard.md` —— 仅针对仪表盘页面的局部覆盖规范（只声明偏离 Master 的差异部分）

**层级上下文检索机制：**
1. AI 在构建特定页面（如 Checkout）时，会首先检索 `design-system/pages/checkout.md` 是否存在
2. 如果该页面配置文件存在，其局部规则将**覆盖 (override)** 全局 `MASTER.md` 的设定
3. 若不存在特定页面配置文件，则默认以 `design-system/MASTER.md` 为全局唯一依据

**上下文感知检索提示词模板 (Context-Aware Prompt)：**
```
I am building the [Page Name] page. Please read design-system/MASTER.md.
Also check if design-system/pages/[page-name].md exists.
If the page file exists, prioritize its rules.
If not, use the Master rules exclusively.
Now, generate the code...
```

### 步骤 3：补充性细节检索（按需执行）

在设计系统大框架下，若需针对特定交互细节进行微调，可运行以下命令检索：

```bash
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "<keyword>" --domain <domain> [-n <max_results>]
```

**补充检索场景：**

| 需求 | 检索领域 (Domain) | 示例 (Example) |
|------|--------|---------|
| 产品原型与排版模式 | `product` | `--domain product "entertainment social"` |
| 视觉风格深度检索 | `style` | `--domain style "glassmorphism dark"` |
| 配色/色板精选 | `color` | `--domain color "entertainment vibrant"` |
| 字体组合/字体配对 | `typography` | `--domain typography "playful modern"` |
| 图表展现与库推荐 | `chart` | `--domain chart "real-time dashboard"` |
| UX 交互与无障碍合规 | `ux` | `--domain ux "animation accessibility"` |
| 落地页布局与 CTA 策略 | `landing` | `--domain landing "hero social-proof"` |
| 开发框架性能调优 | `react` | `--domain react "rerender memo list"` |
| 平台交互规范 (iOS/Android) | `web` | `--domain web "accessibilityLabel touch safe-areas"` |
| AI 提示词与核心变量 | `prompt` | `--domain prompt "minimalism"` |

### 步骤 4：特定技术栈规范适配

检索针对特定框架底层实现的高质量代码模板与性能建议：

```bash
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "<keyword>" --stack <stack>
```

---

## 快捷检索参考表

### 检索领域 (Domains)

| 领域 (Domain) | 用途 | 示例关键词 |
|--------|---------|------------------|
| `product` | 产品类型模式推荐建议 | SaaS, e-commerce, portfolio, healthcare, beauty, service |
| `style` | UI 设计风格、颜色、特效特征 | glassmorphism, minimalism, dark mode, brutalism |
| `typography` | 经典字体搭配、Google Fonts 导入推荐 | elegant, playful, professional, modern |
| `color` | 匹配行业受众的产品色板推荐 | saas, ecommerce, healthcare, beauty, fintech, service |
| `landing` | 落地页页面结构与 CTA 转化策略 | hero, hero-centric, testimonial, pricing, social-proof |
| `chart` | 推荐的数据图表类型与可视化图表库 | trend, comparison, timeline, funnel, pie |
| `ux` | 交互设计优秀实践与反模式防坑指南 | animation, accessibility, z-index, loading |
| `react` | React 生态框架的前端渲染性能调优 | waterfall, bundle, suspense, memo, rerender, cache |
| `web` | 主流端平台设计与无障碍接口规范 | accessibilityLabel, touch targets, safe areas, Dynamic Type |
| `prompt` | 生成特定风格视觉的 AI 绘图提示词 | (style name) |

---

## 标准工作流实战演练

**用户请求**："帮我设计一个 AI 搜索首页"

### 步骤 1：分析需求
- 产品定位：工具类（AI 智能搜索终端）
- 目标受众：追求极致效率、即用即走的 C 端用户
- 风格关键词：前沿、极简、内容优先、暗色模式
- 技术栈：根据项目配置或默认使用 HTML + Tailwind

### 步骤 2：生成设计系统规范（核心前置步骤，必须执行）

```bash
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "AI search tool modern minimal" --design-system -p "AI Search"
```

**输出**：获得一份量身定制的设计系统方案，包含推荐布局、视觉风格特征、语义配色、字体排版、核心交互动效以及避坑指南。

### 步骤 3：补充性细节检索（按需执行）

```bash
# 检索极简与纯黑 OLED 模式的设计规范细节
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "minimalism dark mode" --domain style

# 检索关于搜索过渡和加载状态的 UX 优秀动效实践
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "search loading animation" --domain ux
```

### 步骤 4：特定开发框架的规范检索

```bash
# 检索特定技术栈下的输入性能与列表渲染最佳实践
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "input performance" --stack react
```

**最后阶段**：将设计系统规范与检索出的最佳交互细节融会贯通，产出高质量前端代码。

---

## 输出格式 (Output Formats)

`--design-system` 命令行标志支持以下两种输出格式：

```bash
# ASCII 艺术框（默认）- 最适合终端显示与命令行直观阅读
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "fintech crypto" --design-system

# Markdown 格式 - 格式更规整，最适合保存为文档记录或传给 AI 上下文
python3 .codex/skills/ui-ux-pro-max-zh/scripts/search.py "fintech crypto" --design-system -f markdown
```

---

## 高阶开发与提问技巧

### 提问与检索策略

- **使用多维度复合关键词** —— 结合“产品类别 + 垂直行业 + 视觉基调 + 信息密度”进行检索，例如：`"entertainment social vibrant content-dense"`，而不是随意地检索一个 `"app"`
- **横向测试相似关键词** —— `"playful neon"` → `"vibrant dark"` → `"content-first minimal"`
- **采用漏斗式分析** —— 首先运行 `--design-system` 构建底层规范，随后用 `--domain` 深入研究有疑问的局部维度
- **明确绑定技术栈** —— 检索时追加 `--stack <stack_name>`，以获取针对该框架最优雅的组件封装与渲染逻辑

### 常见设计/交互痛点排查与方案

| 问题 | 解决方案 |
|---------|------------|
| **视觉失焦（风格或配色拿捏不准）** | 尝试微调检索关键词重新运行 `--design-system` 进行设计推演 |
| **暗黑模式文本对比度不合规** | 参阅快速参考指南第 1 节：`color-dark-mode` + `color-accessible-pairs` 优化明度 |
| **动画与交互手感生硬** | 参阅快速参考指南第 7 节：`spring-physics` + `easing` + `exit-faster-than-enter` 曲线 |
| **表单填报交互繁琐、报错不友好** | 参阅快速参考指南第 8 节：`inline-validation` + `error-clarity` + `focus-management` 重构 |
| **导航路径混乱、返回状态丢失** | 参阅快速参考指南第 9 节：`nav-hierarchy` + `bottom-nav-limit` + `back-behavior` 保留现场 |
| **移动端小屏布局拥挤或溢出** | 参阅快速参考指南第 5 节：`mobile-first` + `breakpoint-consistency` 优雅折行 |
| **列表滚动掉帧或交互响应迟钝** | 参阅快速参考指南第 3 节：`virtualize-lists` + `main-thread-budget` + `debounce-throttle` 优化 |

### 上线前 UX 质量走查清单 (Pre-delivery Checklist)

- 在具体实现前，建议检索 `--domain ux "animation accessibility z-index loading"` 作为防错预走查
- 交付前，务必严格核对快速参考指南的 **§1–§3** 章节（CRITICAL & HIGH 级别），这是产品可用性的底线
- 至少在 375px（移动端黄金分辨率）和横屏设备下进行界面极限测试
- 开启系统 **减弱动画 (reduced-motion)** 以及 **系统字体最大字号 (Dynamic Type)**，验证布局是否错乱或被截断
- 亮/暗色模式下的前背景色对比度必须使用专业对比度工具双向校验，不要凭主观推测
- 交互按钮的物理热区面积至少达 44pt，且操作入口在系统级边缘（如状态栏/手势条）绝无重合或干扰

---

## 高质量 UI 开发通用铁律

以下是极易被忽略、但会导致界面显得极其廉价和不专业的细节硬伤：

### 图标与视觉资产规范

- 默认图标库使用 **Phosphor (`@phosphor-icons/react`)**。`src/ui-ux-pro-max-zh/data/icons.csv` 中列出的只是常用推荐图标，不是完整集合。
- 当推荐表中找不到合适的图标时：
  - **优先继续从 Phosphor 的完整图标集中选择任何语义更贴切的图标**；
  - 如果 Phosphor 也没有理想选项，可以使用 **Heroicons (`@heroicons/react`)** 作为备选，注意保持风格一致（线性/填充、笔画粗细、圆角风格）。

| 规范准则 (Rule) | 优秀实践 (Do) | 应规避的反模式 (Avoid) | 核心设计考量 (Why It Matters) |
|------|----------|--------|----------------|
| **禁止将 Emoji 用于结构化图标** | 统一使用高阶矢量图标（如 Phosphor、Heroicons 或平台专有矢量图标库）。 | 在侧边栏导航、设置表单或重要交互控件中胡乱堆叠 Emoji（如 🎨 🚀 ⚙️）。 | Emoji 极其依赖客户端系统预装字库，渲染效果千差万别且无法被设计 Token 所控制。 |
| **纯矢量化资产原则** | 全量使用可无限缩放、完美兼容主题切换的 SVG 图标或平台级矢量 XML 格式。 | 使用 PNG、JPG 等拉伸时会产生锯齿、模糊且无法变色的位图图标。 | 确保在高分视网膜屏幕下的极致清晰度，以及无缝切换亮/暗主题的能力。 |
| **按压状态过渡稳定性** | 点击/按压态仅使用颜色深浅、不透明度 (Opacity) 或投影高度 (Elevation) 的过渡，绝对不改变组件本身的边框物理大小。 | 在按压或悬停时，使用会改变物理宽高、导致周围元素被挤压或抖动的形变 (Layout Shift)。 | 避免造成交互过程中的页面视觉抖动，保障移动端交互的极其平滑，提升感知质量。 |
| **品牌资产规范使用** | 统一使用官方矢量品牌资产，严格遵循其标志安全边界与配色指南。 | 主观臆断或手动绘制 Logo、未经授权随意为品牌图标重新配色或强行拉伸比例。 | 确保品牌呈现的严肃与规范，防范法务纠纷及合规性风险。 |
| **图标尺寸系统化** | 将图标高度与宽度抽象并定义为统一的设计 Token（如 `icon-sm`、`icon-md`=24px、`icon-lg`）。 | 在同一界面中随意混用 18px、21px、25px、29px 等混乱的任意像素尺寸。 | 维持产品设计系统的整体韵律感，建立严谨的视觉阶梯。 |
| **图标描边线宽一致性** | 在同一个视觉信息层级内，所有图标必须采用相同的线宽描边（如统一使用 1.5px 或 2px）。 | 随意拼凑不同线宽、粗细交织的图标集合。 | 描边粗细不一会严重割裂页面的精致度，降低产品的整体品质感。 |
| **图标填充/线性风格纯净度** | 在同一视觉层级或同一组功能区（如底部 Tab 栏）内，必须统一使用同一种图标风格（要么全部线性，要么全部填充）。 | 在同一功能区域内，随性混用填充型与线性图标（如首页是线性，搜索页是填充）。 | 维护图标系统的整体调性与视觉规律性。 |
| **触控热区最小面积限制** | 确保可交互按钮物理热区面积至少达 44×44pt；如果图标视觉过小，必须使用负外边距或 hitSlop 属性扩大实际热区。 | 使用直接暴露的小图标而未做任何热区扩展，导致用户反复点击失败。 | 保障不同手部尺寸用户在运动、颠簸场景下的触碰成功率，满足 A11y 规范。 |
| **图标文字基线精准对齐** | 图标必须与同级排版的文本基线（Baseline）或中心线严格对齐，并保持统一的水平外边距。 | 图标高度偏高或偏低，产生杂乱无章的视错觉。 | 细微的视觉失衡是摧毁高级感的主要元凶，必须做到像素级严谨对齐。 |
| **图形对比度无障碍审查** | 遵循 WCAG 对比度底线：常规功能图标对比度 ≥4.5:1，大型图形化装饰性符号对比度 ≥3:1。 | 采用过淡的灰色图标，导致在阳光直射下完全看不清。 | 确保在各种极端照度和不同设备屏幕上的卓越可读性。 |

### 主流端交互规范原则

| 规范法则 (Rule) | 正面实践 (Do) | 反面模式 (Don't) |
|------|----|----- |
| **即时触控反馈 (Tap feedback)** | 必须在触控后的 80–150ms 内给出明确的涟漪、明度微调或透明度过渡等响应 | 点击后界面如一潭死水，无任何视觉或动效响应 |
| **动效时间把控 (Animation timing)** | 微交互时长限定在 150–300ms 左右，且必须配置符合物理特性的非线性曲线 | 动效瞬间硬切 (0ms) 或采用超过 500ms 的沉重拖沓动画 |
| **朗读焦点顺序 (Accessibility focus)** | 确保屏幕阅读器 (Screen Reader) 的焦点流向与页面视觉布局顺序绝对匹配 | 焦点焦点随意跳动或将无关的装饰元素朗读给视障用户 |
| **禁用态状态表达** | 应用淡化透明度、鼠标禁用标识，并通过 `disabled` 属性禁止一切点击触发 | 按钮看起来依然可用但点击后无任何反应，使用户困惑是否发生卡顿 |
| **扩大最小交互区域** | 确保移动端触控目标热区宽与高均 ≥44pt，对微小图标通过属性扩充交互热区 | 使用过小的像素点，逼迫用户像做外科手术一样精准点击 |
| **规避多重手势冲突** | 每一个物理区域仅设计一种主流手势操作，防止嵌套滚动或边缘划动手势拦截系统手势 | 在支持滑动的卡片组件内，强行嵌入横向滚动的次级数据列表 |
| **无障碍语义声明** | 规范使用原生交互控件（如 Button、Pressable），配置准确的 accessibility role 角色 | 随意使用普通的无语义容器（如 Div、View）来包裹关键交互入口 |

### 亮/暗色模式对比度调优

| 规范法则 (Rule) | 正面实践 (Do) | 反面模式 (Don't) |
|------|----|----- |
| **卡片/卡板层级感 (Surface readability)** | 善用投影高度、高光边框或微弱明度差将卡片面板与底层背景界定分明 | 卡片完全融入背景，导致页面沦为平铺直叙的一张白板 |
| **浅色模式文本易读性** | 确保浅色主题下的正文段落与背景的对比度 ≥4.5:1 | 在白底上使用过淡的浅灰色文字，伤害视力 |
| **暗黑模式文本易读性** | 确保深色主题下主要文本对比度 ≥4.5:1，次要说明文字对比度 ≥3:1 | 暗色主题下的文本对比度不足，导致文字几乎隐形 |
| **边框与分割线辨识度** | 确保分割线和细边框在浅色与暗色主题下均有匹配的明暗度对比 | 只针对默认主题调优，在暗黑模式下边框彻底消失 |
| **交互状态双向适配** | 针对亮/暗两套主题，均设计高水准的 hover、focus、disabled 及 active 交互态 | 仅针对其中一个主题优化了状态，导致另一主题下交互态难以辨认 |
| **Token 化色彩管理** | 在文字、背景和边框上全量引入语义化色彩 Token，实现主题一键切换 | 在组件样式中大量写死 Hex 颜色，使得主题适配维护成本巨大 |
| **半透明遮罩对比调优** | 模态弹窗的底层 Scrim 半透明度应设在 40%–60% 之间，以提供充足的前景聚焦力 | 遮罩过淡导致底层内容与弹窗文本发生视觉打架，严重干扰阅读 |

### 布局与间距节奏规范

| 规范法则 (Rule) | 正面实践 (Do) | 反面模式 (Don't) |
|------|----|----- |
| **安全区域对齐 (Safe-area)** | 针对页眉、底栏及全局悬浮 CTA 栏，强制注入顶部与底部安全区域 padding | 悬浮底栏把刘海屏或系统底部手势条死死挡住，阻断操作 |
| **避开系统交互热区** | 为系统状态栏和导航指示条预留充足的高度避让，严防手势冲突 | 让可交互的按钮与系统自带的手势滑轨边缘重叠，触发误触 |
| **统一多端限制宽度** | 依据设备屏幕尺寸，采用预设的最大安全内容容器宽度（如大屏 PC 限制在 1200px 左右） | 页面布局在宽屏下无限延伸扩展，导致视觉重心涣散 |
| **4/8pt 间距韵律系统** | 内边距、外边距、网格空隙等，一律采用统一的 4/8/16/24/32/48px 等阶梯节奏 | 随心所欲使用 7px、13px、19px、31px 等无规律的间距数值 |
| **控制段落最大排版宽度** | 在宽屏平板及 PC 上，限制单行文字最大排版宽度，保证视线移动舒适 | 段落文字横跨整屏，导致用户每读完一行都需要长距离移动眼球寻找下一行 |
| **垂直间距分明层级** | 页面各区块之间使用鲜明的、拉开差距的垂直间距（如 24px/48px/64px） | 区块区块之间没有清晰的空隙，使页面显得局促而杂乱 |
| **响应式水平槽宽** | 随着设备屏幕宽度增加，按比例增加两侧的水平页面边距 (Page Gutters) | 在平板和移动端上都顽固使用相同极窄的 12px 边距 |
| **底部滚动安全占位** | 凡是页面底部有固定粘性底栏时，必须为列表容器最底端提供相等的 Padding 占位 | 用户滑动到列表最底部时，最后一个条目被粘性底栏永久遮挡 |
