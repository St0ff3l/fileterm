# Windows 便携版应用内更新

## 目标与范围

便携版复用安装版的检查、下载进度、下载验签与重启更新 UI。最终程序留在原目录并保留原文件名，`config` 和 `portable` 标记不参与替换。macOS/Linux 保持下载页更新。

## 实现

- `latest.json` 新增 `windows-x86_64-portable`，发布流水线为便携 EXE 单独生成 updater 签名；安装版保留原条目。
- Rust update service 根据存储模式选择 payload，继续用 Tauri updater 的下载签名验证；便携版不调用 NSIS install。
- 已验签 payload 和原程序副本写入程序目录下随机事务子目录。辅助进程从专用启动入口执行，绕过 Tauri、CLI 和存储初始化。
- 辅助进程取得原进程句柄后通知就绪，应用才退出；等待结束后检查 staging 和原程序 hash，备份旧 EXE，替换并启动。失败时恢复旧文件；回滚本身失败时保留备份并记录实际路径。
- 重启前复用文件编辑器的未保存确认，取消时保留已下载包；辅助进程就绪后保存传输断点并停止会话 worker，准备失败时取消辅助进程。
- 成功后新程序后台清理事务目录；失败记录在更新页显示，保留诊断与恢复文件。目录不可写或 helper 无法就绪时保持原应用运行。
- 旧客户端仍须手动覆盖一次；新客户端检查旧 Release 缺少便携签名条目时保持下载页 fallback。

## 验证

Rust 测试覆盖原位置/重命名 EXE、数据保留、启动失败回滚、payload/原程序变化拒绝更新、越界目标拒绝。Windows 专用测试覆盖 EXE 被占用与真实进程退出等待。Node 测试覆盖双 payload 清单及缺失签名阻止生成。

本机运行全部仓库质量门禁，Windows 原生替换与签名发布产物按 `docs/quality/local-auto-update-test.md` 执行验收。
