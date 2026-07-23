> 归档状态（2026-07-23）：代码修复、自动化回归、Rust 全仓格式化/严格 Clippy 和 Tauri 生产二进制构建已完成。安装身份、正式签名/公证、Windows 应用内排序与原生文件拖放因外部环境/产品决策暂停。本文档移至 `docs/plans/completed/`。

# Tauri 代码审计修复计划（2026-07-17）

## 背景

基于 `FileTerm-Electron-Tauri-Code-Audit-2026-07-17.md` 对提交
`989a25f95e4a28b3f5ef7aa79171a21f27487e51` 的结论，优先修复 Tauri
运行时的数据完整性、权限契约、传输生命周期和迁移语义问题。

当前状态：**代码修复、本机自动化回归、Rust 全仓格式化/严格 Clippy 和
Tauri 生产二进制构建已完成。发行身份、正式签名/公证、三平台真机拖放和按窗口
分拆自定义 command 权限仍是需要产品选择或外部环境的发布验收项。**

## 已确认边界

- 连接凭据继续使用明文 JSON 文件存储，不引入 macOS safeStorage、系统钥匙串或
  `keyring`。
- `profile-secrets.json` 继续与公开 profile 分离；Unix 文件权限必须为 `0600`，
  Windows 保持 best-effort 的应用数据目录隔离语义。
- Electron 与 Tauri renderer 继续物理分离，不跨 app import。
- `packages/core` 仍是 IPC/领域契约的 single source of truth。
- Transfer 必须在退出、暂停、丢弃前等待当前 run 收敛，并保持 journal 与断点一致。

## 修复批次

### A. 数据与权限安全

- [x] 修复远程文件编辑器 `tabId` payload，并在 remote editor 缺少 `tabId` 时
      fail-closed。
- [x] 修复远程 chmod 的 `mode` 契约与本地 chmod 的 `applyTo` 契约。
- [x] 明文 secrets 写入失败必须传播；Unix 权限收紧为 `0600`；删除 profile 时清除
      孤儿 secret。
- [x] 补 JS/Rust payload、serde 和文件系统副作用测试。

### B. Transfer 完整性

- [x] FTP/FTPS 预清理对 not-found 幂等。
- [x] 每个 transfer 跟踪唯一 run generation/handle；pause、resume、discard、shutdown
      不得并发运行同一任务。
- [x] 对齐 `cleanupPending` / `retryAttempt`，清理失败不得从 journal 丢失。
- [x] 本地断点在启动时自动重试，远端断点在匹配 SSH/FTP 文件通道重连后自动
      重试；journal 全量快照写入串行化。
- [x] 修复目录上传完成后的刷新失败、进度事件节流、速度采样与 runtime map 回收。

### C. 数据、版本与契约

- [x] 将 Electron legacy 数据合并改为有 marker 的一次性迁移，删除数据不得复活。
- [x] `sync:version` 同步 Cargo.toml、Cargo.lock 与 tauri.conf.json。
- [x] 修复字段命名、URL allowlist、bridge 类型保护、runtime metadata bootstrap、
      native-drop fallback 和本地工具忽略项。
- [x] 将 Monaco 固定到已验证且不引入受影响 DOMPurify 链路的版本；`npm audit` 为 0。
- [x] 将 npm/RustSec 依赖扫描接入 CI；3 项 Rust 上游通告按可达性精确登记为有期限
      例外，见 `docs/quality/rustsec-advisory-exceptions.md`。
- [ ] 安装身份、正式签名与公证依赖产品发布策略和外部证书，不在代码中伪造完成状态。
- [ ] Windows 应用内排序与原生文件拖放仍需真机事件模型改造和验收。
- [ ] capability 已移除不需要的通用 core/emit 权限；按窗口拆分自定义 command 仍需先
      冻结窗口角色与 command allowlist。

## 验证

- `npm run typecheck`
- `npm run lint -- --max-warnings=0`
- `npm run format:check`
- `npm run test:tauri`
- `cargo fmt --all -- --check`
- `cargo clippy --locked --all-targets --all-features -- -D warnings`
- 与 FTP/FTPS、transfer lifecycle 相关的专用 fixture

## 本轮验证记录

- Rust library：98/98 通过。
- Tauri contract：14/14 通过。
- Electron unit/controller：84/84 通过。
- Electron FTP/FTPS/SFTP protocol fixture：7/7 通过。
- 全 workspace TypeScript typecheck：通过。
- `npm audit --audit-level=low`：0 vulnerabilities。
- RustSec：0 项未登记漏洞；3 项上游漏洞按可达性精确例外，另有 17 项上游
  unmaintained/unsound 非阻断警告。
- Electron/Tauri production build：通过；Tauri `--no-bundle` release 二进制版本为 `1.2.2`，
  本轮前已验证同版本 DMG。
- Rustfmt 与 strict Clippy：通过，零告警。
