# FileTerm 架构规划

## 1. 目标

FileTerm 第一版要解决的是“桌面端远程工作台”的核心闭环，而不是协议大而全。

核心体验：

- 一个连接列表
- 一个多标签工作区
- SSH 会话中终端与 SFTP 文件联动
- Telnet 与 Serial 终端会话（不附带 SSH 文件或系统能力）
- FTP 会话中只呈现文件管理
- 上传下载任务全局可见

## 2. 当前实现状态

当前仓库已经具备 MVP 雏形，主要能力包括：

当前唯一受维护、构建和发布的运行时是位于 `apps/tauri` 的 Rust + Tauri。历史桌面实现已从仓库彻底移除（删除提交 `2a2eb7ff`），不再保留任何参考目录、依赖或构建脚本；共享包仍保持领域类型、纯工具和稳定数据格式。Tauri 已完成 Phase 0–4 的代码/contract 主体与本机协议夹具，发行仍以真实服务、实体设备、三平台 CI 和打包 UI 验收为准。

- Monorepo workspace 基础结构
- Tauri 主窗口启动
- 独立连接管理器窗口
- React 工作台主界面
- 标签页工作区模型
- SSH / FTP 连接配置的新增、编辑、删除、持久化
- 基于文件的 profile 存储（`profiles.json`）
- 本地目录浏览
- 本地文件读写
- SSH 会话连接
- SSH shell 输出接入
- SSH keyboard-interactive/MFA、SOCKS5/HTTP outbound proxy、多级 Jump Host（最多 8 个跳板） 与运行时 `-L/-R/-D` tunnel service；隧道运行状态归属具体 workspace tab，断线自动回收。
- SSH 连接可保存 Base32 OTP 密钥与可选提示识别文本，通过 keyboard-interactive 自动提交标准 TOTP（SHA-1、6 位、30 秒）；每次认证最多自动提交一次，重复/不明确的提示保持手动输入。密钥复用 `profile-secrets.json` 加密凭据存储，公开快照只暴露 `hasSavedOtpSecret`。
- SSH 私钥库：私钥、元数据和可选口令分离存储；连接 profile 仅保存 `privateKeyId`，口令交互通过 `ssh:interaction` 返回 renderer。
- SSH 私钥导入同时支持文件和临时文本输入；文本只在导入弹窗与一次 IPC 调用期间存在，不进入 profile、workspace snapshot 或日志。
- SSH 密码认证缺少凭据时，登录补全弹窗可显式改用密钥库私钥；响应仅传递密钥 ID，私钥解析与口令交互仍由 Rust 完成，选择只作用于本次连接。Kubernetes 模式保持密码认证。
- 终端写入与 resize IPC。主进程待写输入共享 16 MiB 额度（每条至少记账 64 字节），
  单次最多 1 MiB UTF-8，另留 4 KiB 控制字节余量；输入所有权穿过 SSH 暂存/写入队列及
  本地 PTY 线程队列，完成、失败或关闭后归还。Telnet 与串口未完成行也持有额度。
  renderer 每个 WebView 共享 1 MiB/4,096 条待发送额度并预留控制消息空间；满额明确
  拒绝整次输入，不截断粘贴或把连接标为断开。
- SFTP 远程目录浏览
- 远程文件读取与写回
- FTP 会话连接与远程文件能力
- 上传下载任务队列与进度状态
- 工作区快照广播到多个窗口
- Renderer 的 workspace 快照按 IPC 顺序更新为每个 hook 实例独立的不可变快照，React 通过 `useSyncExternalStore` 订阅；监控、目录与传输合并不放进可被渲染重试的 state updater 队列。窗口之间仍通过原生 workspace 广播同步。
- 预览态数据与桌面运行态双模式
- SSH 主机系统信息页与轮询刷新
- 首页、终端、系统信息、新建标签之间的工作区切换动效
- SSH 工作区底部文件面板的抽屉式收起/展开
- 终端命令输入条的雾透悬浮布局
- 侧栏收起态的资源监控摘要
- 文件编辑器的左侧文件树与右侧 Monaco 编辑区
- 工作区焦点模式，可同时收起侧栏和底部文件面板
- macOS 菜单栏 template 托盘图标资源
- Windows 安装包默认图标使用 `icons/app/windows/classic.ico` 经典银边图；运行时偏好通过 `app_icon/windows.rs` 设置窗口与窗口类图标、托盘，并将当前用户桌面、开始菜单和固定任务栏中的 FileTerm 快捷方式切换到版本/内容指纹唯一的 ICO 文件，再通知 Shell 刷新。旧版本启动后会把仍指向当前 FileTerm 可执行文件的默认快捷方式迁移到新版图标资源，保留快捷方式目标和用户自定义图标。
- Linux 的 deb、rpm、pacman 静态 PNG 使用经典银边；运行时由 `app_icon/linux.rs` 同步用户级 `FileTerm.desktop` 和匹配 Wayland app ID 的 `com.fileterm.desktop.desktop`，使用带内容指纹的变体路径刷新缓存，并更新启动命令指向 FileTerm、图标仍为默认值的桌面快捷方式。用户自定义图标和符号链接保持不变。开发版不写正式版启动器，deb/Arch 保留 `/usr/bin/fileterm` 包装入口。deb/rpm 显式使用同一桌面模板，CI 对实际解包后的图标内容与入口字段验收。
- 单一连接导入入口同时支持 `~/.ssh/config`、SSH 配置文本和外部 JSON；导入统一采用 main-process preview plan，renderer 不接收导入 secret。
- Telnet（RFC 854 基础协商）和 Serial（Rust device handle）终端会话；Serial 设备枚举通过 `app_list_serial_ports` 和 `serial:ports-changed` 经 Tauri bridge 暴露，连接表单同时保留手动设备路径。Serial 的换行、Text/Hex 收发、本地回显、调制解调器控制、X/Y/ZMODEM、Kermit 文件传输和断线策略随 profile/会话边界处理；两者不进入 SFTP、CWD、sudo 或系统指标链路。
- Serial 普通发送等待期间继续接收，复用同一流式解码、日志和终端输出路径；文件传输仍独占协议流。设备打开在 blocking worker 中执行，连接和测试连接共用 15 秒打开等待上限，取消或超时后的迟到句柄负责线路清理。连接列表和首页显示设备、波特率及帧格式摘要。
- 设置页的 WebDAV 手动配置同步：加密备份包包含连接密码、被引用的代理/隧道定义及其凭据、FileTerm SSH 密钥库中的私钥及已保存口令；上传使用 ETag 检测冲突，下载导入云端密钥但保留本机独有密钥。连接库快照与保存共用 Rust 锁，网络请求和密码等待不持有该锁；恢复及云端合并通过私有备份身份保持重复连接和重复导入的身份稳定，并重映射跳板引用；私有身份不进入对外快照。代理/隧道遇到本地冲突时创建新配置并重映射引用。renderer 仍不接触明文 payload，界面明确要求仅使用可信 HTTPS WebDAV。
- 设置页的 S3 兼容配置备份：完整包复用 WebDAV 的密钥打包、安全导入、hash 校验和 ETag 冲突策略；S3 签名、Access Key、Secret Access Key 与含凭据 payload 全部停留在 Rust 服务层。Cloudflare R2 作为预设，固定使用 `region=auto` 与 path-style 地址。
- 设置页的会话安全由 Rust `security.json` 保存：主密码只在 Rust 侧验证，空闲锁定由主工作区的 renderer 覆盖层执行；WebDAV/S3 备份包共用安全页中的加密密码，未配置时备份操作返回稳定错误并引导回安全页。Windows portable 使用 exe 旁的 `config` 存储该文件及其设备绑定密钥。

### 2.1 Rust/Tauri 迁移边界

- Tauri bridge 位于 `apps/tauri/src/bridge/tauri-api.ts`，Rust commands/services/sessions 位于 `apps/tauri/src-tauri/src/`；仓库内不存在第二个运行时目录。
- Tauri 当前已覆盖桌面壳、JSON 存储、Workspace snapshot、可迁移的 SSH 私钥库，以及 russh SSH shell/SFTP/MFA/host verification、系统指标、CWD 跟随、重连水化、自动重连、远程编码、递归 chmod、多级 Jump Host（最多 8 个跳板）、SOCKS5/HTTP CONNECT 代理和运行时 SSH `-L/-R/-D` 隧道。
- Tauri 已覆盖 Transfer、FTP/FTPS、Telnet、Serial、WebDAV 同步及 SSH 网络能力；Phase 3/4 的真实服务、实体设备和三平台验收仍是发行候选门禁。SFTP 被服务端拒绝时，Tauri 保留 SSH shell/隧道，并将文件通道故障单独广播到 renderer，而不误报为整条 SSH 连接失败。
- SSH shell 与 SFTP 可能处于不同的文件系统根；`shellCwd` 保留 Shell 物理路径，`remotePath` 保存当前文件通道的路径：user 模式为 SFTP 命名空间，root 模式为 Shell 物理路径。CWD 跟随先尝试原路径，遇到明确 `NoSuchFile` 后按实际 `/volumeN`、`/var/services` 和 Home chroot 候选探测，无法确认时保留最近有效的 SFTP 目录。初始 SFTP 目录明确不存在时，后台初始化保留错误提示，并依次验证服务器当前目录 `.` 与 SFTP `/`；成功目录与列表一起更新，避免缺失账号 home 导致空文件区。完整背景、群晖示例和排查方式见 [ADR-0006](./decisions/0006-ssh-sftp-path-namespaces.md)。
- SSH 初始文件目录以 SFTP `canonicalize(".")` 返回的服务端 Home 为准，不写死 `/volume1`、`/home` 或 `/usr/home`；初始目录列表、符号链接元数据和取消路径均有独立超时收口。远程系统指标对 FreeBSD 优先使用官方 `rctl`/`quota` 账号接口，缺失时回退到 `sysctl`/`swapinfo`/`df`/`ps` 主机级基础命令，不复用 Linux `/proc` 采集器；共享托管主机的 `devil info limits` 仅作为提供商补充来源。完整兼容边界、Debian 12 与 Serv00 记录见 [ADR-0007](./decisions/0007-portable-ssh-sftp-home-and-freebsd.md)。
- 迁移期间 `packages/core` 的领域类型和现有 JSON 数据格式保持兼容；协议 controller 仍按 SSH、FTP、Telnet、Serial 分离。

当前主题系统也已经开始成形，主要落点包括：

- `apps/tauri/src/renderer/styles/themes/tokens.css`
  - 基础视觉 token，供半径、阴影、间距等全局样式复用。
- `apps/tauri/src/renderer/styles/themes/fileterm-dark.css`
- `apps/tauri/src/renderer/styles/themes/fileterm-light.css`
  - 只保存明暗主题 token（包含 `default-dark` / `default-light` 兼容别名）。
- `apps/tauri/src/renderer/styles/features/component-skins.css`
  - 内置主题的组件皮肤与兼容覆盖层；保持旧 UI 的选择器作用域和级联顺序。
- `apps/tauri/src/renderer/hooks/use-theme-mode.ts`
  - 通过 `document.documentElement.dataset.theme` 切换主题。
- `apps/tauri/src/renderer/components/terminal-view.tsx`
  - 从 CSS 变量读取终端主题色，确保终端外观和全局主题联动。
  - `terminal-lifecycle-core.ts` 使用 xterm 的最低文字对比度保护，在显示层处理 ANSI/256 色/RGB 与选区/搜索背景，不改写协议流。
  - DOM 渲染的颜色对比缓存每组最多 8,192 个颜色对，字符宽度缓存最多 16,384 项；仅淘汰可重新计算的结果，保留字体校准、ASCII 快表和历史内容。适配器按终端创建/销毁，私有缓存结构受 xterm 版本回归约束。
  - 同一 WebView 文档内，字体、字号、字重/斜体、字体特性、语言、字距和 DPI 等测量环境一致的终端，复用实际测得的字符宽度。共享测量结果总计最多 32,768 项，超过 64 个 UTF-16 单元的字符组合不进入共享缓存；字体加载/失败和校准失效时清空结果，最后一个终端销毁时移除字体监听器并释放共享状态。
  - 相同主题与日志颜色不重复失效缓存或扫描历史；文件面板的吸附观察仅覆盖 shell 与侧栏，不订阅终端子树重绘。
  - 符号显示槽严格采用 buffer 列宽，避免 fallback 字体实际 advance 与重复字形测量不同而推偏表格；不把 `✻`、`※` 等单列字符扩大为双列。笔画越界时先平移，确实超宽才等比缩小；双列 emoji 的笔画位于选区背景上方，避免从延续格开始选择时被遮住。保留 buffer 列宽与原始复制文本。
  - `terminal-block-glyph-renderer.ts` 将块字符和连续横线 `─`、`━`、`═` 按字符格几何绘入逐行设备像素位图，复用实时 palette 与选区前景，避免字体边缘和 ANSI span 接缝造成断线或粗细变化。同一原始文本 / 样式段里的连续图形共用一个透明 DOM 占位节点，避免长横线逐格建节点放大后续布局测量；不跨 ANSI / 选区来源合并，位图仍按每格绘制。
  - 图形适配器先筛选需要处理的行，再读取布局；普通文本不触发符号/块字符几何测量。行数缩小时移除越界位图，图形变为普通文字时仍清理旧画布。
- `apps/tauri/src/renderer/xterm-adapters/`
  - 归集 xterm 的私有 API、DOM 渲染、Unicode/emoji、输入法、回包过滤、Vim 选区、字体缓存和 WebView 兼容实现；目录 README 说明模块职责与注册时机。组件层负责生命周期、应用快捷键、会话守卫和 PTY resize 编排。共享字体观察器保留在 `app/font-metrics.ts`，xterm 专用的字体栈脉冲在适配目录内完成。
- `apps/tauri/src/renderer/xterm-adapters/terminal-log-colorizer.ts`
  - 在 xterm 完成解析后对普通缓冲区的时间戳、服务名和常见日志级别着色；先匹配文本，
    命中才建立字符到终端列的映射，保留仍适用的颜色，仅刷新前景实际变化的行；不向远端输出注入 ANSI，不处理 `top`、`vim`、`less` 等备用屏幕程序，并随终端主题重新套用颜色。
- `apps/tauri/src/renderer/xterm-adapters/terminal-foreground-adapter.ts`
  - 让正常/备用屏幕默认背景上的 RGB 纯白文字使用主题默认前景，保留样式与显式背景/反色配对；只适配可见 xterm cell，不改写协议数据或原始 transcript。
- `apps/tauri/src/renderer/app/action-text-color.ts`
  - 主题 runtime 根据主按钮/危险按钮背景（透明色先合成 panel surface）选择更易读的白色或深色文字，经参考值和语义变量传入组件。
- `apps/tauri/src/renderer/features/files/file-editor-modal.tsx`
  - Monaco 主题从 CSS 变量读取，跟随深色/浅色主题切换。
  - 显示空白字符时，`file-editor-line-endings.ts` 仅为可见行注入 LF（`↓`）/CRLF（`↵`）标记，不写入编辑模型或保存内容。标记跟随 Monaco 当前统一的换行格式；独立 CR 与混合换行在加载模型时仍按 Monaco 原有行为归一化。
- `packages/core` 的 `ThemeConfig.baseThemeId` 记录自定义主题继承的内置基准，`UiPreferences.customThemes` 保存用户命名的主题；renderer 只将相对基准真正变化的变量覆盖到主题根节点，避免微调一个颜色时整套组件皮肤被重算。

## 3. 技术栈

当前已安装、已接入的第三方项目和维护边界见 [integration-inventory.md](./integration-inventory.md)。本节包含部分阶段规划项，精确现状以该集成总表和 `package.json` 为准。

### 当前已采用

- Tauri v2 + Rust
- `russh` / `russh-sftp`
- `suppaftp` / `tokio-serial` / `reqwest`
- React
- TypeScript
- Vite
- xterm.js
- Monaco Editor
- OpenCC
- Tauri bundler
- 文件型 profile 存储

历史桌面实现的专用依赖（`ssh2`、`basic-ftp`、`serialport`、历史桌面打包器）已从仓库移除；当前产品能力与发行使用 Tauri 对应 Rust crates，不应再把这些依赖接回共享包或 CI。

### 已放弃或暂缓

- Tailwind CSS：已放弃迁移。FileTerm 需要大量覆盖 xterm、Monaco、桌面壳和表格/文件面板细节，继续使用 `token -> theme vars -> component skins` 的 CSS 分层更可控。
- Radix UI / shadcn/ui：暂不引入。当前自有组件和桌面式密集布局更贴近产品形态，避免再叠一层组件体系。
- react-resizable-panels：暂不引入。现阶段文件面板和侧栏 resize 已由本地组件处理。
- Zustand：已完成评估，当前不引入。`app.tsx` 已拆为 7 个领域 hooks 与布局组件，尚未出现需要跨多层组件共享可变状态的稳定场景；继续使用 React state、领域 hooks、workspace snapshot 与 IPC 广播。
- zod：暂不引入。类型先收敛到 `packages/core`，运行时校验如果变复杂再单独评估。
- SQLite / Drizzle ORM：暂缓。当前连接配置使用文件型 profile 存储；等 profile、设置、传输历史的查询需求明确后再迁移。
- 系统钥匙串：暂缓。敏感信息存储策略需要单独做安全设计，不混在 UI 或协议改动里。

## 4. 高层分层

```txt
Renderer UI
  -> Application Stores
    -> IPC Bridge
      -> Desktop Services
        -> Session Controllers
          -> Protocol Adapters
            -> Remote Servers
```

分层职责：

- `Renderer UI`
  - 界面渲染
  - 用户交互
  - 布局与组件状态
- `Application Stores`
  - 标签状态
  - 当前工作区状态
  - 连接列表
  - 传输任务
- `IPC Bridge`
  - 暴露类型安全的前后端调用接口
- `Desktop Services`
  - 配置存储
  - 密钥管理
  - 会话创建与销毁
  - 文件传输调度
  - 系统信息采集调度与快照广播
- `Session Controllers`
  - 会话生命周期管理
  - 状态机
  - 通道复用
  - 面向协议与平台的原始系统指标采集
- `Protocol Adapters`
  - SSH
  - SFTP
  - FTP
  - Telnet (RFC 854 基础协商)
  - Serial (Rust device handles and Tauri bridge)

## 4.1 系统信息能力边界

当前系统信息页先服务于 SSH 会话，但边界需要按“可扩展到多平台”来建立：

- `packages/core`
  - 只定义原始指标结构与 renderer 需要的通用字段，不承载中文或英文展示字符串。
- `apps/tauri/src-tauri/src/sessions/*`
  - 负责按远端平台探测并采集原始指标，例如 Linux `/proc`、BusyBox 兼容命令、未来的 Windows PowerShell。
- `apps/tauri/src-tauri/src/services/workspace/state.rs`
  - 负责轮询、节流、网络历史合并、快照广播。
- `renderer/features/system/*`
  - 只做展示、本地化、布局与表格格式化，不承担平台分支判断。

现阶段的实现仍然偏 `Linux over SSH`，但后续扩展应保持：

```txt
platform probe
  -> raw metrics collection
    -> normalized core shape
      -> snapshot/runtime merge
        -> localized renderer presentation
```

SSH 资源监控由 `sessions/ssh/worker/metrics/` 的独立管理任务拥有持久采集通道。采集能力与
`SessionSnapshot.monitoring` 运行健康状态分离：暂时无有效样本不撤销能力、不清空数据；
后端按有效间隔计算暂停/恢复阈值，并提供最多五次恢复及手动跳过退避。状态与样本经
`workspace:sessionMetrics` 和完整快照同步，generation/revision 拒绝旧任务和乱序事件。
Renderer 只显示遮罩、倒计时和操作反馈，`app_retry_monitoring` 只重试监控通道。
父监控管理任务保留重启上下文和控制接收端，采集子任务异常退出后可手动重建；
监控断开与 `ssh-disconnected` 分别展示监控重试和现有 `reconnectTab` 会话重连入口。
监控开关的手动关闭状态在同一标签 SSH 重连后保留。
关闭标签、断开会话或显式禁用监控会取消任务。网络历史以 `breakBefore` 标记中断。
同一连接配置的多个会话共用一条采集通道：`WorkspaceState.monitoring_leases` 按会话捕获的配置 id、目标地址/端口、登录用户及配置的路由身份发放租约，
持有者采集，其余会话进入 `shared` 阶段并以 `sharedFromTabId` 指向持有者；Renderer 据此显示持有者的样本，
`MonitoringState.targetKey` 只包含非敏感目标身份的摘要，编辑保存配置不会把旧会话重定向到新目标；启停只作用于目标一致的会话。持有者停止、断开、关闭或耗尽重试进入终态失败时释放租约，下一个会话接管采集，renderer 显示健康接管者的指标。
详细参数、启停交互和自动化验证见 [监控恢复、侧栏遮罩与启停控制](./plans/completed/monitoring-recovery-overlay.md)。

## 4.2 Renderer 桌面壳与布局边界

系统侧栏进程数据由远端采集器分别选取 CPU、内存和命令排序的前 40 条，去重后最多传输 120 条；renderer 根据当前排序展示 40 条。`SidebarProcessItem.commandOrder` 保留采集端命令排序，`memoryBytes` 用于数值排序，避免用已取整的展示文本排序。进程 CPU 使用整机百分比，Linux 以进程 tick / 全机 tick 的同窗口增量计算，并校验 PID 的启动时间；不能把前 40 条的合计冒充整机仪表，也不能按脚本解释器名称过滤用户进程。

设置页提供的本地 CLI 直接调用片段在 Windows 上面向 PowerShell：使用 `& '可执行文件路径'`；MCP 注册片段中的可执行文件属于参数，仅引用路径，不插入调用运算符。POSIX shell 与 PowerShell 分别处理路径中的单引号。

桌面壳相关状态目前属于 renderer 本地 UI 状态，不进入 main service：

- 顶部标签栏、工作区焦点模式与标签切换动效由 `features/layout/tab-bar.tsx`、`features/workspace/workspace-stage.tsx` 和 `app.tsx` 协作。
- 标签页布局（`single-row` 单行，默认 / `host-groups` 二级标签页）是 Rust 持久化的 UI 偏好 `tabBarLayout`，非法值归一为默认的 `single-row`；renderer 经 `features/layout/use-tab-bar-layout.ts` 读取并跟随 `app:ui-preferences-changed` 更新，主机分组由 `features/layout/host-groups.ts` 与 `use-host-navigation.ts` 计算。新建会话的首个快照会同步补齐可见标签，不等待排序持久化 effect；关闭活动标签优先选择同组右邻，其次左邻，再回退到其他组。左侧主机栏与主页共用宽度，复用默认宽度吸附、拖拽和双击复位，支持收起为图标栏，右侧监控栏独立调宽。紧凑标签栏 `compactTabBar`（默认 `false`）同为 Rust 持久化偏好，renderer 经 `use-compact-tab-bar.ts` 在 `.fs-shell` 上切换 `is-compact-tabbar`（36px / 48px），macOS 原生标题栏随偏好刷新同步基线。
- SSH 工作区的文件面板高度、抽屉收起状态、终端悬浮输入条由 `features/workspace/session-workspace.tsx` 与 `features/terminal/terminal-dock.tsx` 处理。
- 首页侧栏展开/收起、macOS 红绿灯避让和收起态左边界由 `features/workspace/home-workspace.tsx` 与 `styles/features/home.css` 处理。
- 系统监控的展开态详情和收起态资源摘要都在 `features/system/system-sidebar.tsx` 展示，不把平台判断或采集逻辑放进 renderer。
- 文件编辑器窗口继续通过 `Rust commands/events -> tauri-api.ts -> renderer` 打开，但编辑器内部的文件树、工具栏、状态栏和 Monaco 主题属于 renderer 组件状态。

平台差异的原则：

- renderer 从 `tauri-api.ts` 暴露的平台信息同步到 `document.documentElement.dataset.platform`。
- 设置页展示的 runtime 名称与版本来自 Tauri bridge 和 Rust command，renderer 不写死运行时信息。
- Tauri 的 renderer UI 状态通过 Rust command 持久化为 `ui-state.json` 键值对象；读取时兼容历史 `{ version, values }` 包装格式和早期 `{ key, value }[]` 数组格式，业务组件不得自行假设另一种文件结构。
- 文件面板左侧显示模式通过 `file-panel.left-pane-mode` UI state 保存，默认 `local`，可选择 `remote-tree`。目录树使用 `app_list_remote_directory` 只读 IPC，复用既有带超时和取消的协议 worker，不更新会话当前目录或列表；节点缓存按标签、连接和 user/root 模式隔离，路径联动由现有 workspace 快照驱动。
- CSS 使用 `data-platform` 和稳定 class 做 macOS/Windows/Linux 差异化布局。
- Tauri 无边框子窗口在 macOS 与 Linux 使用透明原生表面，由 renderer 的 `standalone-window-frame` 统一裁切圆角；Windows 窗口使用不透明原生表面，避免 WebView2 圆角处透出桌面。Windows/Linux 主窗口使用无边框 renderer chrome，并共用紧凑菜单栏。三端主窗口均以 `visible: false` 创建；`renderer/lib/main-window-reveal.tsx` 在应用首屏 React 提交并经过绘制机会后，通过 `tauri-api.ts` 的 `showCurrentWindow()` 显示主窗口，含定时兜底以兼容隐藏 WebView 暂停动画帧。Linux 主窗口原生表面透明，但正常页面由 CSS 绘制实色背景；Linux 独立窗口不得附加 GTK 应用菜单。
- Windows 下严禁从同步 Tauri command、托盘回调或原生菜单回调直接执行 `WebviewWindowBuilder::build()`；WebView2 会在该上下文发生建窗死锁并阻塞后续全部 invoke。Renderer 建窗入口必须使用 async command，实际 builder 工作进入 blocking worker；原生事件入口也必须先交给 worker。
- macOS 菜单栏托盘图标使用 `apps/tauri/src-tauri/icons/tray/macos/template*.png` template 资源（源文件位于 `apps/tauri/assets/icons/trayTemplate.svg`），由 Rust/Tauri backend 设置 template 属性。
- macOS 主窗口保留原生红黄绿按钮；`tauri*.conf.json` 不保存 traffic-light 坐标。macOS 配置以 `visible: false` 创建主窗口；Rust 恢复窗口尺寸、读取保存的 `uiZoomPercent` 并校准原生布局，renderer 应用主题和偏好、提交首屏后再显示窗口，确保红绿灯和内容都在首次可见帧前就位。从 `NSWindow` 真实 frame 计算顶栏高度（`36 或 48 × uiZoomPercent / 100` 逻辑点，取决于 `compactTabBar`），同步原生标题栏容器，并经按钮 superview 的 AppKit 坐标转换移动三个原生按钮中心。原生 frame/bounds、圆形绘制及横向中心间距保留。原生窗口 update / resize / 退出全屏以及标题栏 frame 通知同步校准，取代页面加载后的定位和 140ms 去抖；布局使用重入保护和几何差异检查，避免重复绘制。缩放偏好缓存于原生布局状态，UI preferences 变更刷新缓存后主动校准，通知回调不读取存储。全屏期间由系统管理自动隐藏标题栏；退出全屏恢复自定义几何。系统共享指示器沿用 AppKit 原生布局，不按私有类名查找或单独搬动；通知观察者在进程退出时移除。Debug、Release、本地打包和 GitHub Actions 共用同一算法，不允许 `debug_assertions` 分支、Release 垂直偏移或 renderer CSS 伪造按钮；精确设计基线见 `docs/design.md` 的“macOS 原生标题栏基线”。
- Tauri 托盘由 Rust backend 显式创建：macOS 使用独立 template 图标，Windows/Linux 使用应用图标。macOS 可在「设置 → 界面 → 窗口启动」中隐藏菜单栏图标，默认显示；隐藏时仍可从 Dock 图标重新打开主窗口。主窗口与可见子窗口隐藏到托盘后会成组恢复；普通关闭请求与真正退出请求保持分离。
- 应用更新通过 Rust/Tauri update service 统一管理，renderer 仅经 `Rust commands/events -> tauri-api.ts -> renderer` 查询状态和触发检查；更新通道持久化为 `stable` / `beta`，默认使用 `beta`；稳定版只选择非 prerelease Release，测试版选择 prerelease 与正式版中 SemVer 最新的一项，以便测试版自然升级到正式版。Windows 按选中的 Release tag 动态读取 `latest.json`，安装版按运行架构选择 `windows-x86_64` 或 `windows-aarch64` 的签名 NSIS 安装器，便携版选择对应的 `-portable` 独立签名 EXE；两者复用“下载验签 → 重启更新”界面。便携版通过原程序副本运行专用辅助入口，在取得父进程句柄并确认就绪后请求 Tauri 退出，等待进程结束再替换原目录、原文件名的 EXE；保留 `config` 与 `portable` 标记，替换或启动失败时回滚，更新事务与备份位于程序目录下的独立子目录。旧 Release 缺少便携签名条目时回退到下载页；macOS 发行构建保留 ad-hoc 应用签名，复用 Tauri updater 的签名下载与待安装状态，从同一 `latest.json` 选择 `darwin-aarch64` / `darwin-x86_64` 更新归档；不接入 Apple Developer ID 或公证。安装前等待文件编辑器保存确认，在应用同目录用 `ditto` 保留独立备份，安装失败恢复旧版；成功后保存传输断点、关闭会话并重启。DMG、App Translocation 和不可写目录不执行应用内安装；系统拦截新版时由用户手动放行。首次接入更新器仍需手动安装，旧 Release 缺少 macOS 清单时回退下载页。
- Linux AppImage、DEB、RPM 复用签名下载和安装确认，按架构与包类型匹配 `linux-<arch>-<appimage|deb|rpm>`，不跨格式 fallback；DEB/RPM 还须确认当前程序由相应包管理器的 `file-term` 包拥有，防止 Arch 重打包的 DEB 标记误触发安装。AppImage 的签名下载仍走 Tauri，安装额外校验 Type 2 ELF/架构，在原文件所在目录备份和暂存，回读 SHA-256、保留执行权限并刷盘后原子替换；暂存中断不修改原文件，并发变化拒绝覆盖并保留备份；DEB/RPM 通过 Tauri updater 调用系统提权安装，可能要求管理员授权。成功后保存传输断点、停止会话并重启；Arch/pacman、开发态、解包或未确认包归属时继续使用 Release 下载页。Linux 安装前后校验重启路径存在且可执行，不能重启时保持应用运行；包归属查询限时两秒，更新检查限时二十秒，更新网络连接/读取停滞分别限时十五/六十秒。Linux payload 必须在 universal DEB 与 AppImage/zsync 处理完成后签名并合入统一清单。
- 原生关闭快捷键由 Rust/Tauri backend 统一收口：macOS 使用 `Cmd+Q` 请求应用退出确认、`Cmd+W` 请求关闭当前工作区项/子窗口；Windows/Linux 分别保持 `Alt+F4` 退出与 `Ctrl+W` 关闭窗口语义。最后一个工作区项只触发普通窗口关闭/隐藏，不得直接销毁主窗口；托盘退出和应用退出快捷键必须走同一确认与 transfer journal 清理链路。真正退出前还必须逐个等待独立 Monaco 编辑器完成保存或确认丢弃，任一编辑器取消都会中止 session/transfer shutdown。
- 主题和语言属于 Rust backend 持久化的 UI preferences；Tauri 应用菜单、托盘菜单与 Windows/Linux 自绘 menubar 必须从同一份 locale 构建并在语言切换后刷新。Linux 不安装 GTK 全局应用菜单，避免它注入每个独立窗口并破坏 renderer 主题。macOS 应用菜单保留 About/Services/Hide/Bring All to Front 等标准角色，但 Quit 仍走 FileTerm 自己的脏编辑器、活动连接和 transfer cleanup 确认链路。资源监控是 SSH 连接配置，关闭后该连接不采集资源数据，工作区仅保留窄侧栏；开启时支持 1/5/15/30/60 秒采样，默认保持 1 秒，用户可按需切换到低频模式。
- 首次启动的 UI 语言由系统首选语言决定：中文语言环境使用简体中文，其余语言环境使用英文；用户保存的语言选择优先。Rust 在创建默认 preferences 时判定系统语言，renderer 在偏好读取失败时按 WebView 的首选语言回退，并同步 HTML `lang` 属性。
- xterm 与 Monaco 会缓存字体尺寸/像素比并生成运行时样式，统一使用随包的 JetBrains Mono；本地字体完成加载、窗口缩放或跨显示器 DPI 变化后必须主动重测。生产 CSP 仅禁止 Tauri 为 `style-src` 注入 nonce，使声明的 `unsafe-inline` 能供这两个受信任的本地组件生成样式；`script-src` 继续保留 Tauri 的 hash/nonce 加固。
- Renderer 的持久化操作必须返回并应用 Rust command 的最新 snapshot；跨窗口广播只负责同步其他窗口，不能作为发起窗口更新成功状态的唯一来源。弹窗、传输与隧道操作在 React 提交态之外还必须使用同步 guard，避免下一帧禁用按钮前的快速双击重复调用 backend。
- 独立表单保持原生窗口隐藏，等工作区快照（代理/隧道表单则等各自配置库）就绪并提交两帧后显示；隐藏 WebView 暂停动画帧时保留显窗超时回退。同一表单 URL 再次打开只聚焦，切换目标时隐藏并导航，由新 renderer 就绪后显示；命令、代理与隧道编辑入口使用同步 guard 防止重复请求。
- 本地文件面板访问 SMB/UNC 路径时，首次认证失败由 Rust 返回稳定的 `SMB_CREDENTIALS_REQUIRED` 标记，经 `tauri-api.ts` 进入 renderer 凭据弹窗，再通过 `app_connect_local_network_share` 重试。macOS 主机级路径先查询可访问共享目录并由 renderer 让用户选择，再只挂载所选 `smbfs` 共享；Windows 使用临时 WNet 连接。SMB 系统命令有超时保护，账号和密码只在本次 IPC/系统连接期间驻留内存，不写入 profile、日志或 workspace snapshot；macOS 挂载会在应用退出时卸载。
- Tauri workspace tab 状态由 Rust 枚举限制为 core `TabStatus` 的 `idle/connecting/connected/error/closed`；正常或主动断开使用 `closed`，worker/连接失败使用 `error`，renderer 不接受运行时自造状态字符串。
- profile、folder、command 的持久化 mutation 由 Rust workspace 级锁串行化，成功后统一广播 `workspace:snapshot`；完整快照读取使用同一把锁，不能观察跨文件级联写入的中间态。广播失败只记录告警，不能把已经落盘的操作伪装成失败并诱发重复提交。
- Rust 存储层可在 backend-side 读取时解密并合并 `profile-secrets.json` 到短生命周期的内部 profile，但 `workspace:snapshot` 与独立窗口使用的 connection library 在跨 IPC 前必须统一剥离密码、私钥口令、私钥路径、代理密码、sudo 密码和 su 密码；公开 profile 仅可携带 `hasSavedPassword` / `hasSavedSudoPassword` / `hasSavedSuPassword` 这类非敏感存在标记。sudo 与 su 凭据始终分开保存和使用，不复用 SSH 登录密码。renderer 编辑脱敏 profile 时提交的空白 secret 只表示“未替换”，由 backend-side 保留原值；显式 `null` 才表示清除。表单层 `proxyPassword` 必须在 backend-side 规范化为 `proxy.password` 后进入 secret 文件，不能落入公开 `profiles.json`。
- SSH 可见终端中，只有在明确检测到 `sudo -i` 或 `su -` 的密码提示后，才允许从对应的 profile 凭据自动填充；未出现密码提示时不得盲写密码，也不得把密码写入 transcript 或日志。文件区工具栏切换 root 访问模式时始终打开方法选择窗口；用户选择 `sudo` 或 `su` 后，空密码提交由后端自动使用对应的已保存凭据，不向 renderer 回显密码，手动填写的密码只用于本次验证并按既有规则缓存。验证失败必须回滚当前权限状态。
- Rust backend 在 Tauri userData 缺少迁移 marker 时，最多一次导入历史用户目录中的应用自有 JSON/SSH key 数据；Tauri 当前数据按 ID 优先，legacy 只补缺失记录，整批 staging/commit 失败会回滚且不写 marker。迁移成功后不再 live merge，Chromium session 与缓存始终不迁移。Windows 便携版（`*-portable.exe`、带 `FILETERM_PORTABLE_BUILD=1` 编译标识的 portable 可执行文件，或 exe 同目录带 `portable` 标记文件）将应用自有存储根目录切换为 exe 旁的 `config`，普通安装版和开发版继续使用 Tauri app-data 目录；第一次切换时若 `config` 为空，会从原 Tauri app-data 复制应用自有 JSON、凭据密文、密钥和字体数据，使用独立 marker 防止重复覆盖，诊断日志和 MCP 运行时描述文件不复制。首次成功启动后，portable 还会在 exe 同目录持久化 `portable` 标记，因此用户清空 `config` 后不会再次从 `%APPDATA%` 或历史用户目录回填数据；启动、迁移、字体导入和字体文件修复路径均写入同一存储根目录下的 `logs/app.log`。
- 本地凭据字段（AI API Key、SSH 私钥口令、profile 密码/代理密码、sudo/su 密码、WebDAV 密码及 S3 Access/Secret Key）在 Rust 存储层以 AES-256-GCM 加密后再写入 JSON；密钥由每安装随机 seed 与当前设备稳定标识经 HMAC-SHA256 派生，并以字段用途/记录 ID 作为 AAD 绑定，旧版明文在首次读取后原子迁移。该实现不接入 macOS safeStorage/钥匙串、Windows DPAPI 或 Linux credential store，不触发系统授权弹窗；Unix seed/secret/key 文件在创建、迁移和读取自愈时收紧为 `0600`，Windows 依赖应用数据目录的用户 ACL。它防止静态文件被直接读取或被单独误传，不对获得当前用户运行权限的本机主动攻击者提供保护。WebDAV/S3 的密码加密远程备份包含连接凭据与 FileTerm 管理的 SSH 私钥；用户显式导出的连接 JSON 继续按既有格式处理。所有备份序列化仅在 Rust 服务层完成，不进入公开 snapshot、renderer 预览或日志。
- Tauri backend 的持久化诊断统一进入 `services/logging/`：`tracing` subscriber/layer 负责结构化事件、span 上下文与 `FILETERM_LOG_LEVEL` 过滤，现有 `logging::*` 函数作为兼容 facade 发出 tracing event；`tracing-log` 汇入依赖的 `log` facade 事件。文件 writer 执行有界队列、大小轮转与凭据标签脱敏。系统与日志的 `basicLoggingEnabled` 默认开启，控制 app/security/updates/logging 类别的基础 INFO 记录；详细诊断分类独立开启，WARN/ERROR 和崩溃不受这些开关影响。偏好由 Rust 持久化，启动恢复并在保存后即时应用。服务层不得只写 `stderr`；终端内容、文件内容、密码、token、私钥口令和完整主机指纹不得进入诊断日志。
- 用户主动开启设备配置中的“自动保存会话日志”后，`services/session_logs.rs` 独立按 tab 写入终端收到的输出；它不采集键盘输入，远端回显内容仍可能出现在日志中。默认目录为当前存储根目录下的 `session-logs`（便携版位于 exe 旁的 `config\session-logs`），也可按设备指定目录；关闭标签、重连或退出应用前会刷新写入队列。手动保存通过 `app_save_session_log` 和系统保存对话框导出当前 transcript，FTP 不提供该能力。

## 4.3 传输暂停与恢复边界

- `packages/core` 的 `TransferTask.tabId` 记录任务创建时所属的连接标签，renderer 据此隔离同一 profile 下的并行标签任务。
- 标签关闭后，旧任务仍按 profile 作为可重新打开的历史断点；仍然存在的其他连接标签任务不会串入当前标签。
- 主动暂停、标签关闭、连接断开、应用退出和重启恢复都只保留断点并进入 `paused`，不会自动续传；继续传输只能由用户显式触发。
- 单次传输取消通过 controller 调用参数中的 `AbortSignal` 传播。该信号只属于运行时，不进入 `TransferTask` 或传输日志。
- 真正退出应用时，Rust backend 必须先等待活动任务停止并刷新 transfer journal，再放行窗口关闭和应用退出；macOS 隐藏窗口不触发 workspace shutdown。
- Tauri SSH 在同一个已认证 russh transport 上拆分浏览 SFTP 与传输 SFTP channel；传输 channel 失效后按任务重建，服务端拒绝额外 channel 时才回退主 SFTP。FTP/FTPS 则保留一条控制/浏览连接，并为每条上传下载建立独立协议连接，避免大文件数据流阻塞目录操作。

## 4.4 SSH 终端与文件身份联动

- SSH session 的实现按目录职责组织：`files.rs`、`shell.rs`、
  `authentication.rs` 和 `transport.rs` 只保留兼容 facade，具体 SFTP/传输、
  CWD/root/setup/encoding、主认证/KBI、以及 host/jump/proxy/session 逻辑位于
  `sessions/ssh/` 下的片段中；这些片段仍共享 session 的私有边界，后续再收口为
  独立 Rust module 与 `SshSessionContext`。
- 远端 shell integration 在 prompt 上报真实 `cwd` 和 `id -un`，renderer 不解析命令文本、提示符或 `sudo` 输出。
- 每个 SSH controller 的首次用户上报是登录身份；后续终端用户变化会单向驱动文件访问身份，并在切换成功后按最新 cwd 重新跟随。
- 文件区手动切换 user/root 只改变独立的 SFTP/exec 文件通道，不向交互终端写命令；相同 shell 用户的重复 prompt 不会覆盖手动选择。受支持的 POSIX 交互终端在 hook 初始化前确认缺失 HOME 下的 CWD：有效 CWD 保留，无效 CWD 在同一 Shell 尝试 `/`、再尝试 `/tmp`；保留服务器登录报错并通过既有 CWD 事件同步实际目录。进入 root 前，Rust 根据当前文件路径和已观察到的 shell CWD 验证实际目录；缺失目录依次尝试 NAS 映射、shell CWD 和系统 `/`，不假设 `/root` 存在。返回 user 时重新通过 SFTP 确认路径。模式切换命令在后端完成列表刷新并返回完整 snapshot，renderer 不再用切换前闭包刷新旧路径。兼容性范围见 [root 文件访问审计](./quality/root-file-access-compatibility.md)。
- 终端与文件通道不是同一远端进程。文件区进入特权身份会通过独立 exec channel 重建与终端一致的 sudo 或 su 策略；优先复用终端输入期间已捕获的授权，也支持远端免密 sudo / su。
- 普通远程 exec 的 `sudo` / `su` 前缀由 Rust 在独立 exec channel 中处理：sudo 使用 `-S` 通过 stdin 发送凭据，su 只在该独立 exec channel 上设置受控 PTY 标志；密码不进入命令文本、可见终端、日志或 tool result。凭据优先使用用户明确的一次性参数、加密 profile，缺失时由 Rust 恢复、解除最小化并聚焦主窗口，再展示本地安全 prompt；Copilot 对话区和工具活动都会显示等待前台输入的说明。主窗口/renderer 不可用时返回 `SUDO_PASSWORD_NEEDED` / `SU_PASSWORD_NEEDED`，用户取消或超时返回对应的 `*_PASSWORD_CANCELLED`，由 Agent 按结果处理。MFA/验证码/确认/REPL 等 generic input 仍返回 `REMOTE_INTERACTIVE_INPUT_REQUIRED`。
- 特权上传的字节流仍走登录用户可写的随机 /var/tmp SFTP staging，只有 staging → 目标断点/替换等短文件命令走 sudo/su，避免把大文件流塞进 su 的 PTY；历史 /tmp staging 任务继续兼容恢复，避免把旧断点遗留在不可访问路径。

## 4.5 MCP / CLI 外部客户端桥接边界

外部调用审计在桌面 bridge 生命周期内记录开始、完成及中断，通过唯一调用 ID 合并。日志为数据目录中的 `mcp-calls.jsonl` 和上一份 `.1`，每份最多 8 MiB；独立锁文件串行化跨进程读写与轮转。仅保存按动作过滤的元数据，省略命令文本、文件正文、模板参数和原始错误消息。旧记录读取时重新过滤，但已有文件中的原始内容不会被追溯删除。桌面生成的 bridge session ID 与 workspace 解析的目标 profile/tab 分开保存，Hello 中的自报 client ID 不作为身份凭据。审批结果、调用是否返回成功和命令退出状态分别记录。查看接口在阻塞工作线程中读取两份日志，提供 `nextCursor` / `before` 稳定分页，游标已超出轮转保留范围时明确要求刷新；查看页使用 `before` 游标追加记录，保留旧 `offset` 参数以兼容已有调用。

FileTerm 自带一套面向外部客户端的能力桥，与内置 AI Copilot 面板**完全独立、并行、不互相调用**。外部 MCP/CLI 与内置 Copilot 不共享 Provider 会话，但共享 `services/action_review/mod.rs` 的一次性审批队列、独立 SSH exec channel 和目标校验；外部客户端的基础安全操作映射到同一桌面审批边界。

```txt
外部 Agent（MCP）       外部 Agent（CLI JSONL）    shell 脚本
   │                         │                    │
   │ stdio JSON-RPC          │ request/response   │ argv
   ▼                         ▼                    ▼
fileterm mcp             fileterm cli --jsonl fileterm cli <cmd>
   │                         │                    │
   └──────────── BridgeClient（进程级单连接） ────┘
              │ request_id 多路复用 / 单一 writer
              │ read mcp-runtime.json (owner-only)
              │ TCP 127.0.0.1:随机端口
              │ ConstantTimeEq token 比较
              ▼
         桌面 runtime
              │
              │ dispatch_bridge_request
              ▼
   workspace / sessions / transfers
   ── 写操作 ──> action_review/mod.rs 审批队列
                       │
                       ▼
              应用内审批对话框
```

- 代码集中在 `apps/tauri/src-tauri/src/services/mcp/mod.rs`（手写实现，无 `rmcp` / `clap` 依赖）；审批队列在 `services/action_review/mod.rs`；入口路由在 `apps/tauri/src-tauri/src/main.rs`。
- `fileterm mcp` 与 `fileterm cli --jsonl` 各自在进程启动时创建一个 `Arc<BridgeClient>`；同一进程内的 worker 通过一条已认证的 loopback TCP session 复用连接，内部 `request_id` 将乱序 response、progress 和 cancel 路由回对应调用。连接建立后不会在每条工具调用后主动关闭；客户端退出、桌面端退出或连接异常时才结束 session。
- bridge session 使用一次性 hello/token/version 握手，之后持续处理 `request` / `cancel` / `ping` / `close` 帧，并由单一 writer 串行输出。reader 检测到断线后只允许一个 recovery 流程按有界退避恢复；在途请求不自动重放，恢复失败进入短暂 circuit breaker，避免 Agent 重试风暴。bridge 和 progress writer queue 都是有界的，满载时返回可识别的 retryable 错误或丢弃非终态 progress，不会无限积压。
- 同一份桌面可执行文件承担 GUI、MCP server 和 CLI 两种模式：无命令参数时启动桌面 GUI；`mcp` → `run_mcp_stdio`；其他参数先进入 `run_cli`，其中 `cli --jsonl` → `run_cli_jsonl`，其余为一次性 CLI。所有非 GUI 参数都在 Tauri 初始化前处理，避免把外部客户端请求误启动成桌面窗口。
- `fileterm cli --jsonl` 是不初始化 Tauri GUI 的常驻 JSONL bridge，使用固定 worker pool 处理多个 request ID；每个请求的 progress 和最终结果均带回同一个 ID。CLI JSONL 请求始终强制遵循桌面端审批策略，并可用 `cancel_request` 取消请求；正在执行的 SSH exec 会在原通道上尽力发送终止信号并关闭通道，但不回滚桌面端已经接受的其他操作，也不声称远程进程一定已经退出。外部 Agent 必须启动一次 `fileterm cli --jsonl` 或 `fileterm mcp` 并复用进程，不能按每个动作启动一次性 CLI；一次性 CLI 仅保留给用户手动调试和 shell 脚本。一次性 CLI 每次调用都会创建新的 FileTerm OS 进程，2.2.7 的 headless 分流避免启动 GUI，但不会让一次性进程变成复用连接。连接 single-flight 只能去重桌面连接任务，不能消除已经启动的进程。CLI/MCP 的后台打开请求仍由正在运行的 FileTerm App 持有同一个 session worker；它会隐藏在顶部标签栏之外，显示在“后台会话”页面，GUI 或 `fileterm_activate_session` attach 后才进入可见标签。
- 桌面应用 setup 阶段在 `lib.rs` 调用 `start_runtime`，绑定 `127.0.0.1:0` 随机端口，把 `{protocol_version, address, token}` 写到 owner-only 的 `mcp-runtime.json`；进程退出时清理 descriptor 文件。
- `mcp-runtime.json` 路径在三平台：macOS `$HOME/Library/Application Support/com.fileterm.desktop/`、Windows NSIS 安装版 `%APPDATA%\com.fileterm.desktop\`、Windows portable 当前 `fileterm.exe` 旁的 `config\`、Linux `$XDG_DATA_HOME/com.fileterm.desktop/` 或 `$HOME/.local/share/com.fileterm.desktop/`。CLI 端用 `#[cfg(target_os)]` 读取对应路径，依赖 `tauri.conf.json` 的 identifier 保持 `com.fileterm.desktop` 不变；可用 `FILETERM_MCP_RUNTIME_FILE` 环境变量覆盖。
- 公共 MCP JSON-RPC 协议版本固定 `2025-06-18`，server 不与 client 协商，无论 client 发什么版本都回自己的支持版本；内部 desktop bridge protocol version 当前为 `2`，仅用于 hello/session framing，不暴露为公共 MCP 版本。实现 MCP 的 `initialize` / `ping` / `tools/list` / `tools/call`，并通过 `notifications/progress` / `notifications/message` 报告外部命令等待 FileTerm 前台密码输入的状态；审批、sudo/su 密码提示及 SSH 登录凭据、主机验证、私钥口令、MFA 交互状态还保存在每次调用最终成功或失败的 `interactionHistory` 中（MCP 为 `structuredContent.interactionHistory`，CLI JSONL 为顶层字段）。desktop bridge 在返回前收集并排空该请求的状态事件，最终响应带回完整记录；client 以最终记录替换可能缺失的实时通知，避免宿主隐藏通知、writer 队列丢弃 progress 或 dispatch 完成竞争导致 Agent 丢失交互。记录仅含事件类别、状态和稳定代码三个字段，不含密码、口令长度、提示正文、回答、命令、用户名、主机信息或内部请求 ID。SSH 初始连接的认证交互按 operation 保留最多 16 次，超过时明确返回 history-truncated，不伪造完整记录。`requested` 表示后端已请求界面弹窗，`input-completed` 只表示本地安全输入已完成；不把它当作认证成功，也不从退出码 0 或无交互记录推断免密；未实现 `resources/*` / `prompts/*`。
- MCP `initialize.instructions` 和 `tools/list` 都发布同一套 Agent workflow 规则；只读 `fileterm_get_agent_contract` / `fileterm cli agent-contract` 还提供机器可读的 transport、identifier、长任务、下一步和恢复契约。连接与后台命令成功响应带 `agent.next`，失败响应区分 transport `retryable` 与副作用级 `safeToRetry`，避免 Agent 把断线后的未知结果盲目重放。
- 暴露 42 个 `fileterm_*` 工具，覆盖连接管理、会话状态、远程命令执行、远程文件操作、传输调度、SSH 隧道和 Agent workflow contract；tool annotations 标注 `readOnlyHint` / `destructiveHint` / `idempotentHint` / `openWorldHint`。MCP 首次调用 `fileterm_open_connection` 必须携带用户选择的 `execution_mode`（`background` / `visible-terminal`）；`background` 会创建不进入顶部标签栏的后台 session，并返回 `sessionId`（同时保留 `tabId`），`visible-terminal` 会创建可见但不活动的 session。GUI 的“后台会话”页面和 `fileterm_activate_session` 都能把后台 session attach 到可见标签，并复用原 worker。`WorkspaceTab.source` 记录外部来源 `cli` / `mcp`，后台会话页面、工作区底部会话 ID 和外部操作审批弹窗都显示对应来源标签；普通 GUI 会话不设置该字段。普通服务器的 `fileterm_execute_remote_command` 始终走独立、非交互 SSH exec channel，不污染可见 PTY；长部署、镜像构建和迁移使用 `fileterm_start_remote_command` 启动一次，再通过 `fileterm_read_remote_command` 按 offset 读取同一通道的增量输出，必要时使用 `fileterm_terminate_remote_command`，收尾后用 `fileterm_close_remote_command` 释放缓存；该命令不会因 MCP 请求超时或 SSH 重连而自动重跑。`fileterm_execute_visible_command` 只有在显式 `fileterm_activate_session` 后才向可见终端写入单行命令。网络设备没有后台 exec 能力，后台工具返回 `NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED`，必须使用可见终端路径；FTP / Telnet / Serial 不伪装支持远程 exec。
- `fileterm_execute_remote_command` 使用独立、非交互 SSH exec channel；请求取消或超时后在该通道上按 INT → TERM → KILL → close 做有界的 best-effort 清理，绝不在重连通道上重跑已接受的命令。长命令的后台工具在接受命令后把 reader/writer 和有界输出缓冲交给 workspace registry，MCP bridge 只承载启动、增量读取和终止/关闭控制；后台最长 6 小时，输出缓存上限 256 KiB，单次读取上限 64 KiB。MCP/CLI 的 sudo/su 可接收用户明确的一次性参数或复用加密 profile。缺少凭据时，FileTerm 自动恢复、解除最小化并聚焦主窗口后进入本地安全 prompt；Copilot 对话区和 MCP/CLI bridge 都会报告等待前台输入，原始命令继续等待用户完成密码后再返回最终结果，外部 Agent 不应重复调用。内置 Copilot 永远不接收或转发密码字段，只使用已保存凭据或主窗口安全 prompt；如果 prompt 不可用，它会停止本轮并等待用户明确重试。MCP/CLI 在主窗口/renderer 不可用时返回 `SUDO_PASSWORD_NEEDED` / `SU_PASSWORD_NEEDED`，外部 Agent 可以询问用户后用一次性字段重试；用户取消或超时返回对应的 `*_PASSWORD_CANCELLED`。密码不进入命令文本或 tool result。普通命令遇到 MFA、确认、安装器或 REPL 输入时返回 `REMOTE_INTERACTIVE_INPUT_REQUIRED`，用户必须在可见 SSH tab 完成。旧的 `fileterm_execute_command_template` 也属于可见终端路径，MCP 调用要求目标 session 已激活。
- MCP / CLI 设置存于 `UiPreferences.mcpAgent`，只含非敏感策略：连接白名单（全部已保存连接 / 指定已保存连接）与操作策略（仅只读 / 基础安全操作 / 完全访问）。该策略在桌面 bridge 的 action route 前执行；连接列表、会话上下文、传输列表、清理传输的剩余结果与等待传输也必须按白名单过滤，不能只依赖 renderer 隐藏。基础安全操作自动放行查询和完整匹配固定白名单的只读远程命令，命令前缀、额外参数及 shell 复合语法不能自动放行；变更、破坏性、提权或未知命令，以及打开/重连/断开会话、文件变更、上传下载、传输管理、隧道、sudo/su 和未知 action 送回 FileTerm 主窗口审批；一次性 CLI、CLI JSONL 与 MCP 共用同一判定。设置页以两张全局策略卡片展示执行权限和允许连接，不增加 per-connection MCP 权限；旧的活动会话/默认连接配置只在读取时安全迁移。设置页仅检测本机 PATH 和常见用户安装目录，生成 Claude Code / Codex CLI / OpenCode 的可复制注册命令，不执行客户端、不自动写配置文件。
- 设置页还可以按用户点击创建新的本地 PTY tab 并写入固定的 `claude`、`codex` 或 `opencode` 启动命令；它不是后台 CLI JSONL 进程或隐藏终端代理。密码、MFA、登录确认和 TUI 输入只会出现在这个可见的本地终端，MCP 不能触发或向该 terminal 注入任意连续键盘输入。Agent tab 使用可选、受 Rust 校验的本地 terminal title，renderer 只对枚举的受信任 client id 选择启动命令。本地 terminal 允许复用统一 pane tree 拆分，但同一树只含 local pane；每一个 pane 都启动独立 PTY/runtime、输入通道和取消令牌，不能与 SSH pane 混合或共享 shell 状态。这里的本地 PTY 只是用户点击启动的可见 Agent 终端，不改变外部 Agent 必须复用 MCP/CLI JSONL bridge 的约束。
- 安全约束：`subtle::ConstantTimeEq` 常时 token 比较、非 loopback peer 拒绝、非 loopback descriptor 地址拒绝、单条消息上限 2 MiB、单文件读写上限 512 KiB、并发客户端上限 8、bridge writer queue 128、bridge progress queue 64、审批超时 120s、sudo/su 前台密码提示最长 300s、bridge 单次读写超时 5s（后台命令增量读取可额外等待 30s）、连接等待最长 125s、需审批/密码/命令执行串联时的 bridge 有界等待为 545s、CLI/MCP client 超时 600s（设计上大于最长 bridge 等待，保证审批或密码输入超时不触发 client 读超时）。
- **MCP 与 CLI 的关键差异**：两者都使用同一份 action 路由、bridge 和全局策略；基础安全操作中的查询和完整匹配固定只读命令白名单的远程命令自动执行，变更、破坏性、提权或未知命令，以及打开/重连/断开会话、写操作、传输、隧道、sudo/su 和未知 action 回 FileTerm 主窗口审批。CLI 只是输出协议不同，不是权限绕过；完全访问才会跳过包括 sudo/su 操作在内的逐次操作审批，但 sudo/su 密码仍可能需要输入。
- **内置 AI Copilot 不通过本机 MCP 反向调用自己**，也不 spawn `claude` / `codex` 子进程；它直接读 workspace runtime 拿会话上下文。纯对话请求不携带 Provider tools；半自动 / 全自动使用 Rust-owned 的 `tool-call -> approval/guardrail -> isolated exec -> tool-result` 循环，工具调用绑定 L2、leaf/root、CWD/user 和 `sessionRevision`。详见 `docs/plans/completed/ai-copilot-modes.md`。
- 当前完成度：核心代码已构建、已注册，覆盖协议、连接去重、访问策略、CLI JSONL 审批/取消、交互提示识别、脱敏与审计纯函数测试；`apps/tauri/src-tauri/tests/cli.rs` 覆盖 CLI/CLI JSONL 子进程 stdout、stderr 和退出码回归。打包产物手工验收与真实 SSH/FTP/网络设备连接仍按目标环境验收。

## 5. 当前仓库结构

```txt
fileterm/
  AGENTS.md
  README.md
  package.json
  tsconfig.base.json
  docs/
    architecture.md
    roadmap.md
    plans/
      active/
      completed/
    decisions/
    quality/
  .agents/
    README.md
    extensions/
      README.md
  apps/
    tauri/
      index.html
      package.json
      tsconfig.json
      vite.config.ts
      src/
        bridge/
          tauri-api.ts
        renderer/
          app.tsx
          i18n.ts
          main.tsx
          vite-env.d.ts
          components/
            terminal-view.tsx
          features/
          hooks/
          styles/
            themes/
      src-tauri/
        src/
          commands/
          services/
          sessions/
          storage/
          lib.rs
          main.rs
  packages/
    core/
      src/
        index.ts
    shared/
      src/
        index.ts
    storage/
      src/
        index.ts
```

## 6. 仓库结构

当前仓库采用以 Tauri 为唯一维护运行时的 `npm workspaces` monorepo。协议能力保留在 Rust
session/service，不下沉为伪通用 `protocol-*` package。历史桌面实现已从仓库移除（删除提交
`2a2eb7ff`），需要对照旧行为时请从 git 历史检出。

```txt
fileterm/
  apps/
    tauri/
      src/
        renderer/
        bridge/
      src-tauri/
  packages/
    core/
    storage/
    shared/
```

### 目录职责

- `apps/tauri`
  - Tauri CLI、Rust 后端、Tauri bridge 与专用 React renderer
  - 只处理 Tauri 窗口、commands/events 与对应发行产物
- 历史桌面实现
  - **已删除**（提交 `2a2eb7ff`），当前仓库不保留其 main、bridge 与专用 React renderer
  - 如需查阅历史实现，请从 git 历史中检出该提交之前的版本
- `packages/core`
  - 领域模型
  - Session interface
  - 文件操作统一抽象
  - 状态定义
- `apps/tauri/src-tauri/src/sessions`
  - SSH 连接、shell channel、SFTP、FTP/FTPS、Telnet 与 Serial session/service
- `packages/storage`
  - profile repository
  - settings repository 预留
- `apps/tauri/src-tauri/src/services`
  - 文件型 profile 存储、workspace、updates、transfers 与 profile repository
- `packages/shared`
  - types
  - constants
  - 轻量共享工具
- `apps/tauri/src/renderer`
  - Tauri 的 UI 组件、hooks、主题、终端、文件管理与窗口布局
  - 跨层只在 `packages/*` 共享无运行时依赖的类型和数据格式；renderer 不允许绕过 `bridge/tauri-api.ts` 直接 `invoke`

## 7. 会话模型

### 7.1 Session Type

```ts
type SessionType = 'ssh' | 'ftp'
```

### 7.2 Profile

```ts
interface BaseProfile {
  id: string
  name: string
  host: string
  port: number
}

interface SshProfile extends BaseProfile {
  type: 'ssh'
  username: string
  authType: 'password' | 'privateKey'
  password?: string
  privateKeyPath?: string
  passphrase?: string
  sftpEnabled: boolean
}

interface FtpProfile extends BaseProfile {
  type: 'ftp'
  username: string
  password?: string
  secure: boolean
}

type ConnectionProfile = SshProfile | FtpProfile
```

### 7.3 Runtime Session

```ts
interface SshSession {
  id: string
  type: 'ssh'
  shell: ShellChannel
  sftp: SftpClient
}

interface FtpSession {
  id: string
  type: 'ftp'
  ftpClient: FtpClient
}
```

### 7.4 SSH shell cwd 跟随

SSH 终端与远程文件区通过真实 shell cwd 联动，不解析用户输入的 `cd` 文本：

```txt
shell integration
  -> OSC 7 cwd
    -> SSH controller cwd changed
      -> workspace runtime follow policy
        -> SessionSnapshot
          -> renderer
```

`SessionSnapshot` 中三个字段语义保持独立：

- `shellCwd`：远端交互式 shell 当前工作目录。
- `remotePath`：远程文件面板当前展示目录。
- `followShellCwd`：该 tab 是否允许 cwd 变化驱动文件面板。

shell 注入按 bash、zsh、fish、POSIX 风格策略选择；探测或注入失败时只降级目录跟随，不影响 SSH、SFTP 和终端输入输出。

主 shell 就绪后立即运行终端事件循环；辅助阶段与终端并行，内部先探测平台和网关路由，再初始化 SFTP 与启动系统指标。SFTP 就绪后的工作区快照在后台构造，不阻塞文件通道交付或 shell integration。Shell 启动各步骤保留独立超时并优先响应取消；终端退出会取消本次连接的后台任务。辅助探测结束时若用户已经开始输入，则跳过初始 shell 注入，避免脚本与用户命令混写。终端 stdout/stderr 分别保留跨 SSH 数据包的未完成 UTF-8 字符，完整字符串才进入 shell 过滤与终端合批。

`loginUser` 来自首次 shell `RemoteUser` 标记，不能直接使用可能包含容器或堡垒机路由信息的 SSH username。真实身份变化继续同步 sudo/su 文件权限，普通登录身份始终使用 SFTP。开启目录跟随时，相同 CWD 的新提示符也刷新文件列表，以反映解压、创建和删除文件；刷新队列只保留一个活动读取和一个最新待处理请求。

### 7.5 Workspace Tab

```ts
interface WorkspaceTab {
  id: string
  sessionType: 'ssh' | 'ftp'
  profileId: string
  title: string
  layout: 'terminal-file' | 'file-only'
  status: 'idle' | 'connecting' | 'connected' | 'error' | 'closed'
}
```

## 8. 为什么 SSH/SFTP 与 FTP 必须拆开

这是第一版最重要的建模约束。

### SSH/SFTP

- 同一个认证上下文
- 同一个目标主机
- 终端与文件面板天然联动
- 未来可扩展端口转发、远端命令、路径跟随

### SSH 多级跳板生命周期

SSH profile 继续通过 `jumpProfileId` 引用上游连接，最多经过 8 个普通 SSH 跳板。
后端在打开网络前解析完整链路并校验 SSH 类型、引用、循环、端口及逐节点的 proxy/jump
互斥规则，再从最外层依次通过 `direct-tcpip` 建立 SSH 会话。每跳独立认证和校验主机
指纹，`hopIndex` 按实际路径递增；终端、SFTP 和运行时转发使用最终目标的会话。
整条链的 handles 由当前连接拥有，认证失败、取消、测试完成或关闭会话时显式断开。
连接表单和工作区底部显示已解析的跳板路径，候选项排除循环和失效链路。

### SSH config 别名连接

`authType: 'sshConfig'` 的 profile 只保存 OpenSSH `Host` 别名（`host` 字段）。每次打开
会话时，`services/ssh_config` 按 OpenSSH 规则读取 `~/.ssh/config` 与系统 `ssh_config`
（先到先得、`IdentityFile` 累加、`Host`/`Match host`/`Include`、`%n`/`%h`/`%p`/`%r`/`%u`/`%d`
等 token），`sessions/ssh/transport/ssh_config_route.rs` 把别名展开为真实地址并把 `ProxyJump`
链展开为运行时跳板（不另存 profile，与 profile 级跳板/代理互斥）。profile 中非空的用户名
覆盖 `User`，`Port` 缺省时回退到 profile 端口；`ProxyCommand` 不执行，会明确报错。

认证使用 SSH agent：`IdentityFile` 可以只是 `.pub` 公钥（或私钥的 `.pub` 兄弟文件），按
配置顺序挑选 agent 中对应的身份签名，`IdentitiesOnly yes` 时只尝试这些身份；agent 优先级为
`IdentityAgent` > 设置中的 `connectionDefaults.sshAgentSocketPath` > `SSH_AUTH_SOCK`（Windows
默认 `\\.\pipe\openssh-ssh-agent`，再回退 Pageant）。agent socket 设置只在运行时注入 profile，
不写回连接。配置展开出的跳板没有 profile，主机密钥先比对 `UserKnownHostsFile`（默认
`~/.ssh/known_hosts`），用户选择“接受并保存”时写入该文件；目标主机仍保存在 profile 指纹中。

### FTP

- 独立协议
- 无 shell
- 文件操作是完整主路径
- FTPS 仍属于 FTP 家族，显式 TLS、隐式 TLS、被动/主动数据通道和代理能力都保持在 FTP controller 内

SSH/SFTP 和 FTP/FTPS 会在会话建立后上报实际能力，而不是把所有协议都标成“支持”：SFTP 会探测协议版本、磁盘空间、校验和、服务端复制及链接操作，FTP/FTPS 会读取 `FEAT` 并仅启用服务器声明的校验能力。探测失败只收窄文件能力，不伪造协议语义。

如果一开始为了“统一文件协议”硬揉成一个大 `RemoteSession`，后面会很快出现：

- 一堆条件分支
- 一堆无意义空能力
- 布局逻辑混乱
- 状态类型失真

所以这里必须拆。

## 9. IPC 与服务边界

前端不要直接接触协议客户端，所有能力都走服务边界。

### 建议 IPC 模块

- `profile`
  - 创建连接配置
  - 更新连接配置
  - 删除连接配置
  - 查询连接配置
- `session`
  - 打开会话
  - 关闭会话
  - 重连
  - 查询会话状态
- `terminal`
  - 输入
  - resize
  - 粘贴
  - 订阅输出
- `remoteFile`
  - list
  - stat
  - mkdir
  - rename
  - delete
  - upload
  - download
- `transfer`
  - 查询任务
  - 暂停任务
  - 继续任务
  - 丢弃任务与断点
  - 清理历史

### 9.1 高频事件边界

完整 `WorkspaceSnapshot` 只用于标签、会话、文件列表等低频结构状态，不承载高频流式更新：

- 传输层逐数据块累计真实字节数，Rust backend 最多每 200ms 发送一次轻量 `transfer:update` 任务事件；完成、失败和取消立即发送。
- Renderer 的传输订阅与列表状态收敛在独立 `TransferCenter`，进度变化不更新顶层 workspace state。
- Tauri 的 SSH 终端输入使用 renderer 到 Rust backend 的单向 command，并进入每个 tab 独立的无界输入 channel；SSH worker 在写入 PTY 前按序合并当前积压，不能与 SFTP/文件操作共用有界 worker command 队列，也不能因该队列满而丢失按键。
- 终端 resize 同样使用单向 command；终端输出在 Rust backend 按 16ms 合并，再交给 renderer 按 16 KiB 分批写入 xterm；空闲时立即提交，积压由解析完成回调继续排空，绘制帧交给 xterm，避免后台 WebView 暂停动画帧时停止消费输出。xterm 拒绝写入时清除 busy 状态并 reject 受影响的待解析批次，让 bridge 记录失败并归还额度，后续输出可继续；不把失败批次当作已成功显示。
- 终端日志颜色属于 renderer 的 xterm 表现层：远端原有 ANSI 继续由 xterm 解析，纯文本日志只在普通缓冲区完成解析后按主题色补充语义颜色；备用屏幕程序保持原始输出，避免破坏 TUI。可选着色的写入后处理异常会禁用当前终端后续写入的语义着色并只记一次 WARN，始终继续解析完成回调，避免中断 xterm 的内部排空循环。
- Tauri 终端输出使用持久的 IPC `Channel`，所有协议的显示输出按 UTF-8 边界分成至多 16 KiB 的批次，每窗口最多 4 批待确认；renderer 在 xterm 解析完成后经 `app_ack_terminal_data` 归还额度。通道按原生窗口绑定，每窗口独立转发；每标签最多 8 批待发送（最多 128 KiB），每窗口最多 128 批（最多 2 MiB），防止单个刷屏标签占满窗口待发送容量。转发取得额度后按就绪标签轮转取批次，各标签内部保持顺序；同一标签的新输出可合入最后一个尚未发送的批次，不跨显示丢失点合并，也不为凑批增加等待。关闭标签清理其缓存与计数，SSH pump 的迟到排空在标签已移除时补做清理。慢窗口不阻塞其他窗口或协议 IO；重订阅和销毁时取消旧转发、清空旧缓存并释放等待者，入队在队列锁内检查关闭状态。SSH pump 持续读取共享连接，饱和时沿用显示跳过策略并限频告警；串口/Telnet 也使用非阻塞窗口队列，避免暂停设备 IO。原生 transcript 与已启用的会话日志在窗口显示入队前处理。过载时屏幕可能缺少输出，Ctrl+L 是否能重绘由远端决定；普通 Tauri events 只承载低频状态和 snapshot，避免持续输出与状态广播争用同一事件路径。
- SSH transcript 由 controller 的有界分块缓冲统一维护，runtime 不重复拼接第二份历史。
- 系统指标首次绑定时发送完整历史，稳定轮询只发送最新样本，由 renderer 追加到固定长度历史。
- 同一 Renderer 的并发完整快照采用单飞和尾随合并，确保最终状态可达，同时避免重复磁盘读取与大对象序列化。
- 每分钟 `renderer-health` 在原有 IPC 日志中附带 Rust 活跃任务数、全局就绪队列深度、worker 命令队列占用、传输运行数和输出积压。只采集数量，不采集会话内容；使用非等待锁，繁忙时记 `null`，不会等待业务锁或额外启动周期任务。全局就绪队列深度不等于所有等待 IO 的任务数。

文件编辑器的 Monaco 资源通过动态 import 独立分块，只在文件编辑器窗口需要时加载，主工作区不静态携带编辑器与语言服务代码。

### 9.2 可恢复传输边界

- Rust transfer run 在目录扫描、上传/下载、校验和替换目标期间持有共享系统防睡租约。`services/transfers/power.rs` 使用 `keepawake` 的原生电源请求：Windows `SetThreadExecutionState`、macOS IOPM assertion、Linux logind idle inhibitor。创建和释放均在同一专用线程上执行；最后一个活动任务退出（包括暂停、取消、失败、panic 与应用退出收口）后释放。仅抑制空闲自动睡眠，允许屏幕关闭，尊重用户显式睡眠；申请失败记日志并继续传输。手测步骤见 [传输防睡回归](./quality/transfer-power-regression.md)。
- 独立“文件传输”本地标签提供本地/远端双栏，浏览命令不修改终端会话目录。服务器间复制采用 `direction=copy`，目标沿用任务的 `profileId/tabId`，来源保存在 `copySource`；普通文件面板也支持跨会话复制粘贴，禁止跨会话剪切。
- 跨服务器文件流经过 Rust 的有界 duplex 管道，不落地本机完整文件；SSH/SFTP 与 FTP/FTPS 各自维护独立传输通道。复制任务复用 transfer journal、暂停/恢复、目标临时文件与防睡租约；恢复先验证两端配置与源/目标前缀，提交前回读两端 SHA-256。初版只支持普通用户文件通道，目录发现符号链接时停止，默认跳过同名文件，覆盖需显式确认。两端连接实际使用的路由指纹变化时拒绝恢复或清理；相同端点参与的复制任务按固定顺序排队，等待可取消。跨端最终提交拒绝链接与截断回写，使用重命名与备份回滚。文件传输页的本地上传/下载显式持久化冲突策略，在后端逐文件提交时执行；不带策略的旧调用保持原行为。含复制任务或显式策略的 journal 写 v2，仍可读取 v1，旧客户端不会忽略这些安全字段。详见 [跨服务器传输回归](./quality/remote-to-remote-transfer.md)。
- 单文件和目录 manifest 任务由 Rust backend 持久化到当前存储根目录的 `transfer-journal.json`（便携版位于 exe 旁的 `config`）；renderer 不直接读写 journal。
- 上传和下载都先写入 `.fileterm-part` 临时文件，校验大小后再替换正式目标。
- 上传使用任务记录的固定源长度，读取数据块时累计 SHA-256，远端校验与该结果比较，不在上传后重读原文件，也不创建本地副本。SFTP/FTP 续传先核对本地和远端已有前缀的长度与 hash，再追加后缀；FTP 从零回退时重新初始化 hash。旧 root exec 回退路径恢复时必须取得远端完整 SHA-256，否则拒绝提交。原有源身份变化检查保留；固定长度不提供原地改写期间的原子快照语义。
- 普通 SFTP 下载按远端已知大小限制每次读取，读满后不额外请求 EOF；提前 EOF、缺失大小和读写/flush 超时会失败并保留断点。可选 SSH exec SHA-256 仅在收尾时已确认支持该能力的普通会话上执行，能力未知或不可用会记录跳过，仍校验文件大小；root 校验要求保持不变。大小校验不保证内容一致，也不提供远端并发改写时的原子快照。阶段日志见 [本地诊断日志](./quality/logging.md)。
- SFTP 与 FTP/FTPS 分别在 controller 内实现 offset 读写和远端收尾，不把协议命令伪统一到 renderer 或 transfer UI。
- 传输调度使用 `profileId` 作为跨重启身份，不依赖生命周期短暂的 `tabId` 恢复连接；所有中断任务都等待用户手动继续，root 任务继续前还需要恢复 root 授权。
- 高频字节进度仍只走 `transfer:update`，UI 投影直接构造且不深拷贝 manifest。目录条目按索引同步内存状态，journal 在任务创建、状态切换、目录检查点和收尾时更新；序列化与 fsync 在 blocking pool 中持有写锁完成，恢复 offset 以实际临时文件大小为准。
- 传输历史在运行内存、renderer 和 journal 使用同一 200 项保留规则；只淘汰不可恢复且无待清理数据的已结束任务，未完成/可恢复/待清理工作全部保留，因此必要任务超过 200 时不强行裁剪。已结束且无需恢复或清理的目录任务释放 manifest 文件/目录数组；首次快照等待期间，传输事件按任务合并为最新状态。
- 普通断线和暂停保留临时文件；只有显式丢弃才清理断点。
- 本地最终替换采用可回滚的备份重命名。Windows 文件占用导致替换失败时保留 `.fileterm-part`，避免丢失已传数据。
- 目录任务持久化逐文件 manifest：已完成文件经过目标大小复核后跳过，当前文件按真实 `.fileterm-part` 长度继续。
- SFTP root 上传为每个任务同时持久化不可预测的 /var/tmp staging 路径和目标同目录 .fileterm-part：登录用户的 SFTP 通道写入 staging（始终保存源文件从 0 开始的连续前缀），完成校验后由 sudo/su 用短命令复制为目标 partial，再校验并替换正式文件。失败恢复优先检查 staging，若 staging 已提交则改查目标 partial。历史 /tmp staging 任务按原路径恢复。普通 SFTP/FTP/FTPS 直接写目标同目录断点。
- FTP 上传优先使用 `APPE`，服务器不支持时回退 `REST + STOR`；回退结果不安全时删除断点并从零重传，避免拼接出等长但错误的文件。
- FTP 安全模式明确区分未加密 FTP、显式 FTPS 和隐式 FTPS。
- FTP/FTPS 登录后必须成功启用二进制模式，保证换行和断点偏移按原始字节处理；传输 ABOR/QUIT 收尾有超时上限。SFTP 上传等待 flush 与 CLOSE 确认后才返回流式 hash，收尾支持取消与超时。
- SFTP 可恢复路径保持有序流式写入。并行绝对 offset 会让文件长度无法证明前缀连续，因此在没有持久化范围位图前不用于断点判断。
- 传输数据通道不得占用目录浏览通道：SSH/SFTP 共享认证 transport 但使用不同 channel，FTP/FTPS 使用独立连接；session 断开时所有传输通道一并终止，由 transfer journal 保留可验证断点。

## 10. UI 结构

### 10.1 桌面主布局

- 左侧：连接列表 / 收藏 / 分组
- 顶部：标签栏
- 主区：工作区视图
- 底部：传输任务面板

### 10.2 SSH/SFTP 工作区

- 上部：终端
- 下部：远程文件面板
- 支持拖动调整比例
- 支持切换终端全屏或文件全屏

### 10.3 FTP 工作区

- 仅文件面板
- 主区域全高使用

## 11. 主题系统

主题系统需要保持一条清晰链路：

```txt
tokens -> theme vars -> component skins -> terminal colors
```

约束：

- 新视觉能力先进入 `styles/themes/` 的 token 层，再进入对应 feature 的组件皮肤；`component-skins.css` 只承载内置主题的历史兼容覆盖。
- 终端、标签页、按钮、表格、面板这类共用外观，先判断是否需要补 theme vars。
- 不要把颜色、阴影、圆角散落在业务组件里。
- 终端配色从 CSS 变量读取，避免和全局主题脱节。

## 12. 存储设计

### 12.1 本地持久化对象

- 连接配置
- 分组
- 最近连接
- UI 设置
- 终端设置
- 传输历史

### 12.2 路径收藏

文件面板的路径收藏由 `PathBookmark` 共享类型、Tauri bridge 与 Rust `commands/path_bookmarks.rs` 管理，持久化到 `path-bookmarks.json`。本地收藏共享，远程按连接 profile ID 隔离；变更串行执行并原子写入。打开收藏复用已有文件操作链路，错误不触发自动删除。

### 12.3 敏感信息

当前策略：

- 连接配置保存在 `profiles.json`；SSH profile 只保存 `privateKeyId`。导入的私钥原文存放在 `ssh-keys/{id}.key`，元数据在 `ssh-keys.json`，可选口令在 `ssh-key-secrets.json`。WebDAV/S3 的密码加密备份会包含该密钥库；恢复时在本机加密保存密钥口令，并以合并方式保留本机独有密钥。
- 产品定位偏向个人/小团队的本地桌面工具，依赖操作系统对应用用户数据目录的权限隔离；不额外引入 `safeStorage`、系统钥匙串或密文存储层，避免跨平台弹窗和钥匙串依赖破坏独立运行体验。
- 该策略是有意为之，不作为后续安全专项规划。如果未来产品定位转向多用户/企业场景，再重新评估存储安全模型，届时单独出 ADR 决策。

## 13. 状态管理

当前状态由 React state、7 个领域 hooks、workspace service snapshot 与 IPC 广播驱动。`app.tsx` 和 workspace service 的领域边界拆分完成后已重新评估 Zustand，结论是现有 hooks 编排足以满足当前共享范围，不引入额外 store。详见 [ADR-0004](./decisions/0004-retain-domain-hooks-without-zustand.md)。

只有在出现多个非父子 feature 需要直接订阅同一份高频可变状态、且现有 props/IPC snapshot 明显造成重复状态或性能问题时，才重新评估 store。届时按领域拆 store，而不是一个全局超级 store：

- `useWorkspaceStore`
- `useProfilesStore`
- `useSessionStore`
- `useTransferStore`
- `useSettingsStore`

避免重走旧桌面项目常见的“大 store 全塞”路线。

## 14. 当前重构热点

上一轮重构热点（工作区、会话控制与 app.tsx 职责混合）已结束。拆分结果：

- `apps/tauri/src-tauri/src/services/workspace/mod.rs`：工作区状态、会话、传输与跨窗口广播在 Rust backend 收口。
- `apps/tauri/src-tauri/src/sessions/`：SSH、FTP/FTPS、Telnet、Serial 保持物理隔离。
- `apps/tauri/src/renderer/app.tsx`：Tauri 专用工作区入口；不依赖历史 renderer 组件或 hooks。

当前阶段的关注重点已转移：

- **存储策略已明确**：连接配置与密码仍采用文件型明文存储；SSH 私钥库则将 profile 引用、私钥原文和可选口令分离到独立文件，但不引入 `safeStorage`、系统钥匙串或密文存储层。详见第 12 节。该策略是有意为之，非待办债务。
- **系统指标多平台覆盖**：Tauri 位于 `apps/tauri/src-tauri/src/sessions/system_metrics/mod.rs`，由 exec、parser 和平台 command builder 片段组成，以 Rust service 测试和三平台 CI 验收。
- **Rust/Tauri 测试**：以 Rust unit/integration/contract test、协议夹具和发行候选清单为准；历史 controller 测试不再是门禁。
- **renderer 组件测试**：Renderer 逻辑测试、浏览器验证和构建检查分开运行，入口与夹具归属见[测试目录](../tests/README.md)。

建议进入小步迭代阶段：不再做大规模结构拆分，而是围绕安全专项、平台扩展和测试厚度持续巩固。

## 15. 当前阶段不做的内容

这些能力不进入当前远程工作区协议范围：

- RDP
- VNC
- 自动化脚本
- 团队同步
- AI 助手
- 手机端正式布局适配

另外，SCP 不单独建 controller：SSH/SFTP 已覆盖当前文件工作流。WebDAV、S3/对象存储和 SMB 也暂不作为远程会话协议接入；它们与 SSH/SFTP/FTP 的权限、目录、传输和认证模型差异较大，现阶段接入会增加大量“看似通用、实际不一致”的空能力。仓库中已有的 WebDAV/S3 能力属于配置备份/同步边界，SMB 只属于本地文件提供方，不等同于远程会话协议。

## 16. 实施原则

- 所有新功能先落抽象，再接 UI
- IPC 必须有类型定义
- 不允许 renderer 直接操作协议客户端
- 文件传输必须走统一任务中心
- 错误处理必须以用户可读提示为目标
- 优先把 macOS 与 Windows 体验做顺
- 视觉样式优先遵循主题系统链路

### 本地诊断日志模块

Rust 诊断日志集中在 `services/logging/`，由 `mod.rs` 保留稳定 facade；tracing subscriber/layer 接收结构化事件和 span，级别和分类、脱敏格式化、队列写入与轮转、panic 捕获分别维护。
Windows 的 `windows_performance/` 保留独立原生进程健康采样；启动时仅回收通过名称、GUID、PID 和进程创建时间验证的孤儿会话，ETW 采集只由既有卡顿告警触发。短时 headless helper 不初始化桌面 runtime，设置时间、事件量、消费线程/整个 helper CPU 与报告体积限制；身份或清理不确定时不启新采集。日志区分线程调度驻留与实际 CPU 执行能力，明确未校正 DPC/ISR，不持久化原始系统事件，不改变绑核或业务线程策略。
Windows 主窗口及子窗口在创建时登记 WebView2 浏览器 PID 和进程创建时间，窗口销毁时移除；原生分钟采样据此补充父子进程树，普通内存查询仅申请有限查询权限。登记/查询失败与截断明确记录，不以零内存替代未知值，也不依赖 renderer 心跳或管理员权限。
Renderer 使用 `lib/diagnostic-log.ts` 经已有 IPC 写入本地日志，不直接访问文件。
日志增加统一 category，保留原 scope 与标签 ID；所有级别共用 1,024 项有界队列和一个专用线程，接纳时为 WARN/ERROR 预留容量。满载或故障不创建逐条兜底任务、不在业务线程写盘；丢弃和写入失败按级别累计，由写盘线程限频汇总，正常退出进行有界排空。
完整分类、限流、保留和故障边界见 [本地诊断日志](./quality/logging.md)。
