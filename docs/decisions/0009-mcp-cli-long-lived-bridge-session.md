# ADR-0009: MCP/CLI 使用进程级长连接 bridge 并按请求多路复用

## 状态

Accepted（2026-09-02）

## 背景

FileTerm 的公共 MCP stdio 和 CLI JSONL 进程本身可以持续收发多条消息，但旧的内部 desktop bridge 把每个 action 映射为一次新的 loopback TCP 连接。Agent 连续执行连接、部署、文件和状态查询时，会在桌面端看到反复的“连接成功 / 连接关闭”，并且并发响应没有稳定的内部路由边界。

远程 SSH worker 的职责不同：连接和 session worker 应保持长驻，普通命令可以各自使用独立 exec channel，长部署使用有稳定 command ID 的后台 channel。不能为了复用本地 bridge 而把所有远程命令强行塞进一个 SSH exec channel。

## 决策

### 1. 公共协议和内部 bridge 分层

公共接口继续保持 MCP JSON-RPC over stdio、CLI JSONL over stdio 和一次性 CLI 三种语义。只有 `fileterm mcp` 与 `fileterm cli --jsonl` 是外部 Agent 的持久入口；一次性 CLI 仍是手动调试和 shell 脚本入口。

每个持久进程创建一个 `Arc<BridgeClient>`。客户端最多持有一条已认证的 loopback TCP session，使用内部 `BridgeFrame` 表达 hello、request、progress、response、cancel、ping/pong 和 close。公共 MCP/CLI 响应格式不携带内部 bridge request ID。

### 2. 一个 session 复用多个请求

客户端发送唯一的 `request_id`，writer 线程串行写帧，reader 线程持续读取帧，并将 response/progress 按 ID 投递给对应等待者。多个 worker 可以并发调用，响应允许乱序但不能串路。

桌面端只在 session 开始执行一次 token/version/client ID 校验，然后进入持续 session loop。每条 request 独立 dispatch；每个 session 维护自己的 cancellation map；所有响应经单一 writer 输出，避免并发 JSON 行交错。`ping/pong` 只用于检测本地 session 是否仍活跃，`close` 和 EOF 都触发有界清理。

### 3. 断线不重放，恢复单飞且有界

连接失效后，只有 `BridgeClient` 的一个 recovery 流程可以执行重连；其他 worker 复用同一个连接状态和恢复结果。恢复使用短退避并设置 circuit breaker，避免多个请求各自重连或在桌面端不可用时无限快速循环。

已经写入 bridge 的在途请求不自动重放。尤其是部署、启动服务、迁移数据库、上传和删除等有副作用 action，响应丢失时只能返回 `FILETERM_MCP_BRIDGE_DISCONNECTED`，由 Agent 根据 action 语义决定是否查询状态或在用户确认后重试。长任务通过 `commandId`、增量 offset 和原 SSH channel 查询，不通过重新提交命令恢复。

### 4. 背压、取消和进度

客户端 writer、桌面端 writer 和 progress queue 均有界。非终态 progress 在满载时可以丢弃；response、cancel 和 session 清理不能因为 progress 洪峰而无限积压。请求取消只取消对应 ID，并向桌面端发送同 ID 的 `cancel`；它是 best-effort 清理，不承诺回滚已经接受的远程副作用。

### 5. SSH 层保持独立生命周期

本地 bridge 长连接只负责 CLI/MCP 进程到桌面 runtime 的消息传输。桌面端仍持有 SSH/SFTP session worker；普通远程命令使用独立 exec channel，后台长命令由后台 command registry 持有 channel 和有界输出。SSH channel 重试只允许发生在命令尚未被服务端接受的 channel-open 阶段，不能重放已经接受的 exec。

## 影响

### 正面影响

- 连续执行几十个 CLI/MCP action 时，持久进程和本地 desktop bridge 不会每次 action 都建立和关闭 TCP 连接。
- 并发请求、乱序 response、progress、取消和断线都具有明确的 request ID 边界。
- 单飞恢复和短暂熔断抑制重连风暴；不重放保护部署、迁移等副作用 action 的 exactly-once 边界。
- 本地 bridge 的长连接改造不改变 SSH exec channel 的职责，也不把交互式终端伪装成后台执行。

### 限制与非目标

- 如果外部 Agent 每个 action 都重新启动 `fileterm mcp` 或一次性 CLI，进程级复用无法跨进程生效；客户端接入必须复用一个持久 stdio 进程。
- loopback TCP 仍会在 FileTerm 重启、CLI/MCP 进程退出或真实连接异常时关闭；长连接不是无限期保活保证。
- 断线后的在途请求不自动重放，调用方需要查询后台 command ID 或在用户确认后重新发起安全可判定的 action。

## 实现位置

- `apps/tauri/src-tauri/src/services/mcp/bridge/client.rs`：进程级客户端、writer/reader、pending 路由、心跳、取消、恢复和熔断。
- `apps/tauri/src-tauri/src/services/mcp/bridge/wire.rs`：内部 bridge frame。
- `apps/tauri/src-tauri/src/services/mcp/runtime.rs`：桌面端 hello/session loop、请求 task、取消 map 和统一 writer。
- `apps/tauri/src-tauri/src/services/mcp/cli_runtime.rs`：MCP stdio/CLI JSONL worker pool 共享 `Arc<BridgeClient>`。
- `apps/tauri/src-tauri/src/services/mcp/background.rs`：独立 SSH 后台命令生命周期和 command ID registry。
