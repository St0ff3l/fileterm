# 可恢复文件传输计划（已完成）

## 背景

FileTerm 已有统一传输列表、真实字节进度和取消能力，但任务只存在于内存中，SFTP 与 FTP/FTPS 传输也都会从文件起点重新开始。连接中断或应用退出后，已经传输的字节无法安全复用。

## 目标

- 为 SFTP、FTP、FTPS 单文件上传和下载提供跨连接、跨应用重启的断点续传。
- 传输中的内容写入 FileTerm 专用临时文件，完成校验后再替换正式目标。
- 将传输任务持久化到 Electron `userData`，运行态只通过 `main -> preload -> renderer` 暴露。
- 保持 SSH/SFTP 与 FTP/FTPS 的协议实现分离，共用任务状态、持久化和 UI 体验。
- macOS、Windows、Linux 使用同一任务模型；本地文件最终替换失败时保留临时文件以便重试。

## 已完成范围

- 单文件和目录级 SFTP 上传与下载。
- 单文件和目录级 FTP、显式 FTPS、隐式 FTPS 上传与下载。
- 暂停、继续、丢弃断点。
- 应用重启后恢复任务列表，并将未完成运行态转换为 `paused`，等待用户手动继续。
- 文件大小与源文件身份校验。
- 目录逐文件 manifest，完成文件复核后跳过，当前文件按实际断点长度恢复。
- 主动暂停、协议中断、断线、标签关闭、应用退出和重启恢复统一保留为 `paused`；不自动重试或续传。
- SSH root/shell fallback 字节续传，为任务持久化随机 `/tmp` staging；暂停后续写连续源文件前缀，sudo 提交中断后按受保护断点长度继续提交剩余后缀。
- FTP `APPE` 与 `REST + STOR` 兼容路径。

## 分层

- `packages/core`
  - 定义任务状态、源文件身份、断点路径和公开 API。
- `main/services/transfers`
  - 持久化任务、临时文件命名、本地安全替换与恢复判断。
- `main/services/sessions`
  - SFTP 和 FTP/FTPS 分别实现 offset 读写、远端 stat、临时文件收尾。
- `main/ipc` 与 `preload`
  - 暴露暂停、继续、丢弃操作。
- `renderer/features/transfers`
  - 展示暂停、已中断、校验、收尾状态和对应操作。

## 安全规则

1. 新任务从零写入临时文件；只有同一持久化任务才能复用已有断点。
2. 恢复上传前校验本地源文件大小和修改时间；恢复下载前校验远端大小，并在可用时校验修改时间。
3. 临时文件大于源文件时拒绝续传。
4. 完成后至少校验文件大小，再替换正式目标。
5. 本地或远端最终替换失败时保留临时文件，任务停留在可重试状态。
6. 显式丢弃才删除临时文件；普通断线和暂停不删除。

## 明确不采用

- 不直接采用 meatshell 的无序并行绝对 offset 作为可恢复路径。该实现失败后会删除半成品；如果保留半成品，仅凭文件长度无法判断低位区间是否存在空洞。
- 不把 FTP 的 `resume()` UI 状态当成断点续传。Electerm 的实现只恢复当前内存流的进度通知，连接或进程丢失后没有持久化 offset。
- 不依赖 FTP `HASH/XCRC` 等非标准扩展作为正确性前提。当前稳定基线是源身份、实际断点长度和完成文件大小校验。

## 验证

- 纯逻辑测试覆盖 journal 恢复、目录 manifest、断点过大保护和本地可回滚替换。
- 协议测试夹具覆盖 SFTP 双向 offset、root staging 多次暂停续写、预取消、零字节和 sudo 部分提交恢复、符号链接收尾，以及 FTP APPE、REST/STOR、下载 REST、rename、显式 FTPS 和隐式 FTPS；受限环境缺少 sshd/openssl 或不能监听 localhost 时相应用例会明确标记 skipped。
- 全仓 typecheck、renderer/main production build 和 `git diff --check` 必须通过。
