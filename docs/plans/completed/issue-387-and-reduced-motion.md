# 远端备份重建与减少动态效果

- Refs #387：明确区分远端 404 和现存对象缺少/变更 ETag。仅远端不存在时绕过历史版本校验，仍用条件 PUT 防止并发覆盖；成功后再持久化新状态。
- WebDAV/S3 共用版本校验规则，覆盖旧 ETag 重建、现存对象版本冲突与并发创建的回归检查。
- 跟随系统 prefers-reduced-motion：公共弹窗、下拉菜单改为淡入，禁用下拉箭头、复选框与首页侧栏的移动/缩放过渡，补齐内嵌连接管理列表。
- 验证：项目类型、lint、格式、renderer/Rust 测试、Clippy 和 CSS contract。系统 WebView 与三平台实测单独报告。

## 完成结果

- 类型、lint、全量 Prettier、cargo fmt、Clippy 与 CSS contract 通过。CSS contract 原有的 12 处 !important 和约 6 处直接颜色警告未增加。
- Renderer 258 项通过；Rust 单元测试 983 项通过、1 项忽略，集成测试 34 项通过。
- 本地 HTTP 夹具验证旧 ETag 下重建成功、If-None-Match 防止并发创建覆盖；共用版本校验覆盖现存对象缺失 ETag 和版本变化。
- 未做真实 WebDAV/S3 服务及三平台系统 WebView 动画实测；Issue 保持打开。
