# Rust + Tauri 重构路线

## 1. 目标与不可破坏的边界

FileTerm 从 Electron + Node.js 主进程逐步迁移到 Rust + Tauri，同时保留现有 React UI。

必须保留：

- `apps/desktop/src/renderer` 下的页面、feature、hooks、CSS、主题和字体资源。
- React + TypeScript + Vite、xterm.js、Monaco Editor。
- 当前工作区布局、连接管理器、命令管理器、文件编辑器和传输中心交互。
- `packages/core` 中的领域概念和现有 profile/transfer 数据格式。
- `main -> preload -> renderer` 形成的安全边界，在 Tauri 中等价迁移为 `Rust commands/events -> TypeScript bridge -> renderer`。

不在第一阶段同时做：

- UI 重写或视觉改版。
- Zustand、SQLite、全新组件库或新的主题系统。
- SSH、FTP、Telnet、Serial 的协议统一改造。
- 凭据存储策略改造。

## 2. 目标架构

```txt
现有 React UI
  -> window.fileterm 兼容 API
    -> Tauri adapter
      -> invoke / listen
        -> Rust commands
          -> Rust services / session controllers / transfer system
            -> SSH / SFTP / FTP / Telnet / Serial / local filesystem
```

renderer 继续依赖 `window.fileterm`，不直接调用 Tauri API。迁移期间同时保留 Electron adapter 和 Tauri adapter，让同一套 UI 可以在两种桌面壳中运行。

## 3. 仓库调整方向

建议新增：

```txt
apps/desktop/src/bridge/
  fileterm-api.ts
  electron-api.ts
  tauri-api.ts

apps/desktop-tauri/
  src/                         # 复用现有 renderer 的 Vite 入口
  src-tauri/
    src/
      commands/
      services/
      sessions/
      transfers/
      storage/
      platform/
```

第一阶段不移动 React 文件；只把现有 renderer 的 API 依赖收敛到 bridge。等 Tauri 垂直切片稳定后，再决定是否把 UI 目录提取为独立 workspace package。

## 4. 迁移阶段

### Phase 0：契约冻结与双运行时 bridge

- [ ] 梳理并按领域拆分 `FileTermDesktopApi`。
- [ ] 固定 command、event、错误和 secret 脱敏约定。
- [ ] 为 Electron 实现 adapter，确保现有行为不变。
- [ ] 添加 Tauri adapter 的空实现/探测入口，不接入业务。
- [ ] 建立 UI contract test，验证两个 adapter 的 API 形状一致。

验收：现有 Electron 版本功能和测试全部通过，renderer 不再直接依赖 Electron 类型。

### Phase 1：Tauri 桌面壳垂直切片

- [ ] 主窗口和开发/生产资源加载。
- [ ] macOS `hiddenInset`、traffic light 避让。
- [ ] Windows 无边框标题栏、drag/no-drag 区域。
- [ ] Linux 窗口基础行为。
- [ ] 托盘、Dock、应用图标和离线资源。
- [ ] 窗口最小尺寸、最大化、关闭和隐藏。
- [ ] `Cmd+Q`、`Ctrl+Q`、托盘退出统一进入同一条确认链路。
- [ ] 剪贴板、外部链接、文件/目录选择器。

验收：同一套 React UI 在 Tauri 壳中启动，页面视觉无意外变化；三平台窗口和退出链路有手测记录。

### Phase 2：Rust 存储与 Workspace

- [ ] 迁移 profile、folder、command、UI preferences、UI state。
- [ ] 兼容现有 JSON 文件和旧用户目录迁移。
- [ ] 保留 `group` / `parentId` 双向自愈。
- [ ] 迁移 workspace snapshot、tab 生命周期和连接库。
- [ ] 保留 secret 不进入公开 snapshot 的规则。

验收：旧 Electron 用户数据可被 Tauri 读取；创建、编辑、删除和排序行为一致。

### Phase 3：SSH 工作区主链路

- [ ] SSH shell controller。
- [ ] 终端 write、resize、data/state events。
- [ ] SFTP 目录、读写、编辑和权限操作。
- [ ] CWD、远端用户、sudo/root 同步。
- [ ] Linux / BusyBox / Windows 系统指标采集及 CRLF 归一化。
- [ ] host verification、keyboard-interactive/MFA。
- [ ] proxy、Jump Host 和 SSH tunnel。

Rust controller 必须继续与 FTP、Telnet、Serial 分离；只复用明确的生命周期和事件接口。

### Phase 4：其他协议与 Transfer

- [ ] FTP/FTPS。
- [ ] Telnet。
- [ ] Serial。
- [ ] 统一 TransferService、journal、暂停/恢复、取消和退出清理。
- [ ] 断线、tab 关闭和应用退出时的资源回收。
- [ ] WebDAV 配置同步。

验收：协议测试、controller 测试、传输协议测试和真实设备手测全部通过。

### Phase 5：发行与切换

- [ ] Tauri updater、签名、公证和安装包。
- [ ] macOS DMG/zip、Windows NSIS/portable、Linux 包格式评估。
- [ ] 性能、内存、启动时间和终端延迟对比 Electron。
- [ ] 迁移工具和失败回滚。
- [ ] 先灰度发布 Tauri，Electron 保留为可回滚版本。

## 5. 技术决策

### 前端 API

保留现有 `window.fileterm` 方法名和主要 payload 结构。高频终端输出、传输进度和 workspace snapshot 使用事件，不使用 renderer 轮询。

### Rust 类型

第一阶段由 Rust 使用与 `packages/core` 对应的 `serde` 类型；不要在迁移初期同时改造领域模型。协议稳定后，再考虑用 JSON Schema 或代码生成维护 TypeScript/Rust 契约。

### 存储

第一阶段继续使用 JSON 文件，采用临时文件写入和原子 rename。SQLite、系统钥匙串和新的 secrets backend 另立计划。

### 依赖方向

候选依赖包括 `tokio`、`serde`、`thiserror`、`russh`/`ssh2`、`suppaftp`/`async-ftp`、`tokio-serial`、`portable-pty`、`reqwest`。每种协议先做跨平台 PoC，再锁定 crate，避免先绑定实现再发现 Windows/macOS 构建问题。

## 6. 质量门槛

每个阶段必须满足：

- Electron adapter 现有测试不回归。
- Tauri command 有输入校验、结构化错误和取消/关闭处理。
- secret 不出现在日志、公开 snapshot 或 renderer 事件中。
- macOS、Windows、Linux 的窗口、托盘、标题栏和退出行为分别验证。
- 生产构建验证资源路径、字体、图标和 worker/Monaco 加载。
- UI 截图回归覆盖深色/浅色主题、中文/英文、主窗口和关键子窗口。

## 7. 第一批实施任务

1. 抽取 `FileTermDesktopApi` 到独立 bridge 类型。
2. 为现有 Electron preload 接入 `electron-api.ts`，功能保持不变。
3. 建立 `apps/desktop-tauri` 最小 Tauri v2 壳。
4. 让 Tauri 壳加载同一份 React renderer。
5. 实现平台信息、UI preferences、剪贴板、窗口控制和文件选择器。
6. 加入主窗口/托盘/退出链路的跨平台验收记录。
7. 再开始迁移 Rust profile repository 和 workspace snapshot。

## 8. 回滚策略

任何阶段都保留 Electron adapter 和 Electron 构建入口。若 Tauri 某个协议、窗口能力或平台打包出现问题，默认切回 Electron，不回退 UI 文件，不回退用户数据迁移结果；数据迁移必须先备份且幂等。
