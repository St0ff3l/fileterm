# ADR-0006: SSH Shell 与 SFTP 使用不同路径命名空间

## 状态

Accepted（2026-08-28）

## 背景

FileTerm 的 SSH 会话同时打开两个独立通道：

- SSH shell：执行 `cd`、`pwd`、`ls` 等命令，看到的是操作系统或服务端 shell 的物理路径。
- SFTP：通过 SFTP subsystem 读取目录和执行文件操作，看到的是 SFTP 服务端暴露的路径。

这两个通道虽然使用同一套 SSH 认证，但不保证使用同一个文件系统根。服务端可以为 SFTP 配置独立的服务根或 chroot。OpenSSH 文档明确说明，`ChrootDirectory` 会在认证后改变会话的文件系统根；SFTP 客户端看到的 `/` 因此不一定是 shell 看到的系统 `/`。

群晖场景会把这个差异表现得很明显：

- Shell 的 `ls /` 可以看到 `usr`、`var`、`volumeN` 等系统目录。
- SFTP 的 `/` 可以直接看到 `docker`、`photo`、`music` 等共享文件夹。
- Shell 的 `/volumeN/photo` 在以存储卷为 SFTP 根时对应 SFTP 的 `/photo`，而不是 SFTP 的 `/volumeN/photo`。

因此，`shellCwd` 和 `remotePath` 不能共用同一个字符串语义。

## 群晖路径和用户权限

群晖官方文档中的 `/volume1` 是示例，不是固定规则：

- 启用 User Home 后会创建每个用户的私有 `home` 目录和包含所有用户目录的 `homes` 共享文件夹；多卷设备可以选择 `homes` 所在的卷。[群晖 User Home 文档](https://kb.synology.com/en-us/DSM/tutorial/user_enable_home_service)
- 群晖官方示例将本地用户 Home 写成 `/volume1/homes/Account_name`，同时明确说明实际存储卷可能不是 Volume1。[群晖 Synology Drive 路径说明](https://kb.synology.com/en-nz/DSM/tutorial/Drive_difference_between_homes_My_Drive_home_folders)
- `home` 是当前用户的私有 Home，`homes` 保存所有用户的 Home；能否列出或进入其他目录由 DSM 共享文件夹权限和用户是否为管理员决定。[群晖共享文件夹说明](https://kb.synology.com/en-global/DSM/help/DSM/AdminCenter/file_share_desc?version=6)

所以不能假设：

1. 存储卷一定叫 `volume1`；
2. 普通用户一定能看到 `homes`；
3. SFTP `/` 一定等于 shell `/`；
4. 登录成功后两个通道一定落在同一个 Home。

用户使用密钥登录还是密码登录，不会改变上述路径命名空间规则；认证方式和 SFTP 根目录是两件事。

## 证据：截图中的两个“根”

用户反馈的截图实际上说明了两个层次：

- FileTerm 的远程 SFTP 面板位于 SFTP `/`，其下列出 `docker`、`Download`、`home`、`homes`、`music`、`photo` 等共享文件夹。
- 终端位于 shell `/`，`ls -la /` 列出 `bin`、`etc`、`usr`、`var`、`volume1` 等系统目录。
- File Station 的第一张图已经进入 `docker`，而 FileTerm 远程面板仍在 `/` 并把 `docker` 显示为子目录；两者的共享文件夹名称一致，但当前目录并不相同。

这也解释了日志中的失败：

```text
Shell CWD reported: /volume1/homes/Stoffel
CWD follow failed for /volume1/homes/Stoffel: No such file
```

对 SFTP 来说，正确候选通常是 `/homes/Stoffel`，而不是 `/volume1/homes/Stoffel`。

## 决策

### 1. 状态字段分工

- `shellCwd`：保留 Shell 上报的物理路径，用于终端状态和诊断。
- `remotePath`：只保存 SFTP 命名空间中的路径，用于 SFTP 浏览、编辑、上传、下载和删除。
- renderer 不解析终端输出，也不直接访问 SSH/SFTP；路径判断在 Rust session/runtime 层完成。

### 2. Shell CWD 跟随策略

跟随终端目录时，FileTerm 按以下顺序尝试路径：

1. 先尝试 Shell 上报的原始路径；普通 Linux 主机因此保持原行为。
2. 只有 SFTP 返回明确的 `NoSuchFile` 时，才尝试服务端命名空间候选。
3. `/volumeN` 中的 `N` 从实际 CWD 动态识别，不写死 `volume1`。
4. 同时支持群晖常见的 `/var/services` 前缀和 `homes/<user>` 更深层 Home chroot。
5. 第一个成功列出的候选路径成为 `remotePath`，后续所有文件操作都使用这个 SFTP 路径。

示例：

| Shell CWD                       | 按顺序尝试的 SFTP 路径                                                                      |
| ------------------------------- | ------------------------------------------------------------------------------------------- |
| `/volume2/photo/albums`         | `/volume2/photo/albums` → `/photo/albums`                                                   |
| `/volume7/homes/alice/projects` | `/volume7/homes/alice/projects` → `/homes/alice/projects` → `/alice/projects` → `/projects` |
| `/var/services/homes/alice`     | `/var/services/homes/alice` → `/homes/alice` → `/alice` → `/`                               |

这不是无条件的字符串替换，而是“原路径优先、列目录成功确认”的候选探测。

### 3. 错误和安全边界

- `Permission denied`、超时和协议错误不会触发下一种路径猜测，避免把权限问题误显示成另一个目录。
- 所有候选都失败时，文件面板保留最近一次有效的 SFTP 目录，并结束 loading；不会因为 Shell CWD 无法映射而清空文件区或阻塞终端。
- root 文件模式使用 Shell/exec 直接列出物理路径，不把 SFTP 虚拟根映射逻辑套到 root 视图。
- 用户在文件面板中手动输入或进入的路径始终按 SFTP 命名空间处理，不进行反向猜测。
- 终端中用户输入的 `cd` 不会被 FileTerm 改写；Shell 仍由服务端决定命令是否成功。

当前实现位置：

- `apps/tauri/src-tauri/src/sessions/ssh.rs`：候选生成、SFTP 列目录探测和事件驱动的 CWD 跟随。
- `apps/tauri/src-tauri/src/commands/mod.rs`：重新开启“跟随终端”时的即时恢复路径。
- `sessions::ssh::tests`：`volumeN`、`/var/services`、Home chroot 和错误类型回归测试。

### 4. 无法确认映射时的行为

SFTP 协议本身不会返回“当前虚拟根对应 shell 物理路径的哪一段”这一映射信息。对于自定义 chroot，客户端无法在不读取服务端配置的情况下做到百分之百推断。

因此，FileTerm 只对已知路径形状做有限候选探测；如果没有候选能成功列目录，就保留当前 SFTP 路径并记录 `CWD follow failed`。用户仍可关闭“跟随终端”，在文件面板中手动浏览 SFTP `/` 或实际可见的 Home/共享文件夹。

## 同类产品的处理方式

同类产品通常不会把 Shell 物理路径直接当成 SFTP 路径：

- WinSCP 将远程目录作为独立的 SFTP 会话目录；“Synchronize browsing”是可选的浏览同步功能，未要求时仍由用户分别指定远端目录。[WinSCP 目录设置](https://winscp.net/eng/docs/ui_login_directories)
- MobaXterm 曾提供“Follow SSH path”式的跟随能力，同时保留禁用该功能的选项；这说明跟随是额外策略，不是 SSH/SFTP 的协议保证。[MobaXterm 官方更新记录](https://mobaxterm.mobatek.net/download-home-edition.html)
- Cyberduck 的常见方向是“从当前 SFTP 浏览目录打开终端”，把浏览器当前目录作为 `cd` 目标，而不是把 Shell 的物理 CWD 反向写回 SFTP 面板。[Cyberduck SFTP 文档](https://docs.cyberduck.io/protocols/sftp/)

FileTerm 采用折中方案：默认保留跟随体验，但只有在 SFTP 成功确认路径后才更新文件面板；无法确认时不做破坏性猜测。

## 排查清单

遇到“终端能进、SFTP 一直加载”时，优先看 `app.log` 中同一 tab 的以下顺序：

```text
SFTP session ready
initial directory listing started ...
Shell CWD reported: ...
CWD follow scheduled ...
CWD follow mapped shell=... sftp=...
```

如果看到 `CWD follow mapped`，说明 Shell/SFTP 根差异已被识别；如果看到 `CWD follow failed`，继续检查：

```sh
pwd -P
ls -ld /volumeN
ls -ld /volumeN/<shared-folder>
```

然后在独立 SFTP 客户端中确认其显示的 `/`、`home`、`homes` 或共享文件夹名称。重点是把 SFTP 客户端显示的路径作为文件操作路径，不要把 shell 的 `/volumeN/...` 原样复制到 SFTP 地址栏。

## 依据

- [Synology：启用 User Home 服务](https://kb.synology.com/en-us/DSM/tutorial/user_enable_home_service)
- [Synology：My Drive (home) 与 homes 的路径区别](https://kb.synology.com/en-nz/DSM/tutorial/Drive_difference_between_homes_My_Drive_home_folders)
- [Synology：共享文件夹说明](https://kb.synology.com/en-global/DSM/help/DSM/AdminCenter/file_share_desc?version=6)
- [OpenSSH `sshd_config`：`ChrootDirectory`](https://man7.org/linux/man-pages/man5/sshd_config.5.html)
- [WinSCP：Directories 页面](https://winscp.net/eng/docs/ui_login_directories)
- [MobaXterm：官方更新记录](https://mobaxterm.mobatek.net/download-home-edition.html)
- [Cyberduck：SFTP](https://docs.cyberduck.io/protocols/sftp/)
