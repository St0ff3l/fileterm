# 测试目录归类

- 独立 Rust 单元测试进入各模块的 `tests/`，保留原模块名、私有访问和平台条件。
- Renderer 与构建验证分别进入 `apps/tauri/tests/renderer/`、`apps/tauri/tests/build/`；浏览器验证保留在 `apps/tauri/tests/browser/`。
- CI 独立夹具和打包 smoke 进入 `tests/ci/`；需要启动或连接本地服务的验证脚本进入 `tests/local/`。OTP 手动服务放在 `tests/local/`，继续使用原 Cargo 入口。
- 同步 npm、CI、文档和相对路径，运行现有测试与格式检查。

## 验收

- Renderer 244 项通过；构建验证 6 项通过，PowerShell 用例按环境跳过。
- 主 Cargo 套件 930 项单测、10 项 CLI、23 项 contract 通过；手动 OTP 服务默认跳过。
- 三个 CI Cargo 夹具均以锁文件运行；当前平台执行 13 项，Windows 专属用例由其平台条件控制。macOS Dock 夹具通过语法检查。
- AI fixture 与 MCP stdio smoke、类型、Lint、Prettier 和 Clippy 通过。CI 打包矩阵及浏览器实测未重跑。
