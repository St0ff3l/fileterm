# 连接管理器：分组与拖拽排序功能设计草案

## 1. 需求概述
为连接管理器添加以下功能：
1. **新建文件夹（分组）**：允许用户创建文件夹来对主机进行归类。
2. **拖动排序**：支持通过拖拽调整连接和文件夹的顺序。
3. **拖拽入文件夹**：支持将连接项拖拽放入文件夹中，实现层级嵌套。

## 2. 影响层级
- `packages/core`：需要扩展数据模型，增加对“文件夹”类型和层级/排序信息的支持。
- `packages/storage`：需要持久化存储层级结构和顺序。
- `apps/desktop/src/main/services`：需要增加对文件夹的 CRUD 操作，以及调整顺序/父级状态的方法。
- `apps/desktop/src/main/ipc` & `preload.cts`：新增分组和排序相关的 IPC 接口。
- `apps/desktop/src/renderer`：`ConnectionManagerModal` 组件需要引入 HTML5 原生拖拽 API（`draggable`, `onDragStart`, `onDragOver`, `onDrop`），并渲染树形或带缩进的列表视图，增加“新建文件夹”按钮。

## 3. 数据模型设计 (`packages/core/src/index.ts`)

### 方案 A：平铺 + `parentId` 和 `order`（推荐）
```typescript
export interface BaseEntity {
  id: string
  name: string
  parentId?: string // 为空则在根目录
  order: number     // 排序权重
}

export interface ConnectionFolder extends BaseEntity {
  type: 'folder'
  isExpanded?: boolean // 渲染层状态，可选保存
}

export interface BaseProfile extends BaseEntity {
  type: SessionType // 'ssh' | 'ftp'
  host: string
  port: number
  // ... 其他属性
}
```
**优点**：存储结构最平缓，后续扩展任意嵌套层级很容易，React 渲染时只需在内存里转换为树形结构。

## 4. 实施步骤
1. **Model 层**：在 `@fileterm/core` 引入 `ConnectionFolder` 模型，并在 `BaseProfile` 中添加 `parentId` 和 `order`。
2. **Storage 层**：让 `file-profile-repository.ts` 能存取含有 folder 节点的数据结构，或者专门维护一份 `folders.json`，但推荐存在一起（`profiles.json` -> `entities.json`，或在现有的 `profiles` 数组里混合存放 `folder`）。
3. **IPC 层**：
   - `createFolder(name, parentId)`
   - `updateEntityOrder(id, newParentId, newOrder)`
4. **UI 层**：
   - 增加“新建文件夹”按钮和弹窗。
   - 列表渲染由原本的 `.map` 升级为树形遍历，给文件夹行添加展开/折叠功能。
   - 每行增加 `draggable={true}`，实现拖拽交换位置和拖入文件夹的高亮反馈。

## 5. UI 细节设计
- **新建文件夹**：在“新建连接”旁边放一个按钮，点击弹出极简的输入框（仅名称）。
- **拖拽交互**：
  - `onDragStart` 记录被拖拽节点的 ID。
  - `onDragOver` 计算目标位置（是在上方、下方、还是悬浮在文件夹上准备放入）。
  - 给被命中的行添加顶部、底部蓝线或背景高亮，提示拖放目标。
  - `onDrop` 触发 IPC 调用更新状态。
