# Root 文件视图兼容性审计

审计日期：2026-10-05。范围为 SSH 文件视图的 user/root 切换、目录命名空间、提权命令与目录列表；不是各发行版全部功能的认证。

## 群晖问题的证据

开发版日志显示 Shell CWD 为 `/volume1/homes/Stoffel`，普通 SFTP 文件路径为 `/homes/Stoffel`。弹窗验证授权后仍以 `/homes/Stoffel` 调用特权 Shell 列目录，返回 exit=1。终端 `sudo -i` 成功时会重新上报真实 CWD `/root`，因此文件区可以正常列出。用户截图证明这台设备有 `/root`；缺失 root 家目录不是本次已确认的原因。

## 实现调整

- 手动进入 root 前，使用所选 sudo/su 凭据验证目录：当前目录 → 从实际 shell CWD 推导的 NAS 别名映射 → shell CWD → 系统 `/`。不固定卷号、用户目录或 `/root`。
- 目录存在但权限不足、路径是现有文件时返回错误；仅缺失的候选继续探测。解析 `pwd -P` 返回实际目录，授权或目录验证失败恢复原模式与凭据缓存。
- 返回 user 时重新通过 SFTP 探测命名空间；明确路径不存在时可回到 SFTP `/`。实际权限、认证和协议错误仍返回错误。
- Rust 模式切换命令完成文件列表刷新，返回新 snapshot；renderer 不再通过旧闭包重新打开切换前目录。
- 特权文件命令使用 `sh -c`，避免依赖非 POSIX 的 `sh -l` 或登录脚本。密码继续通过受控 stdin 传递，服务器 sudo/PAM 策略决定需要哪个用户的密码。
- 运行时探测 GNU `find -printf`，支持时使用 `find -H` 跟随输入目录符号链接；不支持时使用 POSIX Shell 通配与格式化 stat。stat 依次探测 Linux `-c`、BSD `-f`。包含隐藏文件和断链，保留实际错误输出。

这些映射仅用于切换时选定目录。后续编辑、上传和删除不会猜测或替换用户指定路径。

## 缺失登录家目录的终端回退

交互终端在已支持的 POSIX shell integration 初始化及用户态切换重注入时，先检查 HOME。HOME 缺失或不是目录时，保留服务器已经选择的有效 CWD；如果 CWD 不可访问或 `pwd -P` 无法解析，则实际在同一个交互 Shell 执行 `cd /`，失败再尝试 `cd /tmp`。恢复脚本独立为短输入行，随后由既有 CWD hook 上报真实目录，经 runtime 同步文件区；该流程与 SFTP 初始化回退分开。

服务器的原始登录错误保持可见，HOME 不会被改写，也不会创建远端目录。截图中的 `user@host:/$` 已表明 sshd 回退到 `/`，该目录有效时不会再次切换。Windows、unknown 与不支持的 shell 不额外注入此脚本；fish 通过既有 guard 跳过 POSIX eval。

## 缺失登录家目录的文件区初始化回退

服务器原始 `Could not chdir to home directory ...` 输出保持可见。初始 SFTP 目录返回明确不存在时，依次读取服务器当前目录 `.` 和 SFTP 根目录 `/`；首个能成功列出的目录成为文件区路径，并在终端追加 `[files]` 提示，包含原始目录错误及回退目录。权限、认证和网络错误直接返回，不触发该回退。

目录探测仍在有超时及取消控制的异步初始化任务中执行。只有请求仍拥有当前初始目录、且处于 user 文件模式时，才更新路径和列表，避免覆盖用户后续导航或 root 切换。此行为不创建远端账号或目录，也不修改服务器 HOME。

## 主流系统文档核对

发行版衍生关系不等于实际镜像配置；路径和工具能力均在运行时验证。下表“覆盖”指设计与官方文档核对，不能理解为每种系统都已实机通过。

| 系统或家族                                        | 官方资料与核对项                                                                                                                                                                                                                                                                                                                                                   | 处理方式与边界                                                                                                                              |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Ubuntu / Debian；Mint、Kali 等 Debian 系衍生      | [Debian Root](https://wiki.debian.org/Root)、[Debian sudo](https://wiki.debian.org/sudo)、[Ubuntu 命令行教程](https://documentation.ubuntu.com/desktop/en/26.04/tutorial/the-linux-command-line-for-beginners/)：root 可锁定，sudo 受策略控制                                                                                                                      | 不要求直接 root SSH 或 root 密码；保留当前有效目录，按工具能力选择列表实现。衍生镜像按能力覆盖推断，未逐一实测                              |
| RHEL / Fedora / CentOS Stream / Rocky / AlmaLinux | [RHEL sudo 管理](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/10/html/security_hardening/managing-sudo-access)、[Fedora 安全基础](https://fedoraproject.org/wiki/SecurityBasics)、[Rocky 用户管理](https://docs.rockylinux.org/books/admin_guide/06-users/)、[AlmaLinux 用户管理](https://wiki.almalinux.org/beginners/users-and-groups.html) | 遵守 sudoers 与 wheel 授权；不将 SELinux 或权限拒绝当作缺失目录。工具选择不依赖发行版名称                                                   |
| Oracle Linux / Amazon Linux                       | [Oracle 管理访问](https://docs.oracle.com/en/operating-systems/oracle-linux/10/userauth/userauth-AboutAdministrativeAccessOnEnterpriseLinux.html)、[AWS AMI 用户](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/managing-users.html)                                                                                                                         | 不假设登录用户名、家目录或密码存在；支持既有免密 sudo 策略                                                                                  |
| SUSE / openSUSE                                   | [SUSE sudo 文档](https://documentation.suse.com/sles/15-SP7/html/SLES-all/cha-adm-sudo.html)：SUSE 默认 sudo 可要求 root 密码                                                                                                                                                                                                                                      | UI 提示改为服务器策略决定密码身份，避免固定描述为 SSH 用户密码                                                                              |
| Arch / Manjaro                                    | [Arch sudo](https://wiki.archlinux.org/title/Sudo)：安装与 sudoers 配置由系统决定                                                                                                                                                                                                                                                                                  | 不假设 sudo 必然安装或 root 必然可认证；Manjaro 按共同工具能力覆盖推断                                                                      |
| Gentoo / NixOS                                    | [Gentoo 安装手册](https://wiki.gentoo.org/wiki/Handbook:IA64/Full/Installation)、[NixOS 手册](https://nixos.org/manual/nixos/stable/)                                                                                                                                                                                                                              | 不依赖固定家目录或登录 shell；尊重实际提权工具与配置。未进行各自镜像运行验证                                                                |
| Alpine / BusyBox / OpenWrt                        | [Alpine BusyBox](https://wiki.alpinelinux.org/wiki/BusyBox)、[BusyBox 命令手册](https://busybox.net/downloads/BusyBox.html)、[OpenWrt SSH](https://openwrt.org/docs/guide-quick-start/sshadministration)                                                                                                                                                           | GNU find 扩展不作为前提。裁剪镜像缺少格式化 stat 时明确报错；SSH/SFTP 服务和提权工具仍须由服务器提供                                        |
| Synology DSM                                      | [Synology SSH root](https://kb.synology.com/en-us/DSM/tutorial/How_to_login_to_DSM_with_root_permission_via_SSH_Telnet)、[DSM Home 与 homes](https://kb.synology.com/en-nz/DSM/tutorial/Drive_difference_between_homes_My_Drive_home_folders)                                                                                                                      | 依据已观察 CWD 推导实际卷；处理 SFTP 与 Shell 根不同和目录符号链接，不固定 `/volume1`                                                       |
| 飞牛 fnOS                                         | [安装与管理员](https://help.fnnas.com/articles/v1/start/install-os)、[系统恢复](https://help.fnnas.com/articles/v1/settings/sysrestore)                                                                                                                                                                                                                            | 不猜固定用户家目录；依据实际 CWD 和能力探测。官方资料不足以确认所有版本的 sudo/PAM、SFTP 根映射，仍需设备回归                               |
| QNAP QTS / QuTS hero                              | [SSH 访问](https://www.qnap.com/en/how-to/faq/article/how-do-i-access-my-qnap-nas-using-ssh)、[sudoers 与管理员](https://www.qnap.com/en-uk/how-to/faq/article/sudoers-and-superuser-access-via-ssh-to-disable-admin-account)                                                                                                                                      | 官方资料存在版本差异，旧版可能使用 admin 超级用户且无 sudo。保留目标用户选择，不把所有 NAS 的超级用户名视为 root；未新增缺失提权工具        |
| TrueNAS SCALE / CORE                              | [SCALE 用户目录](https://www.truenas.com/docs/scale/credentials/users/manageusers/)、[CORE SSH](https://www.truenas.com/docs/core/13.0/coretutorials/services/configuringssh/)                                                                                                                                                                                     | SCALE 可能存在 `/nonexistent` 家目录及只读系统盘。CORE 属于 FreeBSD，不归为 Linux；BSD stat 仅新增目录列表兼容，未宣称全部 BSD 文件传输兼容 |

命令语义依据：[POSIX sh](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/sh.html)、[GNU find 符号链接](https://www.gnu.org/software/findutils/manual/html_node/find_html/Symbolic-Links.html)、[FreeBSD stat](https://man.freebsd.org/cgi/man.cgi?query=stat&sektion=1)。

## 验证与限制

新增 Rust 回归检查覆盖 NAS 卷号和别名、普通 Linux 路径、缺失 home、目录符号链接、引号与命令替换字符、目录帧解析、非登录特权命令、隐藏文件与断链。Shell 夹具在本机 macOS 验证 BSD stat 分支；在 Linux 运行同一检查会走 GNU find 分支。Renderer 回归检查确认两种切换都应用后端新 snapshot，不再刷新旧路径。

前一阶段 root 视图调整的质量门禁通过：`npm run test:tauri`（689 单元测试 + 10 CLI 测试 + 21 contract 测试）、新增 renderer 切换回归（2 项）、Tauri typecheck、全仓 lint、Prettier 和 Rust Clippy（locked/all-targets/all-features）。

新增的缺失初始目录回退已通过 `cargo check --locked`、Clippy（all-targets/all-features）、Rust 格式和 diff 检查；尚未在飞牛设备上验证。

本机 Docker daemon 不可用，未执行所有发行版容器或 NAS 实机回归。已有开发版日志用于定位群晖根因；改动后的 DSM/fnOS 结果仍需实际设备确认。自定义 chroot 无法仅靠 SFTP 协议完全反推；候选失败时使用已观察的 Shell CWD 或系统 `/`。

`requiretty`、MFA、仅允许特定命令的 sudoers、缺失 sudo/su、登录 shell 受限、只读挂载和 MAC 策略不能由客户端路径兼容修复。本次未修改上传 staging、原子替换或其他文件操作的工具依赖；不能由目录列表成功推断所有写操作均兼容裁剪系统。
