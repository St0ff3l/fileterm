## FileTerm 2.1.7

FileTerm 2.1.7 聚焦 SSH 隧道转发、`su -` 文件传输稳定性、文件管理器权限切换，以及 Tauri 子窗口和工作区界面的细节优化。

### 2.1.7 更新重点

- SSH 远程端口转发：修复固定监听端口的 `-R` 转发在 SSH 服务端成功但未返回端口值时被记录为 `0` 的问题，转发匹配和停止操作现在始终使用实际有效端口；补充 OpenSSH 回归测试。
- `su -` 文件下载：等待迟到的 SSH 退出码；文件字节数完整但退出码缺失时允许成功；非零退出码或文件不完整仍然失败，避免把成功文件误报为失败或留下不完整文件。
- 文件管理器权限切换：支持在 `sudo` 与 `su` 之间选择，目标用户默认 `root` 且可手动输入；终端权限状态与文件区同步，root 文件上传使用分块 staging，降低大文件传输的内存占用。
- Tauri 窗口与工作区：子窗口相对主窗口居中，优化文件编辑器、传输面板、搜索栏、主题颜色和紧凑布局表现。
- 质量门禁：继续执行 Tauri/Rust 格式、类型、协议夹具、跨平台 socket 生命周期、生产构建和依赖安全检查。

### 本版本包含的主要 PR 和问题修复

- [PR #173](https://github.com/St0ff3l/fileterm/pull/173)：修复固定端口 SSH 远程转发，并补充真实 OpenSSH 回归覆盖。
- [PR #174](https://github.com/St0ff3l/fileterm/pull/174)：合入文件管理器权限、`su -` 下载和 Tauri 窗口定位等 2.1.7 发布前修复。
- [Issue #167](https://github.com/St0ff3l/fileterm/issues/167)：跟进普通用户 `su -` 场景下的文件管理器权限与传输问题。

> 遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

> 也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。
