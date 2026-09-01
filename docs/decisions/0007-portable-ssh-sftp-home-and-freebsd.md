# ADR-0007: 使用服务端 Home 解析和 FreeBSD 采集器兼容异构 SSH 主机

## 状态

Accepted（2026-08-28）

## 背景

近期反馈中的“系统识别不出来”和“SFTP 一直加载”不是同一个问题：

- Serv00 截图中的主机是 FreeBSD 14.3，不是 Debian。它可以正常建立 SSH shell，但原有探测器只识别 Linux、BusyBox、macOS 和 Windows，因此平台会落到 `unknown`，Linux `/proc` 指标脚本也不适用。
- 阿里云 Debian 12 是另一台 Linux 主机。本机 `app.log` 已经记录到它完成 `platform=linux` 探测并产生 CPU/内存首个样本；它的问题集中在 SFTP 初始目录的 `READDIR`、服务器响应慢或连接被限制时，文件面板长时间保持 loading。
- SSH shell 的物理路径和 SFTP subsystem 的可见路径不能互相假设。Serv00 的 shell Home 通常位于 `/usr/home/<user>`；普通 OpenSSH 可能把 SFTP 当前目录解析到 `/home/<user>`；chroot 或群晖服务则可能把用户 Home 直接呈现为 SFTP `/`。

这意味着 FileTerm 不能把 `/volume1`、`/home` 或 shell 登录后的 `/` 当成所有服务器通用的规则。

## 官方依据

- Serv00 文档说明用户可以通过 SSH/SFTP 登录，SFTP 使用同一组登录凭据，并提供 `pwd`、`ls`、`cd` 等目录操作。[Serv00 SFTP](https://dev.docs.serv00.com/SFTP/)
- Serv00 的常用命令文档把用户目录、`~` 和域名目录作为 shell 侧的起点，而不是要求客户端写死某个卷名。[Serv00 Useful commands](https://dev.docs.serv00.com/Useful_commands/)
- Serv00 社区公告确认包含 `s2.serv00.com` 在内的服务器已更新到 FreeBSD 14.3。[FreeBSD 14.3 and Node.js v24](https://forum.serv00.com/d/3341-freebsd-143-and-nodejs-v24)
- Debian 12 的 OpenSSH 配置文档默认使用 `/usr/lib/openssh/sftp-server`，也支持 `internal-sftp`；`ChrootDirectory` 会在认证后改变 SFTP 会话看到的根目录。[Debian bookworm `sshd_config`](https://manpages.debian.org/bookworm/openssh-server/sshd_config.5.en.html)
- FreeBSD 基础系统通过 `sysctl` 提供 CPU、内存、启动时间和内核信息；磁盘、交换区和进程分别可由 `df`、`swapinfo` 和 `ps` 查询。[FreeBSD `sysctl(8)`](https://man.freebsd.org/cgi/man.cgi?manpath=FreeBSD+14.0-RELEASE&query=sysctl&sektion=8)、[FreeBSD `df(1)`](https://man.freebsd.org/cgi/man.cgi?query=df&sektion=1&manpath=FreeBSD+14.3-RELEASE+and+Ports)、[FreeBSD `swapinfo(8)`](https://man.freebsd.org/cgi/man.cgi?manpath=FreeBSD+14.4-STABLE&query=swapinfo&sektion=8)、[FreeBSD `ps(1)`](https://man.freebsd.org/cgi/man.cgi?manpath=FreeBSD+14.4-STABLE&query=ps&sektion=1)
- FreeBSD 官方的 `rctl(8)` 将账号/jail 的资源用量与适用规则分开查询：`rctl -u` 是当前用量，`rctl -l` 是适用规则；`memoryuse` 是常驻集大小，`pcpu` 是单个 CPU 核心百分比，`swapuse` 是交换空间，`maxproc` 是进程数。[FreeBSD `rctl(8)`](https://man.freebsd.org/cgi/man.cgi?query=rctl&sektion=8)
- FreeBSD 官方的 `quota(1)` 以 1024 字节块报告用户文件系统用量和限制，`-f` 可限定到 Home 所在文件系统；Handbook 同时明确说明 `rctl` 不负责文件系统空间，应使用文件系统/ZFS quota。[FreeBSD `quota(1)`](https://man.freebsd.org/cgi/man.cgi?query=quota&sektion=1)、[FreeBSD Handbook: Jail Resource Limits](https://docs.freebsd.org/en/books/handbook/jails/)

## 决策

### 1. 平台探测支持 FreeBSD，并保留 Debian 的兜底探测

- `RemoteSystemPlatform` 增加 `freebsd`。
- POSIX 探测继续优先使用带边界标记的 `sh -lc`，失败后单独执行 `uname -s`。这样受限 Debian 镜像即使拒绝 login shell 或被启动脚本污染，也有机会被识别为 Linux。
- `FreeBSD`/`freebsd` 输出识别为 `freebsd`，不再让 FreeBSD 落到 Windows 探测或 `unknown`。
- FreeBSD 不复用 Linux `/proc` 指标命令。Rust 后端选择单独的 FreeBSD 命令生成器，renderer 继续消费同一套 `__KEY__VALUE` 标记，不新增第二套 IPC 协议。

FreeBSD 采集器优先使用官方提供的账号级接口，缺失时才回退到主机级基础接口：

| 指标            | 账号级来源                                              | 主机级回退                                           |
| --------------- | ------------------------------------------------------- | ---------------------------------------------------- |
| CPU 总占用/分项 | `rctl -u user:<user>` + `rctl -l user:<user>` 的 `pcpu` | `kern.cp_time` 两次采样，限制在 0–100                |
| 内存            | `rctl` 的 `memoryuse` 与 `memoryuse:deny` 规则          | `sysctl` VM 数据（`hw.physmem` 等）                  |
| 交换区          | `rctl` 的 `swapuse` 与 `swapuse:deny` 规则              | `swapinfo -k`                                        |
| 文件系统        | `quota -v -f "$HOME"` 的用户 quota                      | `df -kP`                                             |
| 进程            | `ps -axo ...`，账号模式只保留当前用户                   | `ps -axo ...`，主机模式保留可见进程                  |
| 主机信息        | 不适用                                                  | `freebsd-version`、`uname`、`hostname`、`vm.loadavg` |

Serv00 等共享托管主机还可能提供 `devil info limits`；它不是 FreeBSD 标准接口，
仅用于补齐官方 `rctl`/`quota` 无法提供的账号配额（例如提供商把磁盘、RAM、CPU
统一放在一个面板中）。

在共享 FreeBSD 主机上，`sysctl`、`df`、`swapinfo` 和 `kern.cp_time` 可能返回宿主机
视角，不能直接代表登录账号的资源限制。采集器只在账号级接口同时给出完整的
“已用/上限”时覆盖对应指标；命令缺失、权限不足或输出格式变化时保持该指标的
主机级回退，避免把不完整的账号数据与主机容量拼成错误结果。

FreeBSD 暂不注入 Linux 风格的 shell CWD 脚本。项目的 CWD 注入门控仍保持对 Linux/BusyBox 开放、对未知平台 fail-closed；FreeBSD 使用 SFTP Home 解析和已有的手动/受确认 CWD 映射能力。

### 2. SSH 初始目录由 SFTP 服务端确认

- 新 SSH profile 的默认 `remotePath` 为 `.`；FTP profile 仍为 `/`。
- Rust 在 SFTP subsystem 建立后，对 `.` 执行一次 `canonicalize`，把服务端实际返回的路径作为初始 `remotePath`。
- 旧 profile 中的空字符串、`.` 和历史默认 `/` 都作为“进入服务端 Home”的隐式请求；明确填写的 `/srv/app`、`/volume2/photo` 等绝对路径不重写。
- 如果服务端不支持/不响应 `canonicalize`，保留原配置路径并记录失败原因，不猜测 `/volume1`、`/usr/home` 或其他固定前缀。
- canonical path 只属于 SFTP 命名空间，不能写回 `shellCwd`，也不改变终端中的 `pwd` 或 `cd` 行为。

典型结果如下：

| 服务端形态             | shell 侧可能看到的 Home               | SFTP `canonicalize(".")` 可能返回          | FileTerm 行为                                                 |
| ---------------------- | ------------------------------------- | ------------------------------------------ | ------------------------------------------------------------- |
| Serv00/FreeBSD         | `/usr/home/ddog`                      | `/usr/home/ddog` 或托管服务提供的虚拟 Home | 直接浏览返回路径                                              |
| Debian 12/普通 OpenSSH | `/home/user`                          | `/home/user`                               | 从用户 Home 开始，不把登录用户带到物理 `/`                    |
| Debian/OpenSSH chroot  | 物理路径可能更深                      | `/`                                        | `/` 视为该用户的 SFTP 根                                      |
| 群晖/自定义 SFTP 根    | `/volumeN/...` 或 `/var/services/...` | `/`、`/homes/user` 等                      | 以 SFTP 返回的命名空间为准；继续使用 ADR-0006 的有限 CWD 候选 |

因此普通用户只能看到自己有权限的目录是服务端认证、SFTP 根和文件权限共同决定的结果，不由客户端通过拼接物理路径实现。

### 3. 初始 SFTP 请求必须有独立收口

- 初始 `canonicalize` 最多等待 8 秒。
- 初始目录列表最多等待 `min(profile operation timeout, 15 秒)`；用户主动执行的后续文件操作仍使用 profile 配置的操作超时。
- 列目录时解析符号链接目标只是 UI 补充信息，单个目标最多等待 2 秒，不能让一个无权限或失效链接拖住整个目录。
- 初始列表、超时、失败和取消都结束 `remote_files_loading`，并写入带 tab 的 SFTP 日志；终端主循环不等待初始目录列表。
- SFTP subsystem 初始化失败仍只关闭文件能力，不能误报成 SSH shell 整体连接失败。

## 影响

### 解决的问题

- Serv00 FreeBSD 14.3 能进入专用指标采集路径，系统信息不再因 Linux `/proc` 不存在而全部缺失。
- Debian 12 不再依赖 login shell 才能完成平台识别；SFTP 初始目录也不再默认把用户带到不适合的物理根目录。
- 群晖、多卷 NAS、chroot 和免费托管服务器不需要客户端知道实际卷名；SFTP `/` 与 shell `/volumeN` 的差异被保留并记录。
- SFTP subsystem 接受请求但不完成 `READDIR` 时，文件区会在有限时间内结束 loading，并留下可诊断的失败日志，不会永久转圈。

### 不保证的内容

- 客户端无法仅凭 SFTP 协议推断“虚拟 `/` 对应 shell 物理路径的哪一段”；自定义 chroot 仍需要用户手动选择或由成功列目录确认。
- 如果服务端禁用了 SFTP subsystem、拒绝 `REALPATH`/`READDIR`，或账号没有目录权限，客户端不能绕过服务端权限；此时日志会区分初始化、canonicalize、目录列表和能力探测阶段。
- 本次兼容不会把 FreeBSD 的 shell CWD 强行注入为 Linux 脚本，也不会自动提升权限。

## 排查记录

终端能连接但文件区异常时，建议按同一 tab 查看 `app.log`：

```text
platform probe completed platform=linux|freebsd
SFTP session ready
initial SFTP home resolved configured=. resolved=/home/user
initial directory listing started path=/home/user timeout_secs=15
initial directory listing completed ...
```

在服务器侧可用以下无写入命令确认两个命名空间（不要把密码、私钥或完整主机指纹粘贴到日志）：

```sh
uname -s
uname -r
pwd -P
```

以及在本机单独验证 SFTP：

```sh
sftp USER@HOST
pwd
ls
```

SSH 密钥登录与密码登录只解决认证方式；它们不会决定 SFTP 是否 chroot，也不会把 shell 的物理路径自动变成 SFTP 路径。

## 实现位置

- `packages/core/src/index.ts`：远程平台类型。
- `apps/tauri/src-tauri/src/sessions/system_metrics/mod.rs`：POSIX/FreeBSD 探测、FreeBSD 指标命令和统一标记解析。
- `apps/tauri/src-tauri/src/sessions/ssh/mod.rs`：SFTP Home canonicalization、初始列表超时、符号链接单项超时和日志。
- `apps/tauri/src/renderer/app/app-data.ts`、`apps/tauri/src/renderer/features/connections/connection-modal.tsx`：SSH/FTP 默认路径分流。
- [ADR-0006：SSH Shell 与 SFTP 使用不同路径命名空间](./0006-ssh-sftp-path-namespaces.md)：群晖 `/volumeN`、`/var/services` 和 CWD 跟随候选规则。
