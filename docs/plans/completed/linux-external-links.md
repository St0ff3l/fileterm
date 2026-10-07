# Linux 外部链接打开修复

## 问题

Arch + LXQt 的 Pacman 安装版点击设置中的“打开下载页面”和侧栏更新提示后没有浏览器反馈。两个入口共用 `app_open_external_url`。Pacman 包缺少 `xdg-utils` 依赖，`open::that` 在已安装的启动器返回非零状态后停止，renderer 没有呈现 Promise 的失败结果。

## 实现

- 增加 `services/external_links.rs`，集中校验 HTTP/HTTPS、在 blocking worker 上调用系统启动器，并让 Linux 在启动器退出失败后继续回退。
- 更新页和系统更新服务共用这个入口；失败写入应用日志。
- renderer 更新按钮与侧栏更新提示通过共享 hook 捕获失败，并使用 `FeedbackText` 显示。
- DEB、RPM、Pacman 声明 `xdg-utils`，元数据检查覆盖该依赖。
- 安装文档增加 Arch + LXQt 的启动器和浏览器关联诊断步骤。

## 验证

- 回归测试覆盖非零退出后回退、工具缺失错误、成功后停止，以及真实子进程退出状态。
- 运行类型检查、renderer 测试、Rust 测试、Clippy、lint、格式检查、CSS contract 与 shell 语法检查。
- 当前主机是 Windows；Arch + LXQt 的实际浏览器启动仍需目标机器验证。

## 检查结果

- Tauri 全量测试通过：645 个单元测试、10 个 CLI 测试、21 个 contract 测试。新增真实子进程回退测试后单独复测 external-links 测试组。
- Renderer 测试 41 项通过，Tauri 类型检查通过。
- lint 与格式检查在适配 Windows 检出文件的 CRLF 后通过；CSS contract 与 shell 语法检查通过。CSS 历史债务保持 11 处 `!important` 和约 1 处直接颜色值。
- 临时使用官方 NASM 2.16.03 解决本机构建工具缺失。严格全目标全特性 Clippy 仍被已有 Windows 分支的 unused-imports、byte-char-slices、needless-return 阻断；仅允许这三类已确认的旧告警后检查通过，新代码没有额外告警。
- 现场缺少哪个工具或浏览器关联尚未通过目标机器日志确认；已确认的修复覆盖安装依赖遗漏、启动器非零退出不回退、界面忽略失败结果三个代码缺口。
