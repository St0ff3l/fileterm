# 日志模块收敛与 tracing 接入

## 目标

将诊断日志从单文件实现收敛为稳定模块，并接入 Rust tracing 生态：统一级别和分类，拆分脱敏、格式化、文件队列、轮转和 panic 捕获；将 tracing event/span 和旧业务调用汇入统一 subscriber；保留业务调用接口。

## 影响层级

- Rust services/logging：稳定 facade 与职责模块。
- Rust renderer 诊断 command：共享日志级别校验。
- Rust 应用生命周期：正常退出时有界排空日志队列。
- Renderer lib：集中 IPC 写入和 scope 常量。
- Renderer 监控诊断：Related to #259，保留此前新增的限频日志。
- Rust tracing subscriber/layer：统一承载结构化事件、span 上下文、EnvFilter 与 app.log writer。

## 实现边界

- 仍使用本地 app.log 与 app.log.1，不改变安装版和开发版的数据隔离。
- category 为统一枚举；原 scope 与标签 ID 保留，未知 scope 归入 other。
- 默认级别保持 DEBUG，tracing-subscriber EnvFilter 读取 FILETERM_LOG_LEVEL；支持标准 level 与 target directives。
- 新 Rust 调用使用 tracing 事件和具名字段；兼容 facade 将旧自由文本日志转换成 category/scope/message event。
- tracing-log 将依赖通过 `log` facade 发出的事件接入 FileTerm subscriber。
- 常规日志进入 1024 条有界队列，由专用线程写入；队列满时丢弃 TRACE/DEBUG/INFO 并累计报告，WARN/ERROR 沿用后台写入路径。
- 正常退出最多等待 500ms；不承诺强制终止后的日志持久化。
- 终端会话 transcript 和 renderer console 不纳入诊断 logger，避免混入远端输出。
- 保留现有脱敏测试源码；不新增或运行测试。

## 检查

- Rust cargo check / fmt / clippy。
- Renderer typecheck / ESLint / Prettier。
- CSS contract 与 git diff --check。

## 完成结果

- 原 logging.rs 287 行拆为目录模块，并接入 tracing subscriber：当前 `mod.rs` 196 行、`layer.rs` 148 行、`category.rs` 81 行、`format.rs` 113 行、`level.rs` 47 行、`writer.rs` 150 行、`panic.rs` 21 行，模块合计 756 行。
- 原文件不属于规模豁免类别，也未达到 1000 行强制拆分阈值；按日志职责主动拆分，当前单文件最大 196 行。
- `cargo fmt --check`、`cargo check --locked` 与 `git diff --check` 通过。最终 Clippy 重跑因系统磁盘仅剩约 120 MiB，在生成依赖文件时报告 `No space left on device`；未能完成。此前 renderer typecheck、ESLint、Prettier 和 CSS contract 已在同一工作树的先前 UI/logging 改动中通过。
- 前一阶段的旧日志路径已在开发版 app.log 观察到 category 字段；本次 tracing subscriber 通过编译检查，尚未在运行中的应用里现场验证输出。
- `tracing` 事件经 EnvFilter 与自定义 layer 格式化，具名字段和 span 名称会进入原有 app.log 行格式；现有调用点仍可逐步迁移，不要求一次性机械改写所有旧消息。
- 未运行测试；未进行队列过载或退出排空的动态验证。保留原有脱敏测试源码。
- CSS contract 的 11 个 !important 和约 1 处旧色值属于原有债务，本次未新增。
