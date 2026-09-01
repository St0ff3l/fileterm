> 归档状态（2026-07-23）：迁移前实现 与 Tauri renderer 物理拆分、CI 收敛至 Tauri、Tauri 协议夹具串行化已完成。Tauri 发布前真机启动验证和旧路径清理中部分事项因产品决策暂停。本文档移至 `docs/plans/completed/`。

# 历史桌面实现与 Tauri 前端物理分叉计划

> 历史归档：本计划记录迁移期的双运行时拆分；当前仓库只维护 Tauri，迁移前实现已移除。

## 目标

完成历史桌面实现与 Tauri 的 renderer 物理拆分后，冻结迁移前实现为历史参考；仅维护、构建和发布 Tauri。

## 已完成

- [x] 将当前 Tauri app 固定为 `apps/tauri`。
- [x] 从 `origin/main` 保存迁移前实现作为历史基线。
- [x] Tauri 保留自己的 `src/renderer` 与 `src/bridge/tauri-api.ts`。
- [x] 迁移前实现曾保留独立的 main、bridge 与 renderer。
- [x] 使用独立开发端口、包名、bundle ID、发布产物名和 userData 根。

## 待完成

- [x] 重新生成根 `package-lock.json`，将两个 workspace 的依赖写入锁文件。
- [x] 根命令、CI 与发布工作流已收敛至 Tauri；迁移前实现 不再参与自动构建或测试。
- [x] 将 Tauri 的真实协议夹具测试串行化，避免并发启动本地 OpenSSH fixture 时互相干扰。
- [ ] 验证 Tauri 的完整质量门禁与发布前真机启动；迁移前实现仅作为人工代码参考。
- [ ] 清理剩余历史文档中的旧路径，或明确标记为历史快照。
- [x] 定义跨端功能节奏：新功能默认只进指定 runtime；双端需求在两个 app 分别实现并用 `packages/*` 稳定契约校验。

## 运行命令

```bash
npm run dev # 默认启动 Tauri/Rust
npm run dev:tauri
旧版桌面开发命令（已移除）
npm run build:tauri
旧版桌面构建命令（已移除）
npm run test:tauri
# 迁移前实现的开发、构建和测试命令已移除
```

## 数据边界

Tauri 与迁移前实现不共享可写 userData。Tauri 首次启动时可从历史 userData
执行一次带版本 marker 的导入：Tauri 已有记录优先，legacy 只补缺失 ID，整批写入失败
则回滚且不落 marker。迁移成功后禁止 live merge；后续比较或交换数据只能通过显式
导入导出或专门同步协议完成，避免 JSON repository、secret 文件和 transfer journal
并发写入。
