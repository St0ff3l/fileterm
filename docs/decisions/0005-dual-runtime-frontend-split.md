# ADR-0005: 历史桌面实现与 Tauri 前端物理分叉

> 历史决策记录：双运行时拆分已完成收口，当前仓库只维护 Tauri；下方的第二个 app 仅表示迁移时期的历史边界。

## 状态

Accepted（2026-07-15）

## 背景

Tauri 迁移期间曾让 Rust bridge、原生窗口规则和 React renderer 共处于
`apps/tauri`。这会让 Tauri 专用实现反向污染历史桌面实现，也无法同时启动、
独立构建或可靠比较两个运行时。

## 决策

仓库拆成两个独立的桌面 app：

```txt
apps/tauri/       # Tauri CLI、Rust backend、Tauri bridge、Tauri renderer
（迁移前历史 app，已移除）/ # 迁移前的 main、bridge、renderer 与 tests
packages/*        # 仅领域类型、纯工具、稳定数据格式
```

- Tauri 与历史桌面实现的 React、CSS、字体和静态资源物理分叉；不得跨 app
  import UI、hooks、bridge 或窗口层代码。
- `packages/core`、`packages/shared`、`packages/storage` 只承载不依赖某个
  runtime 的领域模型、纯工具和存储格式。
- 两套 app 使用不同开发端口、bundle ID、发布产物名和 userData 根目录；
  不允许并发写入同一组 profile、secret 或 transfer journal 文件。
- 新功能默认只进入明确指定的 runtime；需要双端支持时，在两个 app 中分别
  实现，并以 `packages/*` 的稳定契约校验数据兼容性。

## 影响

Tauri 可以继续按 Rust 原生窗口和 bridge 优化，不会伤到历史桌面实现；迁移前实现
可以作为兼容基线与回归对照。代价是 UI 修复不再自动双端同步，跨端功能
需要显式安排两次实现和两套验证。
