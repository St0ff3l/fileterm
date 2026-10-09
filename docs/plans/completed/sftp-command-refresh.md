# SFTP 命令后静默刷新

关联：Refs #239。日期：2026-10-09。

- Rust SSH shell integration 为 Bash 4.4+ 的 PS0、Zsh 的 preexec 添加命令开始标记，保留已有用户 hook；提示符标记配对完成后才刷新。空回车不产生命令标记。
- CWD、权限模式或访问方式变化仍正常加载目录；同目录命令结束后静默解析并读取终端 CWD 对应目录；文件面板手动浏览到其他位置后，下次命令完成会重新对齐。仅路径或列表变化时发布 snapshot。保持单飞与 200ms 合并。
- 不支持可靠命令标记的旧 Bash/BusyBox/POSIX shell 回退到提示符后的静默比较，不按输入或命令名称猜测；不增加 UI 轮询。
- 保留 SFTP 命名空间映射、启动重放、跟随开关、过期结果与权限校验。静默读取不修改 loading；路径与列表一起更新，使用确认有效的 SFTP 映射路径，并丢弃读取期间手动导航后的过期结果。
- 验证标记跨包、空回车、命令完成、列表差异、已有 hook 与真实本机交互 shell；运行项目门禁。真实远端桌面验收单独记录。

## 验证结果

- Tauri 全量 748 项通过：715 单元测试、10 与 23 项集成/contract 测试；零失败。
- Tauri typecheck、全仓源码 lint、指定范围 Prettier、Rust fmt 和 Clippy 全部通过。
- 回归覆盖命令标记的所有分包边界及 BEL/ST 终止符、同目录空提示符、命令完成后静默刷新、CWD/身份分包配对、旧 shell 回退、相同列表不更新及新增/删除/权限变化。
- 本机真实交互 Zsh 验证原 preexec hook 保留、重复注入不重复注册、空行不产生命令标记。本机 Bash 3.2 验证旧版本回退、用户 PS0/PROMPT_COMMAND 保留与关闭 history 后执行；后续在用户指定主机的 Bash 5.3 验证了命令/空回车标记分支，详见 [sudo/su CWD 跟随](./root-shell-cwd-follow.md)。
- 未进行真实远端 SFTP、Windows/Linux 桌面 UI 实机验收。后台运行的文件修改若在提示符已返回后才完成，需要再次执行命令或手动刷新；此实现不持续监视远程文件系统。
- Refs #239；未关闭 Issue，未提交或发布。

## Shell 接口依据

- [Bash Interactive Shell Behavior](https://www.gnu.org/software/bash/manual/html_node/Interactive-Shell-Behavior.html)：PS0 在读取命令后、执行前输出。
- [Zsh Functions](https://zsh.sourceforge.io/Doc/Release/Functions.html)：preexec 在命令执行前调用；通过 add-zsh-hook 注册而不替换用户 hook。

## 手动浏览后恢复跟随

- 开启跟随终端时，执行命令后重新对齐 CWD，即使 `cd .` 或 `cd` 返回相同 CWD；空回车仅在面板偏离已确认的 CWD 映射时恢复对齐；对齐后跳过。关闭跟随时保留手动浏览位置。
- 静默读取同样经过 SFTP 命名空间解析，避免把 Shell 物理路径直接写入 chroot 文件面板。路径改变但两边列表都为空时也必须发布。
- 回归覆盖同 CWD 命令与空回车、手动上一级后重新对齐、空列表路径变化和已解析命名空间。
- 本次回归门禁：745 项 Rust 测试、Clippy、Rust fmt、Tauri typecheck、Lint、Prettier 和 diff 检查通过。桌面交互尚未实机验收。

- 后续截图回归：缓存最近成功跟随的 Shell/SFTP 路径及身份，提示符返回时只比较缓存与面板路径；手动离开后空回车可恢复跟随，对齐后不读目录。该缓存不猜测 chroot 路径，不进入 UI 轮询。

- 用户最终确认：每次回车、提示符返回后比较已确认的 CWD 映射和面板路径；不同才同步，相同不重新加载，不使用空格键触发。实际命令结束后的文件变更仍静默比较。
- 最终门禁：748 项 Rust 测试、Clippy、Tauri typecheck、Lint、Prettier、Rust fmt 与 diff 检查全部通过；原生桌面 su 场景尚未验收。

- 恢复 loading 规则：命令或空回车后的提示符若发现面板偏离已确认 CWD，使用正常目录加载与遮罩；同目录文件变化检查保持静默。
