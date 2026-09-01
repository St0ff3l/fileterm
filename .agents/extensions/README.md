# Extensions

这里用来存放新功能或新协议的扩展说明，建议一项功能一份文档。

每份扩展说明建议包含：

## 1. 背景

- 要解决什么问题
- 为什么现在要做

## 2. 范围

- 本次会做什么
- 明确不做什么

## 3. 影响目录

- `packages/core`
- `apps/tauri/src-tauri/src`（Rust backend）
- `apps/tauri/src/bridge`（Tauri IPC 边界）
- `apps/tauri/src/renderer`

## 4. IPC 设计

- 新增哪些调用
- 输入输出类型是什么

## 5. 会话或数据模型变化

- 新增类型
- 旧类型是否受影响

## 6. 验收标准

- 用户可以完成什么操作
- 哪些状态需要可见

建议命名方式：

- `ssh-tunnel.md`
- `transfer-center-v2.md`
- `connection-groups.md`
