> 归档状态（2026-07-23）：Electron 与 Tauri renderer 物理拆分、CI 收敛至 Tauri、Tauri 协议夹具串行化已完成。Tauri 发布前真机启动验证和旧路径清理中部分事项因产品决策暂停。本文档移至 `docs/plans/completed/`。

# 双运行时前端物理分叉计划

## 目标

完成 Electron 与 Tauri 的 renderer 物理拆分后，冻结 Electron 为历史参考；仅维护、构建和发布 Tauri。

## 已完成

- [x] `apps/desktop` 重命名为 `apps/tauri`。
- [x] 从 `origin/main` 冻结 Electron 基线到 `apps/electron`。
- [x] Tauri 保留自己的 `src/renderer` 与 `src/bridge/tauri-api.ts`。
- [x] Electron 保留自己的 `src/main`、`src/preload` 与 `src/renderer`。
- [x] 使用独立开发端口、包名、bundle ID、发布产物名和 userData 根。

## 待完成

- [x] 重新生成根 `package-lock.json`，将两个 workspace 的依赖写入锁文件。
- [x] 根命令、CI 与发布工作流已收敛至 Tauri；Electron 不再参与自动构建或测试。
- [x] 将 Tauri 的真实协议夹具测试串行化，避免并发启动本地 OpenSSH fixture 时互相干扰。
- [ ] 验证 Tauri 的完整质量门禁与发布前真机启动；Electron 仅作为人工代码参考。
- [ ] 清理剩余历史文档的旧 `apps/desktop` 路径，或明确标记为历史快照。
- [x] 定义跨端功能节奏：新功能默认只进指定 runtime；双端需求在两个 app 分别实现并用 `packages/*` 稳定契约校验。

## 运行命令

```bash
npm run dev # 默认启动 Tauri/Rust
npm run dev:tauri
npm run dev:electron
npm run build:tauri
npm run build:electron
npm run test:tauri
npm run test:electron
```

## 数据边界

Tauri 与 Electron 不共享可写 userData。Tauri 首次启动时可从旧 Electron userData
执行一次带版本 marker 的导入：Tauri 已有记录优先，legacy 只补缺失 ID，整批写入失败
则回滚且不落 marker。迁移成功后禁止 live merge；后续比较或交换数据只能通过显式
导入导出或专门同步协议完成，避免 JSON repository、secret 文件和 transfer journal
并发写入。
