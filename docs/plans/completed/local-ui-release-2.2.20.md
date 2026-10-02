# 本地 UI 修复纳入 2.2.20

## 目标与范围

将 `chore/dev-app-identity` 上的三个本地提交纳入最新 main，准备 2.2.20 PR。
影响 renderer 监控/首页/设置、主题语义变量、Rust 传输记录解析、开发启动配置与版本元数据。

## 实现

- 新建 `fix/local-ui-2.2.20`，合入最新 main，保留 2.2.19 已发布的修复。
- 监控侧栏冲突按本地布局处理，同时保留 main 的诊断 hook 与操作日志。
- 保留首页标签挂载状态、安全设置跳转清空搜索、监控样式隔离与 FileTerm Dev 数据隔离。
- 旧传输记录的小数毫秒时间戳按整毫秒解析，补充合法值、缺省值、空值与非法值回归测试。
- 仅修改根版本号，再运行 `npm run sync:version`；补充双语发布说明。

## 验证

- Tauri typecheck、lint、Prettier、CSS contract、renderer 生产构建通过。
- Rust Clippy（locked、all-targets、all-features、warnings denied）通过。
- Rust unit/integration/contract 测试 707 项通过；renderer 逻辑测试 28 项通过。
- CSS contract 仍有既有的 11 处 !important 和 1 处直接颜色债务，没有新增。
- 本次 PR 准备不等同于发版；尚未创建 release 分支、tag 或发布产物。
