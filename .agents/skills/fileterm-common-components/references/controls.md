# 公共控件与资产约束

## 7. 通用组件规范

### Button

- 统一使用 components/common/button/。
- 变体使用 primary、secondary、ghost、danger 等语义命名。
- 同一操作组的按钮必须共享高度、圆角和内边距。
- 侧栏面板中 32px 高的文字操作按钮使用公共 `size="panel"`。保存、取消等两字按钮最窄 72px，长文案可自然撑宽。主操作和同一区域的次要操作使用同一尺寸。
- 带文字的操作按钮把装饰图标传给公共 `Button` 的 `icon`。32px 面板按钮使用 11px 文字，36px 标准按钮使用 12px 文字，配套图标使用 14px，图文间距为 6px。`MaterialIcon` 在按钮内由公共样式定尺寸，`AppIcon` 显式传 `size={14}`。
- 主按钮使用 --action-primary-*，不能用 --focus-outline 填充。
- 异步操作必须有 busy/submitting 状态：禁用重复提交、显示 spinner、保留失败反馈。

### DropdownSelect

- 所有表单下拉框必须使用 DropdownSelect。下拉框与按钮并排的表单操作行默认对齐为 36px。下拉框使用默认高度，按钮使用 `size="md"` 或默认尺寸。
- 禁止在业务组件中新增原生 select。
- macOS 原生外壳和 Windows/Linux 自绘 Portal 都必须保持同一套语义变量和自适应箭头尺寸。
- 聚焦与展开态沿用 Input 的中性双层阴影，不额外设置彩色 outline 或改变边框为强调色。原生 select 的焦点画在外壳，菜单项键盘高亮和选择行为保持现有实现。

### Input

- 背景、边框、文字和错误状态只使用 --surface-input、--border-_、--text-_、--status-danger*。
- 不要用 !important 解决普通校验状态的 selector 问题。
- 普通聚焦使用 `outline: none` 和 `box-shadow: var(--input-focus-shadow)`；不通过聚焦 border-color 产生新的蓝/黑描边。颜色依次来自四主题/运行时 `--ref-input-focus-inset`、`--ref-input-focus-outer`，经 semantic.css 组合；自定义 accent 不应染色该中性阴影。
- 复合 Input、搜索框和筛选框只在外壳反馈，内部输入保持无环；不修改 Monaco/xterm 内部渲染焦点。
- 深色主题不绘制外部深色阴影层，亮色可用轻量外阴影；终端底部命令输入栏（`terminal-dock-input-shell`）保持原样。
- 路径/筛选与运行时隧道聚焦时只使用既有单层 1px 中性边框，不叠加 inset 实线或外阴影；聚焦时指针移入/移出粗细和颜色一致，非聚焦 hover 不变。
- 命令代码区复用包含行号列的完整 1px 外框，左边框位于数字左侧并与上下边框连通；不能单独给 textarea 加第二圈。运行时隧道字号消费与连接表单一致的 `--font-size-sm`。
- 保留原 hover 的颜色、边框、背景与过渡；不能用全局重置覆盖 hover，特别是文件工具栏“本地文件 / 跟随终端 / SFTP”的悬停蓝框。默认/聚焦/hover/hover+聚焦应分开验收。
- 禁用和错误校验保持原语义；按钮、单选、复选、滚动条的键盘焦点不能作为输入框一起清除。
- 使用外阴影的输入外壳要在所有 overflow 裁剪祖先内预留绘制空间，不能贴着裁剪边界；留白在非聚焦状态就应存在，避免聚焦后移动布局。管理器搜索框右侧保留 12px 安全留白，顶部也必须留足绘制空间；滚动表单用 scroll-padding 保留焦点滚动余量，复合搜索框内层 input 不重复绘制阴影。
- 强制色彩模式不显示 box-shadow，因此仅在 `forced-colors: active` 下允许 `CanvasText` 系统描边回退及用于抵消历史主题重置的 `!important`。这属于高对比度平台兼容，不适用于普通主题样式。

### Dialog 和危险操作

- 删除、清空、覆盖、断开等破坏性操作必须使用 ConfirmActionDialog。
- 禁止在桌面 WebView 中使用 window.confirm()。
- Dialog 的 surface、边框、阴影和按钮都通过语义变量获得。

### 图标规范与资产分类

- **应用身份与 UI 图标严格隔离**：
  - 应用本体身份图标（`apps/tauri/src-tauri/icons/*` 平台图标、`apps/tauri/assets/icons/trayTemplate.svg` 托盘模板、`apps/tauri/public/icon.png` Favicon）供操作系统与外壳使用，严禁与 UI 业务图标混合存放。
  - 界面操作与功能图标一律就地归档于 `apps/tauri/src/renderer/assets/icons/`，且必须按功能域归类到子目录：
    - `actions/`：操作与编辑（增删改查、保存、刷新、排序等）
    - `navigation/`：页面导航与窗口（前进后退、折叠、全屏、关闭等）
    - `files/`：文件与存储（文件夹、文件类型、上传下载等）
    - `network/`：连接与终端（终端模拟、局域网、DNS、云同步等）
    - `security/`：安全与权限（锁、密钥、盾牌、指纹、可见性等）
    - `system/`：系统与偏好（设置、调色板、语言、更新、日志等）
- **本地 SVG 规则**：应用自身的静态 UI 图标必须来自仓库内独立 SVG 文件，存放在 `apps/tauri/src/renderer/assets/icons/<功能域>/`；优先复用 `<AppIcon />` 或 `<MaterialIcon />` 已注册的素材。缺少素材时新增 SVG 并接入对应注册表，保留原有图形与显示尺寸。
  - 禁止在 TSX/CSS 中直接画静态图标路径、用字符或 emoji 代替，也禁止用 WebFont、网络/CDN 作为图标来源。SVG 应继承 `currentColor`；有意使用多色或固定品牌色的现有素材保留原设计。动态图表和第三方编辑器内部图标不属于静态 UI 图标。
  - 操作系统 Dock、任务栏、启动器和托盘图标仍按应用身份资产管理，不放入 renderer UI 图标目录。

### 滚动区域

- 新增纵向滚动区域默认复用 features/common/vertical-scrollbar.tsx，并隐藏容器原生纵向滚动条。
- 横向滚动、第三方编辑器内部滚动和协议组件自带滚动可以保留专用实现。

### StatusIndicator 和系统指标

- StatusIndicator 的状态、尺寸和可访问性语义必须保持一致；装饰性状态点使用 aria-hidden。
- CPU、交换、内存风险阈值与内存分段色是两个概念，不要用一套颜色覆盖另一套信息。

公共选择控件规则：

- 多选或独立开关使用 `<SelectionControl type="checkbox">`；互斥选择使用 `<SelectionControl type="radio">`，同一组 radio 必须共享 `name`。
- 默认使用 `size="default"`；只有 18px 高密度场景才使用 `size="large"`。会话目标选择器和终端 dock 这类紧凑目标列表使用 `className="selection-control--target"`，不在业务 CSS 中复制一套圆点样式。
- 需要“全选但部分选中”时通过 ref 设置原生 `indeterminate`，不要用第三种伪造状态替代 checkbox 语义。
- `SelectionControl` 保留原生 input 的键盘、表单和辅助技术语义；业务层只负责状态与布局。自定义轨道式开关（例如递归操作开关）可以继续使用隐藏原生 input + track，但不应冒充圆点选择控件。
- 不得在 feature CSS 中重新写 `appearance: none`、圆形边框、中心圆点、checked/focus 状态；需要尺寸或上下文差异时扩展公共组件的 size/skin/token。

- **紧凑系统指标磁盘选择器例外**：系统侧栏磁盘容量行中的 `disk-select` 仍必须使用 `<DropdownSelect>`，以保留键盘和跨平台切换语义；由于该行只有约 28px 宽、挂载点本身已经是可识别的交互文本，可通过 `hideArrow` 隐藏视觉箭头。不得把这个例外扩展到普通表单或其他下拉框；macOS 原生壳与 Windows/Linux 自绘触发器仍必须验证点击、键盘和菜单定位。
