# React 更新循环排查

## 触发场景与证据边界

用户最初反馈终端右键粘贴、监控侧栏反复展开时出现 React #185，随后确认“启动后立即连接，右键菜单还没点粘贴就崩溃”。开发版显示完整错误 `Maximum update depth exceeded`。

Armbian 身份校验遗漏导致监控能力被禁用，继而触发旧的强制折叠规则。这条兼容性回归已由反馈与既有日志确认，修复见 [macOS 侧栏与监控恢复](../plans/completed/macos-sidebar-monitoring-recovery.md)。它解释了侧栏为何点不开，但单独不能证明 React 无限更新的成因。

2026-10-03 13:37:59（Asia/Kuala_Lumpur）的开发版日志再次指向 `ContextMenu` 布局定位中的 `dispatchSetState`，组件栈为 `ContextMenu → TerminalContextMenu → TerminalView → SessionWorkspace`。当时相同坐标保留原 state 的保护已经生效，说明仅验证没有多余 commit，不足以证明不会触发嵌套更新限制。此前的简化测试也没有覆盖启动后立即连接和右键的顺序；上一轮对修复有效性的判断过早。

最终删除通用菜单的定位 state，布局测量只更新该 DOM 节点的 left/top，不再进入 React 更新链。关闭监听器保持挂载/卸载生命周期，通过 ref 调用最新回调。用户重新加载当前开发版后反馈“好像正常了”；这属于开发版初步复测，仍不代表正式版发布验证。

生产 React、实际 TerminalView/xterm 与 SystemSidebarShell 的组合夹具只替换桌面 IPC。旧菜单和早期相等保护在简化场景下没有还原完整 #185；实际开发版堆栈与用户复测才是本次菜单修复的关键证据。不能宣称侧栏点击的独立崩溃已经复现或最终归因。

## 扩大排查

扫描 Renderer 的 225 个 useEffect/useLayoutEffect 调用，重点审核 DOM 测量、依赖自身状态的同步 effect、父子回调身份、焦点清理与回调 ref：

- 通用菜单、DropdownSelect、会话发送目标浮层的定位更新。
- 侧栏折叠、宽度恢复、监控开关、磁盘选择、网络图表和共享滚动条。
- 标签清理、工作区保留状态、分屏权重、文件面板尺寸和文件选择同步。
- 设置切换与偏好持久化、连接表单初始化、保存的代理/隧道同步、命令目标选择和终端查找。

未发现组件/hook 在渲染期间直接调用其本地 useState/useReducer setter。回调 ref 设置 checkbox 的 indeterminate，不写 React 状态。文件选择、图表与文件面板等既有同步中存在相等保护；稳定依赖或一次性门控也使部分同步不会反复自触发。静态扫描不能单凭保护存在就判断绝对安全，需要结合实际回调和状态来源。

### 新确认的表单循环

连接表单的 `updateForm` 原来每次父渲染都会重建。保存的代理、隧道同步 effect 都依赖这个回调，并且无条件创建新 form：effect → 新 form → 父渲染 → 新回调 → effect，形成持续更新。

浏览器夹具直接使用 `useWorkspaceModals` 和实际代理/隧道组件：当前未修复代理路径的渲染计数在两个采样点从 1612 增到 3914；替换为 v2.2.19 的隧道路径从 1739 增到 3926（均只间隔 4 个动画帧）。这些数字是一次夹具观测，不是固定性能指标。两条路径都确认持续更新；夹具没有单独触发与终端完全相同的致命 #185。

修复用 useCallback 稳定 `updateForm`，代理字段及隧道规则在调用 setter 前比较实际值。规则比较覆盖 id、name、kind、bindHost/Port、targetHost/Port、autoStart。相同配置刷新不再写父级 form；配置真正变化仍正常同步。

### 浮层定位加固

| 位置                    | v2.2.19 对照                                | 最终处理                                             |
| ----------------------- | ------------------------------------------- | ---------------------------------------------------- |
| DropdownSelect 自绘菜单 | 相同内容和位置的 50 次父更新产生 100 次提交 | 删除定位 state；在应用触发器宽度后测量并直接定位 DOM |
| SessionSendTargetPicker | 相同内容和位置的 50 次父更新产生 100 次提交 | 删除定位 state；布局、滚动与 resize 回调直接定位 DOM |

两处均未独立复现致命 #185，属于消除已验证的重复提交和同类定位更新链路。保留展开、选择及自适应箭头等交互状态。

## 回归与质量检查

- `tests/browser/terminal-sidebar-interactions.mjs`：生产 React；连接后立刻在中心与边缘右键、不执行粘贴；随后 40 次真实右键粘贴、合计 360 次菜单打开期间的父更新、40 轮侧栏折叠/展开，覆盖监控正常与 unsupported。检查粘贴只读一次剪贴板、只发送一次输入、焦点回到 xterm、无 React 错误。
- `tests/browser/connection-form-sync.mjs`：实际表单 hook、代理与隧道组件；表单同步后停止渲染，两类配置各 10 次相同库刷新不写父级，改变端口仍传播。
- `tests/browser/popup-update-stability.mjs`：浮层打开/定位只提交一次；options/targets/selectedTabIds 使用新数组，共 100 次父更新只产生相应父提交；移动触发器仍重定位、320px 宽触发器在边缘不越界、点击外部能关闭。
- `tests/browser/context-menu-updates.py`：打开/边缘夹紧只提交一次；100 次菜单父更新、视口边缘定位、焦点与最新 Escape 回调。
- `tests/browser/system-sidebar-toggle.mjs`：三种平台分支、监控正常/禁用/未知、存在/缺失样本，共 180 轮侧栏切换。

终端组合、表单同步和浮层稳定性脚本在 Chromium 和 WebKit 均通过。Renderer 35 个测试、typecheck、Lint、Prettier、CSS contract 和 Renderer 生产构建通过。当前工作区 Rust 679 个单元测试及 31 个集成/contract 测试、Clippy 已通过；此轮后续只修改 Renderer，没有重复执行未改变的 Rust 检查。CSS contract 保留原有 11 处 !important 和约 1 处旧直接颜色值警告，本次没有增加样式债务。

浏览器脚本可通过 ESBUILD_MODULE_PATH、PLAYWRIGHT_MODULE_PATH 指定外部测试工具。Chromium 可用 PLAYWRIGHT_CHANNEL=chrome；三个新脚本支持 PLAYWRIGHT_ENGINE=webkit，表单脚本可用 FORM_SYNC_KIND=proxy/tunnel 分别运行对照。无需向项目依赖或 lockfile 添加浏览器工具。

原生 Tauri WKWebView 与真实 Armbian 会话仍需发布后持续验证。若再次出现 #185，应查看同一次故障的 renderer error componentStack 与 JS stack，不应仅凭错误编号归因于某个组件。开发日志另有 xterm renderer dimensions 的历史异常，尚未确认与本次 #185 的因果关系，不作为本次已修复结论。

## 文件规模

本次 Renderer 业务文件均为非豁免类别且低于 800 行，无需拆分：ContextMenu 185 → 192、DropdownSelect 297 → 287、SessionSendTargetPicker 251 → 237、useWorkspaceModals 247 → 251、ConnectionProxySection 315 → 322、ConnectionTunnelSection 278 → 301。浏览器测试与本文属于测试/文档豁免类别。
