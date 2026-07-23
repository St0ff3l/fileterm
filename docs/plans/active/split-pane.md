# 分屏（Split Pane）功能计划

## 目标

在 SSH session 的**终端区域内**支持分屏；系统信息侧栏、顶部 tab、文件面板和命令面板保持单一工作区，不随 pane 复制。每个 pane 是**复用当前 profile 新建的一个独立 session**，不共享 PTY。

## 快捷键

| 操作                | macOS         | Windows / Linux                                   |
| ------------------- | ------------- | ------------------------------------------------- |
| 新建标签页          | `Cmd+T`       | `Ctrl+Shift+T`                                    |
| 垂直分屏（左右）    | `Cmd+D`       | Windows: `Alt+Shift+D`；Linux: `Ctrl+Shift+D`     |
| 水平分屏（上下）    | `Cmd+Shift+D` | Windows: `Alt+Shift+-`；Linux: `Ctrl+Alt+Shift+D` |
| 关闭当前 pane / tab | `Cmd+W`       | 平台原生关闭快捷键                                |
| 切换 pane 焦点      | —             | Windows: `Alt+方向键`                             |

注册位置：Tauri 原生菜单 accelerator（View 菜单），与现有 `Cmd+W` 一致。终端聚焦时前端键盘事件会被 xterm 拦截，原生菜单加速器全局有效。

终端右键菜单也显示“垂直分屏 / 水平分屏”及当前客户端对应的快捷键。菜单通过 Tauri bridge 的平台字段区分 macOS、Windows、Linux；从某个 pane 的右键菜单触发时，始终以该 pane 为 source，不依赖此前的焦点。

## 数据模型

### PaneNode（新增，packages/core）

```ts
export type SplitDirection = 'row' | 'column'

export type PaneNode =
  | { kind: 'leaf'; tabId: string }
  | {
      kind: 'split'
      direction: SplitDirection
      children: PaneNode[]
      weights: number[] // 每个子节点的占比，和为 1；拖拽时更新
    }
```

- `row` = 左右分（垂直分屏）
- `column` = 上下分（水平分屏）
- 每个 leaf 引用一个真实 `WorkspaceTab` id

### WorkspaceTab 扩展

`packages/core` 的 `WorkspaceTab` 新增可选字段：

```ts
export interface WorkspaceTab {
  id: string
  sessionType: SessionType
  profileId: string
  title: string
  layout: TabLayout
  status: TabStatus
  paneRoot?: PaneNode // 分屏树根节点；普通 tab 无此字段
  paneRootTabId?: string // leaf 所属 root；leaf 永不显示为顶部 tab
}
```

只有「分屏的根 tab」才持有 `paneRoot`，被引用的 leaf tab 自己不持 `paneRoot`。

### 活跃 pane

`WorkspaceSnapshot` 新增：

```ts
export interface WorkspaceSnapshot {
  // ...
  activePaneTabIdByRoot: Record<string, string> // rootTabId -> 当前活跃 leaf tabId
}
```

用于终端输入、文件操作、命令发送的目标定位。

### 工作区辅助面板归属

- **终端本身与终端命令栏**跟随当前活跃 pane。
- **系统信息侧栏、远端文件面板、文件编辑器与传输中心**固定绑定顶级 root tab 的原始 session，不随 pane 焦点切换。
- 新建 split pane 会启动独立 shell；系统信息和文件面板仍只绑定当前 root。分屏 session 保留资源监控能力，这样原始 root 被关闭并提升其他 pane 时，新的 root 可以继续提供实时系统信息。

## 后端改动

### 新增 command: `app_split_tab`

```rust
#[tauri::command]
pub async fn app_split_tab(
    app: AppHandle,
    source_tab_id: String,
    direction: String,  // "row" | "column"
) -> Result<serde_json::Value, AppError>
```

流程：

1. 读 source tab，拿到 `profile_id`
2. 读 profiles.json，找到 profile
3. 生成新 tab_id，构造 `WorkspaceTab`（layout 同 source）
4. 创建新 session snapshot、worker、terminal input、cancel token（复用 `app_open_profile` 的核心逻辑，抽取 `spawn_session_for_profile` helper）
5. 更新 `paneRoot`：
   - 若 source tab 有 `paneRoot`：把 source 作为 leaf，和新 leaf 组成 split，挂回 source root
   - 若 source tab 无 `paneRoot`：source 变成 root，`paneRoot` = split(source_leaf, new_leaf)
   - **注意**：被分屏的 source tab 本身可能就是某个 root 的 leaf。这时 split 发生在 source 所在的 pane 位置，需要在 pane tree 里替换该 leaf 为 split(source_leaf, new_leaf)
6. 保持 `active_tab_id` 为分屏 root tab，并设置 `activePaneTabIdByRoot[root]` 为新 pane
7. `get_workspace_snapshot_and_emit`

### 新增 command: `app_close_pane`

```rust
#[tauri::command]
pub async fn app_close_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError>
```

流程：

1. 在 `paneRoot` 树中找到 `pane_tab_id`，移除该 leaf
2. 清理 session worker、tabs、sessions（复用 `stop_session_worker`）
3. 规整 pane tree：
   - split 只剩一个子节点时，用唯一子节点替换该 split
   - 若 root 的 paneRoot 只剩一个 leaf，清掉 `paneRoot`，退化回普通 tab
   - 若关闭的是原 root 对应的 leaf，提升一个存活 leaf 为新 root，并重新绑定其余 leaf；绝不保留指向已关闭终端的 root
4. 更新 `activePaneTabIdByRoot`
5. `get_workspace_snapshot_and_emit`

### 新增 command: `app_set_active_pane`

```rust
#[tauri::command]
pub async fn app_set_active_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError>
```

只更新 `activePaneTabIdByRoot[root] = pane_tab_id`。

### 新增 command: `app_set_pane_weights`

```rust
#[tauri::command]
pub async fn app_set_pane_weights(
    app: AppHandle,
    root_tab_id: String,
    pane_path: Vec<usize>,
    weights: Vec<f32>,
) -> Result<serde_json::Value, AppError>
```

拖拽 resize 结束时持久化 weights。`pane_path` 是从根 split 到目标 split 的 child index 路径，避免嵌套的两个二分 split 互相覆盖权重。

### 关闭 tab 链路调整

`app_close_tab` 关闭的若是分屏 root，需要关闭整个 pane tree 的所有 leaf；若是 leaf，等价于 `app_close_pane`。

### snapshot 输出

`get_workspace_snapshot_unlocked` 增加 `activePaneTabIdByRoot` 字段。

## Bridge 改动

`apps/tauri/src/bridge/tauri-api.ts` 新增：

```ts
splitTab: (sourceTabId: string, direction: 'row' | 'column') =>
  invoke<WorkspaceSnapshot>('app_split_tab', { sourceTabId, direction }),
closePane: (rootTabId: string, paneTabId: string) =>
  invoke<WorkspaceSnapshot>('app_close_pane', { rootTabId, paneTabId }),
setActivePane: (rootTabId: string, paneTabId: string) =>
  invoke<WorkspaceSnapshot>('app_set_active_pane', { rootTabId, paneTabId }),
setPaneWeights: (rootTabId: string, panePath: number[], weights: number[]) =>
  invoke<WorkspaceSnapshot>('app_set_pane_weights', { rootTabId, panePath, weights }),
```

`FileTermDesktopApi` 接口同步补齐。

## Renderer 改动

### 新增组件 `SplitPaneLayout.tsx`

位置：`apps/tauri/src/renderer/features/workspace/SplitPaneLayout.tsx`

- 递归渲染 `PaneNode`
- `SessionWorkspace` 保留为共享外壳；leaf 只渲染 `TerminalView`
- split 渲染两个子容器 + 一个 resizer div（复用现有 `session-split-resizer` 样式思路）
- 拖拽 resizer 时只改前端 state，drag end 调 `setPaneWeights`

### WorkspaceStage / SessionWorkspace 改造

```tsx
<SessionWorkspace
  splitRootTab={activeTab?.paneRoot ? activeTab : undefined}
  activeTab={activePaneTab}
  ...
/>
```

`SplitPaneLayout` 仅挂在 `SessionWorkspace` 的 terminal area；根 tab 仍显示在顶部 TabBar，分屏产生的 leaf tab 不单独显示为顶部标签。

### useWorkspaceTabs 扩展

新增 `splitCurrentPane(direction)`、`closePane(tabId)`、`activatePane(tabId)`，调用 bridge。

### 活跃 pane 焦点

- 点击 pane 内任意区域时调 `setActivePane`
- 终端 `TerminalView` 聚焦时也触发 `setActivePane`
- `Cmd+W`：分屏仍有多个 pane 时直接关闭当前活跃 pane，并把焦点交给下一个 pane；只剩一个 pane 时才走顶级 tab 的既有二次确认流程

## 快捷键菜单项

在 `apps/tauri/src-tauri/src/lib.rs` 的 View 菜单加：

```rust
let split_vertical = MenuItemBuilder::with_id(
    "view-split-vertical",
    localized(is_english, "Split Vertically", "垂直分屏"),
)
.accelerator(if cfg!(target_os = "macos") { "Cmd+D" } else { "Ctrl+Shift+D" })
.build(app)?;

let split_horizontal = MenuItemBuilder::with_id(
    "view-split-horizontal",
    localized(is_english, "Split Horizontally", "水平分屏"),
)
.accelerator(if cfg!(target_os = "macos") { "Cmd+Shift+D" } else { "Ctrl+Alt+Shift+D" })
.build(app)?;
```

`on_menu_event` 分发：

```rust
"view-split-vertical" => {
    if let Some(window) = focused_webview_window(app) {
        let _ = window.emit("app:split-pane-request", "row");
    }
}
"view-split-horizontal" => {
    if let Some(window) = focused_webview_window(app) {
        let _ = window.emit("app:split-pane-request", "column");
    }
}
```

bridge 暴露 `onSplitPaneRequest`，renderer 收到后调 `splitCurrentPane(direction)`。

## 边界约束

1. **FTP tab 不支持分屏**：FTP 无终端，快捷键对 `file-only` layout 不响应。
2. **首页 tab / system info tab 不支持分屏**：这些不是 session tab。
3. **tab 标题**：分屏 root 的 tab 标题显示第一个 leaf 的 session 标题。
4. **MaxSessions**：同一 SSH host 多 session 可能触发 MaxSessions 限制，新 session 创建失败时要在 pane 里展示错误，不影响已有 pane。
5. **关闭确认**：关闭分屏中的非最后一个 leaf 不弹窗；关闭最后一个 pane（即关闭顶级 tab）才走 `shortcutCloseConfirm`（如果有活跃连接）。
6. **拖拽 resize**：参考现有 `session-split-resizer` 实现，不引入 `react-resizable-panels`（architecture.md 已决定不引入）。

## 实施阶段

1. **Phase 1**：`packages/core` 加 `PaneNode`、`SplitDirection` 类型，扩展 `WorkspaceTab`、`WorkspaceSnapshot`。
2. **Phase 2**：Rust 后端加 `PaneNode`/`PaneLayout` 镜像类型、`app_split_tab`/`app_close_pane`/`app_set_active_pane`/`app_set_pane_weights` 命令、snapshot 输出扩展。
3. **Phase 3**：bridge 暴露 4 个新 API + `onSplitPaneRequest` 事件。
4. **Phase 4**：renderer `SplitPaneLayout` 组件、`WorkspaceStage` 改造、`useWorkspaceTabs` 扩展。
5. **Phase 5**：拖拽 resize + weights 持久化。
6. **Phase 6**：快捷键菜单项 + i18n。

## 质量门禁

```bash
npm run typecheck -w @fileterm/tauri
npm run lint
npx prettier --check apps/tauri packages/core packages/shared packages/storage
npm run test:tauri
cargo clippy --locked --all-targets --all-features -- -D warnings
```

## 不做的事

- 不引入 `react-resizable-panels` 或 Zustand。
- 不做「同一 session 多视图」（共享 PTY），那个方向输入/光标会互相干扰。
- 不做 pane 之间拖拽重排（后续可加）。
- 不做 FTP / Telnet / Serial 的分屏（先只 SSH）。
