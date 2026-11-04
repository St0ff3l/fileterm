# 测试目录

| 用途                    | 目录                                                                       | 运行方式                                                 |
| ----------------------- | -------------------------------------------------------------------------- | -------------------------------------------------------- |
| Renderer 逻辑           | [apps/tauri/tests/renderer](../apps/tauri/tests/renderer/)                 | `npm run test:renderer -w @fileterm/tauri`               |
| 构建、打包与检查脚本    | [apps/tauri/tests/build](../apps/tauri/tests/build/)                       | `npm run test:build -w @fileterm/tauri`                  |
| 浏览器与布局验证        | [apps/tauri/tests/browser](../apps/tauri/tests/browser/)                   | 按相关质量文档单独运行                                   |
| Rust 单元测试           | [apps/tauri/src-tauri/src](../apps/tauri/src-tauri/src/) 各模块的 `tests/` | `npm run test:tauri`                                     |
| Rust 集成测试           | [apps/tauri/src-tauri/tests](../apps/tauri/src-tauri/tests/)               | 由 Cargo 自动发现，与单元测试一起运行                    |
| CI 原生夹具与打包 smoke | [tests/ci](ci/)                                                            | 由 [CI workflow](../.github/workflows/ci.yml) 按平台调用 |
| 本地服务、桌面联调      | [tests/local](local/)                                                      | 通过根目录 `qa:*` 命令或对应验收文档运行                 |

`npm run test:local` 依次执行代码检查、Renderer 测试、构建测试和 Rust 测试。Renderer 和浏览器共享的数据保存在 [apps/tauri/tests/fixtures](../apps/tauri/tests/fixtures/)。

需要私有实现或平台条件的 Rust 单元测试统一放在该模块自己的 `tests/` 目录内，通过 `mod tests;` 或 `#[path]` 挂载，保持原模块名、私有访问和平台条件。SSH 自动 OTP 测试使用短生命周期的 loopback 夹具。手动 OTP 服务单独存放在 [tests/local/otp-manual-fixture.rs](local/otp-manual-fixture.rs)，默认跳过，启动和停止方法见 [验收说明](../docs/quality/ssh-otp-autofill.md)。

技能自带的测试随对应技能维护。
