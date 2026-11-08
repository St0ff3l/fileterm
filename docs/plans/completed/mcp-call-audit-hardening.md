# MCP 调用审计后端修复

关联 PR #380。实现与本机自动验证完成。

## 范围

- 审计只保存白名单元数据，省略命令文本、文件正文、模板实参及原始错误。旧记录读取时同样过滤。
- 日志检查、轮转、读写通过独立锁文件串行化，兼容开发版与安装版共用目录。
- 开始记录先落盘，完成或 future 被丢弃时补记结果，以调用 ID 合并展示。
- bridge 会话 ID 由桌面生成，目标连接从操作开始时的 workspace 解析。
- 记录实际审批决定、调用状态和命令状态。原有 UI 字段保持兼容。
- 读取两份日志，增加稳定游标。UI 后续接入游标，保留旧 offset 参数。

## 验证

- 9 项审计回归覆盖脱敏、并发轮转、取消中断、执行结果、目标解析、旧记录、超限历史文件和稳定游标。
- `npm run typecheck`、`npm run lint`、项目 Prettier 检查通过。
- Renderer 244 项测试通过。
- Rust 单元 940 项、CLI 10 项、contract 23 项通过；既有交互式 SSH OTP 手工夹具保持 ignored。
- `cargo clippy --manifest-path apps/tauri/src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings` 通过。
- UI 与真实 SSH 主机手工验收由后续任务完成。已有日志文件可能含原始敏感内容，读取过滤不会追溯改写文件。

## 模块职责与规模

原 `call_log.rs` 为 202 行，改为 `call_log/mod.rs`（355 行，生命周期与元数据）、`store.rs`（281 行，文件锁、轮转与读取）及 `tests.rs`（280 行，回归）。修改的 core 类型聚合与原有测试聚合超过 1000 行，属于项目规模规则豁免；本次未拆分这两个聚合文件。
