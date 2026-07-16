# 0003：可恢复传输使用分层 staging

## 状态

Accepted（2026-07-03，2026-07-16 修订 root 提交流程）

## 决策

1. 普通 SFTP、FTP、显式 FTPS、隐式 FTPS 上传写入目标同目录的 `.fileterm-part`，完成大小校验后再 rename 到正式文件。
2. SSH root 文件模式先顺序写入任务独占的 `/tmp/fileterm-root-upload-*.part`，因为登录用户通常无权在目标目录创建文件。完整大小校验通过后，由 sudo 把 staging 移到目标同目录的 `.fileterm-part`，再次校验后再执行最终替换。这样 `/tmp` 与目标目录跨文件系统时，正式目标也不会参与非原子的跨设备移动。
3. SFTP 替换目标前检查符号链接和 uid。符号链接或属主不同的目标采用原 inode 写回；普通文件尽量继承原权限后再 rename。
4. FTP 上传优先 `APPE`；不支持时尝试 `REST + STOR`。如果结果无法通过大小校验，删除不可信断点并从零上传。
5. 可恢复 SFTP 不采用无序并行写。只有在未来引入持久化范围位图并能证明连续区间后，才考虑把并行绝对 offset 加入恢复路径。

## 依据

- WinSCP 的 SFTP 自动恢复采用 `.filepart + offset + rename`，并明确避开符号链接和其他属主文件；临时新文件也可能改变原文件属主与属性。
- meatshell 的并行绝对 offset 能提升高 RTT 链路吞吐，但失败或取消会删除半成品，因此不能直接用“文件长度”恢复。
- Electerm 的 `pause()/resume()` 只暂停进度上报，没有跨连接或跨进程断点状态。
- `basic-ftp` 6.x 原生支持下载 `REST` offset、上传 `APPE` 和隐式 TLS；FileTerm 在 controller 层组合这些能力，不向 renderer 暴露协议命令。

## 影响

- 正式目标在传输完成前保持可用。
- root 上传保留原有权限兼容路径，同时具备跨连接恢复能力。
- root journal 分别持久化 `/tmp` staging 与目标目录 partial；旧版仅记录 `/tmp` partial 的任务在读取 journal 时自动迁移。
- 普通文件 rename 仍可能改变 inode；对符号链接和可检测的其他属主目标使用保守写回。
- 目录传输 journal 会随文件数增长，但只在状态边界持久化，不按每个数据块写盘。
