# Rust + Tauri 重构路线（单运行时）

> 当前状态（2026-07-14）：Phase 0–4 实现已完成；macOS 的真实协议、生产打包和性能基线已通过，Windows/Linux socket 生命周期由 CI 矩阵验证。签名应用内 updater 和发行公证仍属于 Phase 5 外部发布前置。实际 Rust 工程路径为 `apps/desktop/src-tauri`；本计划中的早期目录示例已按实际仓库路径修正。

## 1. 目标与不可破坏的边界

FileTerm 从 Electron + Node.js 主进程逐步迁移到 Rust + Tauri，同时保留现有 React UI。

必须保留：

- `apps/desktop/src/renderer` 下的页面、feature、hooks、CSS、主题和字体资源。
- React + TypeScript + Vite、xterm.js、Monaco Editor。
- 当前工作区布局、连接管理器、命令管理器、文件编辑器和传输中心交互。
- `packages/core` 中的领域概念和现有 profile/transfer 数据格式。
- `main -> preload -> renderer` 形成的安全边界，在 Tauri 中迁移为 `Rust commands/events -> TypeScript bridge -> renderer`。

不在第一阶段同时做：

- UI 重写或视觉改版。
- Zustand、SQLite、全新组件库或新的主题系统。
- SSH、FTP、Telnet、Serial 的协议统一改造。
- 凭据存储策略改造。

## 2. 目标架构

```txt
现有 React UI
  -> typed Tauri bridge
      -> invoke / listen
        -> Rust commands
          -> Rust services / session controllers / transfer system
            -> SSH / SFTP / FTP / Telnet / Serial / local filesystem
```

renderer 通过 `tauri-api.ts` 使用类型化 command/event bridge，不直接在业务组件中调用 Tauri API。

## 3. 仓库调整方向

建议新增：

```txt
  apps/desktop/src/bridge/
    tauri-api.ts

apps/desktop/src-tauri/
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

第一阶段不移动 React 文件；只把现有 renderer 的 API 依赖收敛到 Tauri bridge。Electron 不再作为运行时、adapter 或回滚方案。

## 4. 迁移阶段

### Phase 0：Tauri 直连骨架与基础能力

- [x] 梳理并按领域拆分 `FileTermDesktopApi`。
- [x] 固定 command、event、错误和 secret 脱敏约定。
- [x] 建立 `apps/desktop/src-tauri` Tauri v2 工程。
- [x] 建立唯一的 `tauri-api.ts` bridge，React 不直接散落调用 `invoke/listen`。
- [x] 实现平台信息、剪贴板、UI preferences/state。
- [x] 建立 Rust command/event contract test；不再维护 Electron adapter。

验收：现有 Electron 版本功能和测试全部通过，renderer 不再直接依赖 Electron 类型。

### Phase 1：Tauri 桌面壳垂直切片

- [x] 主窗口和开发/生产资源加载。
- [x] macOS `hiddenInset`、traffic light 避让。
- [x] Windows 无边框标题栏、drag/no-drag 区域。
- [x] Linux 窗口基础行为。
- [x] 托盘、Dock、应用图标和离线资源。
- [x] 窗口最小尺寸、最大化、关闭和隐藏。
- [x] `Cmd+Q`、`Ctrl+Q`、托盘退出统一进入同一条确认链路。
- [x] 剪贴板、外部链接、文件/目录选择器。

验收：同一套 React UI 在 Tauri 壳中启动，页面视觉无意外变化；三平台窗口和退出链路有手测记录。

### Phase 2：Rust 存储与 Workspace

- [x] 迁移 profile、folder、command、UI preferences、UI state。
- [x] 兼容现有 JSON 文件和旧用户目录迁移。
- [x] 保留 `group` / `parentId` 双向自愈。
- [x] 迁移 workspace snapshot、tab 生命周期和连接库。
- [x] 保留 secret 不进入公开 snapshot 的规则。

验收：旧 Electron 用户数据可被 Tauri 读取；创建、编辑、删除和排序行为一致。

### Phase 3：SSH 工作区主链路

- [x] SSH shell controller。
- [x] 终端 write、resize、data/state events。
- [x] SFTP 目录、读写、编辑和权限操作。
- [x] CWD、远端用户、sudo/root 同步。
- [x] Linux / BusyBox / Windows 系统指标采集及 CRLF 归一化。
- [x] host verification、keyboard-interactive/MFA。
- [x] SOCKS5/HTTP CONNECT 代理、单级 Jump Host 和运行时 SSH `-L/-R/-D` tunnel。

Rust controller 必须继续与 FTP、Telnet、Serial 分离；只复用明确的生命周期和事件接口。

### Phase 4：其他协议与 Transfer

- [x] FTP/FTPS（plain/explicit/implicit TLS）。
- [x] Telnet（RFC 854 IAC/NAWS/TERMINAL-TYPE 与代理）。
- [x] Serial（tokio-serial；不支持的 mark/space parity 明确报错）。
- [x] 统一 TransferService、journal、目录 manifest、暂停/恢复、取消和退出清理。
- [x] 断线、tab 关闭和应用退出时的 worker/transfer 资源回收。
- [x] WebDAV 配置同步（上传/下载、ETag、hash、secret stripping）。
- [x] JSON/SSH config 导入导出、命令历史/发送偏好、编辑器关闭确认、窗口事件、CSP、日志。

验收：Rust 26 项单测（含真实 TCP FTP round-trip）、macOS 本机 OpenSSH/FTP/FTPS 7 项协议测试、Tauri 生产打包和 macOS socket 生命周期测试均通过；Windows/Linux 使用 CI socket matrix。详见 `docs/quality/tauri-phase4-validation.md`。

### Phase 5：发行与切换

- [ ] Tauri updater、签名、公证和安装包。
- [ ] macOS DMG/zip、Windows NSIS/portable、Linux 包格式评估。
- [ ] 性能、内存、启动时间和终端延迟对比 Electron。
- [ ] 迁移工具和失败回滚。
- [ ] 直接发布 Tauri；迁移失败通过数据备份和 command 级回滚处理，不保留 Electron 运行时。

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

- Tauri commands 有输入校验、结构化错误和取消/关闭处理。
- Tauri command 有输入校验、结构化错误和取消/关闭处理。
- secret 不出现在日志、公开 snapshot 或 renderer 事件中。
- macOS、Windows、Linux 的窗口、托盘、标题栏和退出行为分别验证。
- 生产构建验证资源路径、字体、图标和 worker/Monaco 加载。
- UI 截图回归覆盖深色/浅色主题、中文/英文、主窗口和关键子窗口。

## 7. 第一批实施任务

1. 建立 `apps/desktop/src-tauri` 最小 Tauri v2 壳。
2. 让 Tauri 壳直接加载现有 React renderer。
3. 建立 `tauri-api.ts`，接入平台信息、UI preferences/state 和剪贴板。
4. 加入 Rust command 输入校验、结构化错误和基础 contract test。
5. 再开始迁移 Rust profile repository 和 workspace snapshot。

## 8. 回滚策略

迁移期间不保留 Electron 运行时。若 Tauri 某个协议或平台能力未完成，保持该 Tauri command 明确返回结构化 `unsupported` 错误，不伪造已完成能力；数据迁移必须先备份且幂等。
