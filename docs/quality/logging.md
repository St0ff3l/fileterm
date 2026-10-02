# 本地诊断日志

## 代码入口

Rust 统一入口为 `apps/tauri/src-tauri/src/services/logging/mod.rs`。持久化管线使用 `tracing` subscriber 和自定义 layer，旧的 `logging::debug/info/warn/error/session/write_global` 等 API 作为兼容 facade，把原有调用转成 tracing event。

| 模块        | 职责                                                |
| ----------- | --------------------------------------------------- |
| mod.rs      | 稳定 facade、日志目录、subscriber 注册与旧调用适配  |
| layer.rs    | tracing 字段和 span 提取、级别转换、写入统一 writer |
| level.rs    | TRACE / DEBUG / INFO / WARN / ERROR 类型与解析      |
| category.rs | scope 到统一功能分类的映射                          |
| format.rs   | 秘密脱敏、控制字符转义、单行和长度限制、行格式化    |
| writer.rs   | 有界队列、后台线程、文件写入、轮转、退出排空        |
| panic.rs    | panic 位置和消息捕获，同时保留原 stderr 行为        |

前端入口为 `apps/tauri/src/renderer/lib/diagnostic-log.ts`，统一 scope 常量和写入失败处理。
调用链为 renderer → writeDiagnosticLog IPC → Rust tracing event → subscriber layer → writer，组件不直接写文件。`tracing-log` 把依赖通过 `log` facade 发出的记录接入同一订阅器。

## 分类与格式

```text
毫秒时间戳 [INFO] [renderer:monitoring] category=monitoring sidebar action action="toggle" tab_id="…"
```

- level 表示严重程度。
- category 表示统一功能域：app、workspace、connection、terminal、monitoring、files、transfer、storage、security、updates、integration、logging、other。
- scope 保留原功能名和标签 ID，例如 ssh:tab-id、metrics:tab-id、renderer:workspace。
- 未识别的 scope 归入 other，不丢弃日志；增加业务日志时同步扩充 category.rs 映射。
- 新 Rust 代码优先使用 `tracing::{debug, info, warn, error}` 并记录具名字段；需要贯穿函数调用的上下文时使用 span / `#[instrument]`。不要把凭据、终端输入或完整远端输出作为字段传入。
- tracing event 的额外字段会以 `key=value` 附加到消息后；活动 span 名称会以 `span="…"` 附加。旧 facade 调用生成相同的 tracing event，但暂时保留原来的自由文本消息。

```rust
tracing::info!(
    category = "connection",
    scope = "ssh",
    tab_id = %tab_id,
    protocol = "ssh",
    "session connected"
);
```

分类用于检索，所有功能的事件仍写入同一个 app.log，方便按时间和 tab_id 串联跨层问题。
历史行没有 category 字段，新旧行仍兼容原来的时间戳/级别/scope 前缀。

## 文件与保留

日志位于应用数据目录的 logs/，设置页“系统与日志”可打开。
开发版与安装版分别使用 com.fileterm.desktop.dev 和 com.fileterm.desktop 的数据目录；Windows portable 沿用 exe 旁的配置目录规则。

app.log 在下一条日志导致超过 2 MiB 前轮转为 app.log.1，仅保留一个备份。单条消息上限为 16 KiB，控制字符转义，不允许换行注入。

密码、token、Authorization、私钥、OTP 等带标签字段会脱敏。业务代码仍应避免将秘密传给 logger，脱敏不能识别所有没有标签的秘密。

## 级别过滤

使用 tracing-subscriber `EnvFilter`，默认保留 DEBUG 及以上，延续原来的诊断行为。启动前可设置 `FILETERM_LOG_LEVEL` 为 TRACE、DEBUG、INFO、WARN、ERROR，或使用 tracing-subscriber 的 target 过滤指令；大小写不敏感，无法解析时回退到 DEBUG。
该设置在进程启动时读取，修改后需重启。过滤在事件格式化、脱敏和排队前完成，不影响原 renderer console 输出。

## 写入和限流

- 常规日志通过非阻塞提交进入 1024 条有界队列，由一个专用线程串行写入；避免每行都创建 Tokio blocking task。
- 队列满时 TRACE/DEBUG/INFO 被丢弃，并在后续写入时记录 dropped_low_priority 计数（受 WARN 级别过滤约束）。
- WARN/ERROR 在队列满时继续走后台写入回退路径，不因队列满而主动丢弃；该回退不受普通队列容量限制。
- 排队顺序在普通写入线程中保持；回退路径可能与普通日志交错，应以时间戳和 scope 关联。
- 正常桌面退出最多等待 500ms 排空已排队的日志；磁盘错误、后台任务异常、线程启动失败、强制终止或超时仍可能导致日志缺失。

业务采样日志应单独限频，不依赖队列满后的保护。监控样本摘要最多每分钟一条，侧栏状态变化去重，详见 monitoring-sidebar-diagnostics.md。

## 前端异常

`renderer-error-log.ts` 通过同一 IPC 将异常写入 `app.log`，级别为 ERROR、scope 为 `renderer:error`（category 为 workspace）。覆盖 React ErrorBoundary、window error、未处理的 Promise 拒绝和 bridge 就绪后的初始化失败。

记录 source、应用版本、平台、窗口类型、错误名称/消息、JavaScript stack 和 React componentStack。只提取窗口类型，不记录完整 URL 或任意拒绝对象；不自动收集连接配置、终端内容或业务状态。堆栈在生产构建中可能包含压缩符号，但仍保留定位信息。三个错误正文/堆栈字段分别按转义后 UTF-8 3000 字节限制，同来源同内容在 5 秒内去重，React 组件堆栈不会被全局错误去重覆盖。

日志提交是 best-effort：bridge 尚未就绪、写入失败、进程强制退出时可能无法落盘。处理器不更新 React state，不阻止浏览器默认错误处理，也不把所有 console 输出转存为文件日志。

## 范围

诊断 app.log、终端会话 transcript 和 WebView console 是三个不同入口。诊断日志不自动保存终端输出，console 也不会自动归入文件日志。
