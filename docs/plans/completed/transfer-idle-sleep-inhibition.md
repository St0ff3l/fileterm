# 传输期间系统空闲防睡

## 目标与依据

SSH transport 已有可配置心跳，但心跳不能阻止操作系统自动睡眠。参考 Electerm 关于系统睡眠导致网络断连的说明，在 Rust transfer 生命周期内补充系统空闲防睡。保留现有协议心跳、请求超时和传输数据阶段的一小时上限。

## 实现边界

- `services/transfers/power.rs` 管理共享租约，覆盖 SFTP、FTP/FTPS 的单文件和目录上传/下载、扫描、校验与提交。
- `keepawake 0.6.1` 在专用线程申请并释放原生请求，满足 Windows `SetThreadExecutionState` 的线程归属。多个任务共享请求，最后一个活动任务结束后释放；暂停、失败、取消和异常展开均由 RAII 收口。
- 只抑制空闲自动睡眠，允许屏幕关闭。申请失败或初始化超过 3 秒后继续传输，记录 `transfer:power` 日志。
- 系统能力留在 Rust service，不新增 renderer、bridge 或 core 状态。依赖清单、MIT 声明和打包许可同步更新。
- 同一分支保留 Windows 主窗口恢复大小后、首次显示前居中的修复。

## 验证与限制

- Tauri Rust 全套测试和 renderer 类型检查通过；最终代码补跑 652 项 Rust 单元测试全部通过。防睡测试覆盖共享计数、原生线程归属、异常、取消、初始化超时和后端不可用。
- 新增 6 项防睡测试全部通过；Windows 原生 API 测试检查实际系统防睡标志、允许屏幕关闭及释放结果。`cargo fmt --check` 通过。
- `npm run build -w @fileterm/tauri -- --no-bundle` 通过，包括 renderer、离线资源检查和 Windows release 可执行文件编译；未安装或启动该产物。
- 默认 lint/Prettier 检查受现有 Windows CRLF checkout 影响；禁用 ESLint 的 Prettier 规则和使用 Prettier `--end-of-line auto` 后通过。
- 严格 Clippy 存在未改动代码的 unused-imports、byte-char-slices 和 needless-return 问题，不能声称严格门禁通过；仅允许这三类现有问题后，所有 targets/features 检查通过。
- 未进行实际自动睡眠、合盖或跨平台桌面手测；操作系统策略及 Linux logind 支持边界见 [回归清单](../../quality/transfer-power-regression.md)。
