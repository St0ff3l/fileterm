# 公共控件与资产约束

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
- **矢量就地化与样式规范**：
  - 所有 UI SVG 统一设置 `fill="currentColor"`，使其自然跟随父级 CSS 语义文字色、`--folder-accent` 或交互伪类变色。
  - 图标组件优先使用离线预置的 `<AppIcon />`，禁止新增 `<span className="material-symbols-outlined">` 等依赖外部 WebFont 的实现，彻底消除 FOUT 连字字符闪烁。

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
