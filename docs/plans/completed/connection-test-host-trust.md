# 新建连接测试的主机指纹保存修复

日期：2026-09-29

状态：代码修复与自动化验证完成。

## 原因

新建表单测试尚无 profile ID。SSH worker 的 accept-and-save 只尝试更新已存在的 profile，找不到记录时仍返回成功；renderer 的测试接口返回 void，表单没有接收已接受的指纹，随后 createProfile 保存了空指纹。正式连接因此再次提示。

## 实施

- 复用现有 SSH 交互请求中的指纹，仅在 resolveSshInteraction 成功确认 accept-and-save 后传回表单。
- 校验测试 tab、profile ID、主机和端口，排除 jump-host；目标主机的指纹随 createProfile/updateProfile 保存。
- 表单修改主机、端口、协议或跳板机时清除携带的旧指纹；取消新建不创建任何 profile。
- accept-once、取消、过期和失败的交互不写入表单。

## 验证

- 新增 `apps/tauri/tests/connection-host-trust.test.mjs`：执行生产交互 hook、表单 hook 和保存 handler，mock IPC 与 React 状态边界。12 项回归覆盖新建保存 payload、已保存连接换 key、仅接受一次、取消、过期/失败、重复点击、目标改变、IPv6、跳板机和其他会话隔离。
- `npm run test:renderer -w @fileterm/tauri`：21 项通过（含 12 项新增回归）。
- `npm run typecheck -w @fileterm/tauri`、`npm run lint`、`npx prettier --check apps/tauri packages/core packages/shared packages/storage`：通过。
- `npm run test:tauri`：675 项通过（644 unit、10 protocol fixture、21 contract）。
- `cargo clippy --manifest-path apps/tauri/src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings`：通过。
- 本次仅修改 renderer 行为 hooks，不涉及组件、CSS、IPC 类型或 Rust 运行逻辑；源文件均未达到 1000 行拆分阈值，修改后的业务源文件最大 788 行。
