# 传输期间系统防睡回归

防睡由 Rust transfer run 拥有，适用于 SFTP 与 FTP/FTPS 的单文件和目录上传/下载，包括 root 文件传输、目录扫描、校验与最终替换。SSH 心跳与断点续传继续使用原有边界。

## 自动验证

- `cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml --locked services::transfers::power::tests --lib`
- 验证并行租约最后释放、下一批重新申请、创建/销毁线程相同、异常展开释放、申请期间取消、初始化超时后的释放，以及后端不可用后的恢复。
- Windows 附加原生 API 验证，确认 `ES_SYSTEM_REQUIRED` 实际生效、没有 `ES_DISPLAY_REQUIRED`、Drop 后请求消失。

## 桌面手测

从目标工作树启动 FileTerm Dev，核对进程路径和开发身份；使用测试服务器与足够大的文件。

1. 空闲状态：Windows 执行 `powercfg /requests`（该诊断可能需要管理员终端），macOS 执行 `pmset -g assertions`，Linux 执行 `systemd-inhibit --list`。FileTerm 不应持有传输防睡请求。
2. 启动一个上传和一个下载。应仅有一个共享防睡请求；目录扫描、传输、校验和替换期间持续持有。
3. 暂停其中一个任务，另一个继续时请求仍在；最后一个暂停、完成、失败或取消后请求消失。继续任务会重新申请。
4. 将系统自动睡眠时间设为较短值，确认有活动传输时系统保持运行，屏幕仍可按原设置关闭；最后一个任务结束后恢复原来的空闲睡眠行为。验收完成后恢复原电源设置。
5. 断网、断开标签与退出应用，确认没有遗留请求，断点任务仍可重连后手动继续。
6. 在缺少 logind/system bus 或权限拒绝的 Linux 环境确认传输仍运行，`transfer:power` 有失败日志。

边界：操作系统电源策略可能限制防睡（例如 Windows Modern Standby 的电池模式）。此能力只申请防止空闲自动睡眠；不保证显式睡眠、合盖、强制休眠或关机期间继续传输。Linux 的效果取决于桌面电源管理器是否遵循 logind idle inhibitor。初始化最多等待 3 秒，失败或超时后继续传输；超时期间任务结束后，即使原生申请随后完成，也会立即释放。申请与释放日志位于 `transfer:power`。
