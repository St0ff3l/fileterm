# 连接协议与 SSH 高级能力扩展计划

> 2026-07-13 实施更新：本计划的核心交付已落地。SSH MFA、SOCKS5/HTTP 代理、Telnet、Serial、SSH Config 导入、Jump Host、运行时 SSH 隧道、两阶段 JSON 导入/单文件兼容导出，以及 WebDAV 手动同步均已接通。当前文档保留为实现与验收记录；后续仅补真实设备和真实 WebDAV 服务的手工验收结果。

> 范围说明（2026-07-16）：Electron 是本计划的基线实现；Rust/Tauri 对齐进度以 `tauri-migration-progress.md` 为准。Tauri 已完成 SSH 代理、Jump Host 和运行时 `-L/-R/-D` 隧道，并补齐 workspace capability 快照，使 renderer 可显示并操作隧道面板；剩余工作以真实服务、实体设备和三平台发行验收为主。

## 实施完成摘要

- 用户手动导出的 FileTerm 备份包含连接凭据，用于完整迁移；WebDAV 同步包仍会递归剥离 SSH、私钥和嵌套 `proxy.password`。profile repository 也会把代理密码恢复到 main process，而不写回 `profiles.json`。
- SSH 工作区底部面板支持“文件 / 隧道”切换；可在当前 tab 运行时新增、启动、停止和删除 `-L/-R/-D`，断线或关闭 tab 自动回收 listener 和活动 socket。停止远程 `-R` 时会报告服务端取消失败并保留错误状态以供重试；本地 `-L/-D` 会在关闭 listener 前断开活动客户端，避免端口释放卡住。
- Jump Host 使用自身的认证方式、代理、keyboard-interactive 和 host verification；仅支持单级，循环或自引用会被明确拒绝。
- 单一导入入口支持 SSH 配置和外部 JSON，采用“选择文件或目录 → main-process 解析/秘密隔离 → renderer 预览 → 确认”流程。预览提供勾选、跳过/覆盖/另存为策略；覆盖会保留源文件未提供的既有凭据。
- WebDAV 位于设置页，使用手动上传/下载、HTTPS 默认、20 秒超时、ETag 冲突检测和原子本地配置写入。同步的 portable 包不包含连接 secrets。
- 自动测试包含 17 个代表性外部 JSON、嵌套代理 secret、HTTP CONNECT、Telnet NAWS、SSH tunnel lifecycle、Serial mock 和 WebDAV secret-free upload/download。

## 1. 背景

FileTerm 当前已经稳定覆盖 SSH shell、SFTP、FTP/显式 FTPS/隐式 FTPS、远程文件编辑、传输中心、断点续传、CWD 跟随和远端系统指标。连接管理器的主要短板集中在传统网络设备、受限网络和多跳运维场景：

- 不能保存或打开 Telnet 会话。
- 不能打开串口会话（Windows `COM*`、Linux/macOS `/dev/*`）。
- SSH 代理、端口转发、Jump Host 尚未落地。
- 没有 `~/.ssh/config` 导入。
- SSH keyboard-interactive 目前只有密码自动回填式兼容，没有显式认证模式和多提示交互。
- 没有跨设备的连接配置同步。

本计划将这些能力纳入 FileTerm 的现有分层，不直接照搬其他桌面客户端的实现。FileTerm 继续遵守：

```txt
packages/core
  -> main services / session controllers
    -> IPC + preload
      -> renderer connection manager / workspace
```

Renderer 不直接访问 socket、串口、SSH client 或 WebDAV client；所有系统和网络能力都从 main process 经过类型化 IPC 暴露。

## 2. 目标与非目标

### 目标

1. 让连接 profile 能明确表达 SSH、Telnet、Serial、FTP 的差异，而不是在 SSH profile 上继续堆字段。
2. 支持 SSH 密码、私钥、加密私钥、SSH Agent、keyboard-interactive/MFA、代理、端口转发和 Jump Host。
3. 为 Telnet 与 Serial 复用现有终端工作区，但不伪造 SFTP、远程资源监控或 SSH 专属能力。
4. 支持从 `~/.ssh/config` 安全导入 SSH profile。
5. 提供可选的 WebDAV 连接配置同步，同时保留本地文件/系统同步作为更简单的替代方案。
6. 保证 Windows、macOS、Linux 的路径、串口、窗口和凭据边界清晰。
7. 为每个新增 controller 和关键协议边界建立可自动化回归的测试夹具。

### 非目标

- 不在本计划中加入 RDP、VNC、Mosh、HTTP 文件服务或通用插件协议。
- 不把 Telnet/Serial 强行接入 SSH 的 SFTP、远程系统指标、sudo/root 和 CWD 模型。
- 不把代理密码或 WebDAV 密码直接放进 renderer 状态快照。
- 不默认启用云同步，不因为增加 WebDAV 就改变现有本地 profile 的存储位置。
- 不在第一阶段引入 Zustand、数据库或新的全局状态框架。

## 3. 当前基线与缺口

### 已有能力（保持不回归）

- SSH 密码、私钥、加密私钥 passphrase、SSH Agent/默认私钥。
- SSH 主机指纹确认与保存。
- SSH shell、SFTP、远程文件操作和独立 transfer SFTP 通道。
- FTP、显式 FTPS、隐式 FTPS。
- profile 分组、`group`/`parentId` 双向自愈、连接管理器窗口。
- SSH keyboard-interactive 的密码自动回复兼容路径。
- 主机系统指标的 Linux、BusyBox/OpenWrt、Windows 路由。

### 需要新增或增强的能力

| 能力                           | 当前状态                     | 计划结论                                   |
| ------------------------------ | ---------------------------- | ------------------------------------------ |
| SSH keyboard-interactive / MFA | 有密码自动回填，不可显式选择 | 补成独立认证模式和逐提示交互               |
| SOCKS5 / HTTP 代理             | 连接表单只有占位入口         | 新增通用 outbound proxy 配置与 socket 工厂 |
| SSH `-L/-R/-D`                 | 连接表单只有占位入口         | 新增持久化规则、运行时面板和生命周期管理   |
| `~/.ssh/config` 导入           | 未实现                       | 新增安全 parser、预览和去重导入            |
| Telnet                         | 未实现                       | 新增 TCP Telnet controller                 |
| Serial                         | 未实现                       | 新增跨平台串口 controller                  |
| Jump Host                      | 未实现                       | 新增单级 SSH ProxyJump                     |
| WebDAV 配置同步                | 未实现                       | 作为可选、显式触发的配置同步功能           |

## 4. 总体架构

### 4.1 Profile 类型扩展

当前 `SessionType = 'ssh' | 'ftp'`，第一步扩展为：

```ts
export type SessionType = 'ssh' | 'ftp' | 'telnet' | 'serial'
```

建议保持 discriminated union，不把 Telnet/Serial 的字段塞入 `SshProfile`：

```ts
interface TelnetProfile extends BaseProfile {
  type: 'telnet'
  proxy?: ProxyConfig
  encoding?: string
}

interface SerialProfile extends BaseProfile {
  type: 'serial'
  devicePath: string
  baudRate: number
  dataBits: 5 | 6 | 7 | 8
  stopBits: 1 | 2
  parity: 'none' | 'odd' | 'even' | 'mark' | 'space'
  flowControl: 'none' | 'hardware' | 'software'
  encoding?: string
}
```

SSH profile 增加或明确以下字段：

```ts
type SshAuthType = 'password' | 'privateKey' | 'system' | 'keyboard-interactive'

interface SshProfileFields {
  proxy?: ProxyConfig
  jumpProfileId?: string
  forwards?: SshForwardRule[]
  disableShellIntegration?: boolean
}
```

所有新增字段必须使用向后兼容默认值。旧 profile 缺少 `type` 时继续视为 SSH；旧 profile 缺少 `proxy`、`forwards`、`jumpProfileId` 时视为空配置。

### 4.2 通用代理模型

```ts
interface ProxyConfig {
  type: 'none' | 'socks5' | 'http'
  host: string
  port: number
  username?: string
  password?: string
}
```

代理密码属于 secret 字段，不能进入普通 `profiles.json` 或 renderer 的公开 snapshot。代理 socket 创建放在 main 的网络基础设施中，SSH、Telnet 和未来需要代理的协议通过同一工厂获取连接。

第一版范围：

- SOCKS5 CONNECT。
- HTTP CONNECT。
- 可选用户名/密码认证。
- 直连、环境变量代理和显式 profile 代理的优先级必须写入实现文档。
- 不实现 PAC、系统代理自动发现和代理链。

### 4.3 Session Controller 接口

现有 SSH/FTP controller 保持分离；新增：

```ts
interface TerminalSessionController {
  connect(): Promise<void>
  disconnect(): Promise<void>
  write(data: string): Promise<void>
  resize(cols: number, rows: number, width: number, height: number): Promise<void>
  getSummary(): string
}
```

SSH、Telnet、Serial 可以共享终端生命周期接口，但各自实现协议细节：

- SSH：shell channel、SFTP、exec、系统指标、CWD、sudo。
- Telnet：TCP socket、RFC 854 option negotiation、raw terminal bytes。
- Serial：串口 read/write、设备参数和连接关闭。

不要让 `BaseFileSessionController` 成为 Telnet/Serial 的父类；它们没有远程文件能力。

## 5. 分阶段实施

### Phase 0：模型、能力矩阵与兼容迁移

- [ ] 在 `packages/core` 扩展 `SessionType` 和 profile discriminated union。
- [ ] 新增 `ProxyConfig`、`SshForwardRule`、keyboard-interactive 请求/响应类型。
- [ ] 将 profile secret 字段继续从公开 profile 文件中剥离。
- [ ] 为 profile repository 增加旧数据迁移和未知字段保留/丢弃策略。
- [ ] 明确 `sftpEnabled`、`enableExecChannel`、`enableResourceMonitoring` 只对 SSH 生效。
- [ ] 为 connection library snapshot 增加能力字段，供 renderer 决定哪些操作可用。
- [ ] 更新 `docs/architecture.md` 与 `docs/integration-inventory.md` 的连接类型边界。

验收：旧 SSH/FTP profile 可读取、编辑、保存；类型检查通过；保存后不会把 secret 写回普通 profile 文件。

### Phase 1：SSH keyboard-interactive / MFA

#### 目标

把当前“密码自动回复所有提示”的兼容逻辑升级为显式、可观察、可取消的认证流程。

#### Main/controller

- [ ] `SshAuthType` 增加 `keyboard-interactive`。
- [ ] SSH controller 对每个 challenge 暴露 prompt 名称、说明、提示文本和是否为 echo 输入。
- [ ] 第一条普通 password prompt 可复用保存的密码；后续提示不得盲目复用同一密码。
- [ ] MFA/OTP/验证码提示通过 IPC 请求 renderer 输入。
- [ ] 支持取消认证、连接超时、连接关闭时清理 pending request。
- [ ] shell、exec、SFTP、transfer SFTP 四条 SSH 连接复用同一认证策略。
- [ ] 日志只记录 prompt 类型和数量，不记录密码、OTP 或私钥内容。

#### IPC/preload/renderer

- [ ] 扩展 `SshInteractionRequest` 为 `keyboard-interactive` challenge。
- [ ] `preload` 提供 resolve/cancel API，并保证 requestId 一次性消费。
- [ ] 复用 `useSshInteractions`，新增逐提示输入弹窗。
- [ ] 连接表单新增“密码 / 键盘交互 / 私钥 / 系统认证”选项。

#### 验收

- [ ] password-only SSH 正常登录。
- [ ] keyboard-interactive 仅密码提示正常登录。
- [ ] password + OTP 两步认证能分别输入。
- [ ] SFTP 使用同一认证策略。
- [ ] 取消 OTP 后连接干净关闭，无残留弹窗或 pending promise。

### Phase 2：SOCKS5 / HTTP 代理

- [ ] 在 main 新增 `proxy-socket-factory`，提供 direct、SOCKS5、HTTP CONNECT 三种连接路径。
- [ ] SSH controller 的主连接、exec、SFTP、transfer SFTP 全部使用同一代理配置。
- [ ] Telnet controller 复用 SOCKS5/HTTP 代理。
- [ ] 代理认证 secret 进入独立 secret storage。
- [ ] 连接表单完成代理类型、地址、端口和可选认证字段。
- [ ] 连接状态错误区分“代理连接失败”和“目标主机连接失败”。
- [ ] 代理 socket 设置连接超时、关闭传播和取消处理。
- [ ] 增加 IPv4、IPv6、域名、代理认证失败和代理断开测试。

明确不把代理逻辑放进 renderer，也不在各 controller 中复制 SOCKS/HTTP 握手代码。

### Phase 3：SSH `-L/-R/-D` 隧道

#### Profile 与持久化

```ts
interface SshForwardRule {
  id: string
  name?: string
  kind: 'local' | 'remote' | 'dynamic'
  bindHost: string
  bindPort: number
  targetHost?: string
  targetPort?: number
  autoStart: boolean
}
```

- [ ] 规则只挂在 SSH profile 下。
- [ ] 校验端口、地址和 `local/remote/dynamic` 必填关系。
- [ ] `dynamic` 规则不需要目标 host/port。
- [ ] 兼容旧的未命名规则，自动生成稳定显示名称但不改写用户数据。

#### Runtime

- [ ] 在 main 新增 `SshTunnelService`，管理每个 tab/profile 的运行时隧道。
- [ ] SSH 连接成功后按 `autoStart` 启动规则。
- [ ] 支持运行时新增、启动、停止、删除，不自动覆盖已保存 profile。
- [ ] `-L` 使用 SSH direct-tcpip；`-R` 使用 remote forward；`-D` 提供本地 SOCKS5 listener。
- [ ] SSH 断线时停止或标记隧道，重连后按策略恢复。
- [ ] 关闭 tab/profile 时等待 listener 和 SSH channel 完整释放。

#### UI/IPC

- [ ] `packages/core` 增加 tunnel runtime snapshot 和事件类型。
- [ ] preload 暴露 list/start/stop/create/delete tunnel API。
- [ ] SSH 文件面板增加“文件 / 隧道”切换，或建立独立隧道面板。
- [ ] 显示监听地址、目标地址、状态、错误和所属 tab。

#### 验收

- [ ] `-L` 可访问远端 HTTP/SSH 服务。
- [ ] `-R` 可从远端访问本地暴露服务。
- [ ] `-D` 可被 SOCKS5 客户端使用。
- [ ] 隧道能在断线、重连、关闭 tab 时正确回收。
- [ ] 不影响已有 SFTP transfer。

### Phase 4：`~/.ssh/config` 导入

- [ ] 新增 main-only parser，不执行 shell、不展开命令、不读取未声明的敏感文件。
- [ ] 第一版支持：`Host`、`HostName`、`User`、`Port`、`IdentityFile`、`ProxyJump`（可先作为待处理提示）。
- [ ] 跳过 wildcard host、缺失 HostName、重复 host 和非法端口。
- [ ] 对 IdentityFile 做 `~` 展开，但不把私钥内容读入 renderer。
- [ ] 导入前返回 preview：名称、host、port、user、key path、冲突原因。
- [ ] 用户确认后批量创建 profile，默认分组可选。
- [ ] 记录导入结果：新增、跳过、失败数量和原因。
- [ ] 支持 Linux/macOS `~/.ssh/config`，Windows 使用用户目录下的 `.ssh/config`。
- [ ] 不支持或无法安全映射的 OpenSSH 指令必须明确显示为“未导入”，不能静默伪造。

### Phase 5：Telnet

#### Profile

- [ ] 新增 `TelnetProfile`，默认端口 23。
- [ ] 支持 host、port、encoding、代理和 note。
- [ ] Telnet 不显示 SSH 用户名、私钥、SFTP、远程指标和端口转发设置。

#### Controller

- [ ] 新增 `telnet-session-controller.ts`。
- [ ] 复用 TCP socket 工厂和终端生命周期接口。
- [ ] 实现 RFC 854 基础 option negotiation：IAC、DO/DONT、WILL/WONT、SB/SE。
- [ ] 至少正确处理 suppress-go-ahead、echo、binary、terminal type/window size 的常见协商。
- [ ] 禁止协议控制字节污染 xterm transcript。
- [ ] 连接关闭、半关闭、超时和代理错误统一转为 session state。

#### UI

- [ ] 连接表单新增 Telnet 类型。
- [ ] Telnet tab 使用 terminal-only layout。
- [ ] 隐藏 SFTP drawer、系统指标、sudo/root、CWD follow 和 SSH 专属设置。

### Phase 6：Serial

#### Profile

- [ ] 新增 `SerialProfile`，默认 115200/8N1/无流控。
- [ ] 支持 COM3、`/dev/ttyUSB0`、`/dev/ttyACM0`、macOS `/dev/cu.*`。
- [ ] 支持波特率、数据位、停止位、校验和流控。
- [ ] 不把串口设备路径当作远端 host 做网络校验。

#### Controller

- [ ] 新增 `serial-session-controller.ts`。
- [ ] 选择跨平台串口库，并把设备打开/读取放在 main process。
- [ ] 读写使用背压和取消，关闭时释放设备句柄。
- [ ] 设备拔出、占用、权限不足、参数不支持必须给出可读错误。
- [ ] Serial 不创建 SFTP、exec、系统指标或 CWD runtime。

#### 平台验收

- [ ] Windows：COM1、COM10 以上设备路径、设备拔出和驱动错误。
- [ ] macOS：`/dev/cu.*` 与 `/dev/tty.*`，权限和设备消失。
- [ ] Linux：`/dev/ttyUSB*`、`/dev/ttyACM*`，dialout 权限提示。
- [ ] 高 DPI 下串口参数表单不发生布局溢出。

### Phase 7：SSH Jump Host / Bastion

- [ ] SSH profile 增加可选 `jumpProfileId`。
- [ ] 只允许引用另一个 SSH profile，拒绝自引用、循环引用和已删除引用。
- [ ] 第一版只支持单级跳板，不做多级递归。
- [ ] 先连接跳板并完成其认证，再通过 direct-tcpip channel 建立目标 SSH handshake。
- [ ] shell、exec、SFTP、transfer SFTP 均通过跳板。
- [ ] 跳板 profile 的 proxy、私钥、keyboard-interactive 策略独立生效。
- [ ] 目标 profile 的 host key 仍校验目标主机，而不是跳板主机。
- [ ] 跳板断开时目标会话和关联隧道进入明确的断开状态。
- [ ] UI 只在 SSH 高级设置显示 Jump Host 下拉框。

### Phase 8：外部连接配置 JSON 导入/导出

#### 背景

现有目录中有一组 `*_connect_config.json` 样例文件。它们采用“一个文件对应一个连接”的扁平 JSON 结构，文件名通常包含连接名称。已检查的 17 个样例均为有效 JSON，主要字段包括：

- 连接标识与展示：`id`、`name`、`description`、`parent_id`、排序和时间字段。
- 连接地址：`host`、`port`、`user_name`。
- 认证：`authentication_type`、`password`、`secret_key_id`。
- 终端：`terminal_encoding`、删除键/退格键序列、窗口尺寸。
- SSH 行为：`exec_channel_enable`、`forwarding_auto_reconnect`、`port_forwarding_list`、`remote_port_forwarding`。
- 兼容字段：`conection_type`、`proxy_id`、`drivestoredirect`、`accelerate` 等。

当前样例全部表现为 SSH 连接，绝大多数端口为 22，也包含自定义端口；样例中的 `port_forwarding_list` 当前为空。导入器不能依赖文件名或固定端口判断协议，必须以字段映射和用户预览为准。

#### 目标

- [ ] 支持选择单个 `*_connect_config.json` 导入。
- [ ] 支持选择目录并批量导入多个连接文件。
- [ ] 支持将一个连接导出为单文件 JSON。
- [ ] 支持将选中的多个连接导出为目录中的多个 JSON 文件，文件名安全处理并避免覆盖。
- [ ] 保留 FileTerm 的原生 profile 格式作为默认导出格式。
- [ ] 增加“兼容连接 JSON”导出格式，便于与现有样例文件互相迁移。
- [ ] 导入时显示预览、字段映射、重复项和不支持字段。
- [ ] 导入失败时不写入半条 profile，不影响已经存在的连接。

#### 外部字段映射

| 外部字段                 | FileTerm 字段            | 处理方式                                           |
| ------------------------ | ------------------------ | -------------------------------------------------- |
| `name`                   | `name`                   | 直接映射；为空时使用文件名                         |
| `description`            | `note`                   | 直接映射                                           |
| `host`                   | `host`                   | 规范化并校验域名/IP/IPv6                           |
| `port`                   | `port`                   | 校验 1–65535；不使用默认端口猜测协议               |
| `user_name`              | `username`               | 直接映射                                           |
| `terminal_encoding`      | `encoding`               | 映射 UTF-8、GBK 等已支持编码                       |
| `password`               | secret password          | 只写入 secret storage，不进入公开 profile 文件     |
| `authentication_type`    | `authType`               | 建立版本化枚举映射，未知值进入人工确认             |
| `conection_type`         | `type`                   | 保留原始值并通过映射表转换，不能因拼写错误静默丢失 |
| `exec_channel_enable`    | `enableExecChannel`      | 映射布尔值                                         |
| `port_forwarding_list`   | `forwards`               | 解析后校验每条规则；未知结构单独提示               |
| `remote_port_forwarding` | runtime/forward metadata | 仅在结构可识别时导入，否则作为未支持字段报告       |
| `parent_id`              | `parentId`/group         | 不直接复用外部 ID；按导入批次重建文件夹关系        |
| `proxy_id`               | `proxy`                  | 仅作为外部引用提示，不能假设本机存在对应代理配置   |

以下字段属于其他客户端的布局、排序、缓存或加速状态，默认不导入：`height`、`width`、`fullscreen`、`custom_size`、`sort_time`、`access_time`、`create_time`、`modified_time`、`delete_time`、`rename_time`、`parent_update_time`、`order`、`accelerate`、`drivestoredirect`。预览中应显示“已忽略”，方便用户确认没有静默丢数据。

#### Secret 与安全

- [ ] 导入前识别 `password`、私钥路径、代理密码等敏感字段。
- [ ] 不在导入预览、日志、错误信息和导出文件名中显示密码或 token。
- [ ] 外部文件中的密码不得原样写入 `profiles.json`；必须进入现有 secret storage。
- [ ] 对外部文件权限较宽的情况给出提示，不因为权限问题直接泄漏内容。
- [ ] 导出默认提供“仅连接信息”和“包含凭据”两个选项，默认不包含凭据。
- [ ] 包含凭据时必须使用用户明确设置的导出密码进行加密，禁止明文批量导出。
- [ ] 导入加密导出包时先在内存中解密并校验 schema，再原子写入。

#### 导入流程

```txt
选择文件/目录
  -> 读取并限制文件大小
    -> JSON 语法校验
      -> schema 识别与版本转换
        -> 字段映射与 host/port 校验
          -> secret 字段隔离
            -> 重复检测与冲突选择
              -> 用户确认
                -> 原子批量写入 profiles/folders/secrets
```

- [ ] 单文件和批量导入使用同一解析器。
- [ ] 批量导入采用 all-or-nothing 或逐条结果明确的事务策略，不能出现“界面显示失败但部分写入”而没有记录。
- [ ] 重复检测至少比较 `type + host + port + username`；名称相同不能单独作为重复依据。
- [ ] 冲突选项包括跳过、覆盖已有 profile、另存为新连接。
- [ ] 覆盖前保留已有 profile 的 secret，除非用户明确选择替换凭据。
- [ ] 导入结果显示新增、跳过、覆盖、失败和忽略字段数量。
- [ ] 支持中文、空格、括号等文件名；导出时对 Windows/macOS/Linux 保留字符做安全替换。

#### 导出流程

- [ ] 单连接导出文件名默认使用连接名称，并清理路径分隔符、控制字符和保留设备名。
- [ ] 批量导出时处理同名连接，自动追加稳定后缀，不覆盖现有文件除非用户确认。
- [ ] 原生导出包含 schema version、生成时间和 FileTerm 版本，便于未来迁移。
- [ ] 兼容导出只写入外部格式能表达的字段，无法表达的能力进入 `unsupported_fields` 报告，不静默伪造。
- [ ] 导出结果提供摘要，不把完整 JSON 内容写入日志。

#### IPC 与 UI

- [ ] `packages/core` 增加 `ImportConnectionResult`、`ExportConnectionOptions`、`ImportConflict` 等类型。
- [ ] main 新增 connection-config codec/service，负责文件选择后的读取、解析、转换和写入。
- [ ] preload 暴露单文件导入、批量导入、单连接导出、批量导出 API。
- [ ] 连接管理器新增“导入连接”“导出连接”入口。
- [ ] 导入预览支持逐条勾选和冲突策略选择。
- [ ] 导出凭据选项必须明确风险，并默认关闭。
- [ ] 大批量导入使用进度事件，避免阻塞连接管理器窗口。

#### 验收

- [ ] 17 个现有样例均能完成解析，并对未知字段给出报告。
- [ ] SSH 名称、host、端口、用户、编码和可识别转发规则能正确导入。
- [ ] 密码不出现在公开 profile JSON、日志和 UI 预览中。
- [ ] 重复导入不会无提示地产生重复连接。
- [ ] 单条导出后重新导入，连接语义保持一致。
- [ ] 批量导入中混入损坏 JSON 时，其他文件的结果可预测且不会破坏已有数据。
- [ ] Windows、macOS、Linux 的文件名清理和导出路径均可用。

### Phase 9：WebDAV 连接配置同步

#### 定位

WebDAV 不是新的终端协议，也不是远程服务器连接类型。它只是一个基于 HTTP 的可读写配置文件存储位置，用于在多台设备之间同步 FileTerm profiles、folders 和必要的配置元数据。

它是可选能力，不应替代本地 profile 存储。对单机用户，直接使用系统文件同步目录、iCloud Drive、OneDrive、Syncthing 或 NAS 共享通常更简单；WebDAV 的价值主要在于：

- 设备不在同一局域网时仍可通过 HTTPS 访问。
- 不需要先把 WebDAV 挂载成系统盘。
- Nextcloud、ownCloud、NAS 等已有服务可以直接复用。
- Windows、macOS、Linux 使用同一个 HTTP endpoint。

第一版采用“手动上传 / 手动下载 + 冲突确认”，不做后台双向实时同步。

#### 数据与安全

- [ ] 设计 portable export 格式，版本化并带 schema version。
- [ ] secrets 默认继续由本机 secret store 保护，不直接把明文 password 上传。
- [ ] 如果要跨设备恢复密码，必须另设用户提供的同步密码/加密口令，不能复用机器本地密钥。
- [ ] WebDAV URL、用户名和同步口令不进入 renderer workspace snapshot。
- [ ] 默认 HTTPS；允许用户显式开启不验证证书，并在 UI 明确高风险提示。
- [ ] 下载后先写临时文件、校验 JSON/schema，再原子替换本地配置。
- [ ] 上传前保留本地备份，失败时不覆盖本地 profile。
- [ ] 支持 ETag/Last-Modified 做基本冲突检测。
- [ ] 不自动删除远端文件，不默认覆盖本地未同步变更。

#### IPC/UI

- [ ] 设置页增加“连接配置同步”区域，而不是把 WebDAV 当成 SSH/FTP 连接类型。
- [ ] 提供启用、URL、用户名、远端路径、上传、下载、检查冲突和清除本地凭据操作。
- [ ] 显示最近同步时间、版本、冲突和错误原因。
- [ ] 上传/下载均由 main service 执行，renderer 只接收结果。

#### 验收

- [ ] Nextcloud/NAS WebDAV HTTPS endpoint 可上传和下载配置包。
- [ ] 错误证书、401、404、网络超时、ETag 冲突不会破坏本地配置。
- [ ] 明文 secret 不出现在普通导出包，或在用户明确设置同步加密口令后才进入加密包。
- [ ] 两台设备的导入不会覆盖未确认的本地 profile。

## 6. Renderer 与连接管理器设计

### 类型分层

连接表单按类型动态显示字段：

- SSH：认证、主机指纹、代理、Jump Host、隧道、SFTP/exec/指标设置。
- FTP/FTPS：安全模式、账号、远端路径。
- Telnet：host、port、encoding、代理。
- Serial：device path、串口参数、encoding。

不能让不适用的字段显示后再由 main 忽略；这会造成 profile 看似支持但实际不生效。

### Workspace layout

- SSH：terminal + SFTP + system metrics。
- FTP/FTPS：file-only。
- Telnet：terminal-only。
- Serial：terminal-only。

### 平台边界

- renderer 只使用 `window.fileterm.platform`，不通过 `navigator.platform` 猜平台。
- 串口路径枚举、打开和错误处理全部在 main。
- Windows/macOS 标题栏、窗口和托盘改动不能与连接 controller 耦合。
- Telnet/Serial 不触发 SSH 的 POSIX CWD 注入、远端指标采集或 sudo 解析。
- Windows 远端 SSH 仍必须遵守现有 fail-closed POSIX 注入门控。

## 7. 测试与质量门禁

### Core/存储测试

- [ ] 旧 SSH/FTP profile 迁移。
- [ ] Telnet/Serial profile 默认值和字段过滤。
- [ ] 代理 secret 不进入普通 JSON。
- [ ] forward 规则校验、去重、默认值。
- [ ] Jump Host 自引用、循环引用、删除引用。
- [ ] WebDAV export schema 和导入冲突。

### Controller/协议测试

- [ ] keyboard-interactive 多 prompt、取消、超时、SFTP 复用。
- [ ] SOCKS5 CONNECT、HTTP CONNECT、代理认证和断开。
- [ ] SSH `-L/-R/-D` 建立、停止、重连、清理。
- [ ] Telnet RFC 854 协商和控制字节过滤。
- [ ] Serial mock read/write、参数错误、拔出和关闭。
- [ ] Jump Host 目标 host key、目标 shell/SFTP、跳板断开。
- [ ] WebDAV 401/404/超时/ETag 冲突/临时文件回滚。

### UI/跨平台回归

- [ ] 连接表单按类型切换时不残留 SSH/FTP 字段。
- [ ] Windows 高 DPI 下 SSH、Telnet、Serial 表单和终端布局。
- [ ] macOS 触控板滚动 SSH/Telnet/Serial 终端。
- [ ] Windows COM 设备和 macOS/Linux 设备路径错误提示。
- [ ] 独立连接管理器窗口与主窗口之间的 snapshot/IPC 同步。
- [ ] 关闭 tab、关闭窗口、退出应用时所有 socket、串口句柄、隧道 listener 都释放。

### 必跑门禁

```bash
npm run typecheck
npm run lint --max-warnings=0
npm run format:check
npm run test:electron
npm run test:transfers:protocol -w @fileterm/electron
```

涉及真实 socket、串口或 WebDAV 的测试必须提供 mock/本地 fixture；不能让 CI 依赖公网、真实设备或用户 SSH 配置。

## 8. 风险与决策点

### 风险 1：把所有协议塞进一个 profile

后果是 UI 字段、controller 分支和 session snapshot 越来越难维护。必须使用 discriminated union，并让每种 session 明确 capability。

### 风险 2：代理、Jump Host 与 SFTP 只接通终端

SSH 当前存在 shell、exec、SFTP、transfer 四条连接。高级网络能力必须明确覆盖所有通道，否则终端能用但文件和传输失败。

### 风险 3：WebDAV 同步泄漏凭据

本机加密 key 通常不能直接跨设备解密。第一版宁可只同步非 secret profile，或使用用户明确设置的 portable encryption password，也不要把明文密码放入 WebDAV。

### 风险 4：Telnet/Serial 被误用 SSH 逻辑

这两种会话没有远端文件、系统指标、CWD 和 sudo。应通过 capability flags 控制 UI 和 runtime，而不是依赖 `if (type !== 'ssh')` 的散落判断。

### 风险 5：跨平台设备和窗口回归

串口路径、Windows 高 DPI、macOS trackpad 和 Electron 子窗口必须分别验证。其他桌面技术栈中的修复不能直接复制为 FileTerm 的实现。

## 9. 推荐落地顺序

1. Phase 0：core profile/capability/迁移。
2. Phase 1：keyboard-interactive/MFA。
3. Phase 2：SOCKS5/HTTP 代理。
4. Phase 3：SSH `-L/-R/-D`。
5. Phase 4：`~/.ssh/config` 导入。
6. Phase 5：Telnet。
7. Phase 6：Serial。
8. Phase 7：Jump Host。
9. Phase 8：外部连接配置 JSON 导入/导出。
10. Phase 9：WebDAV 配置同步。

其中 Phase 5 和 Phase 6 可以并行设计，但必须先完成 Phase 0 的 session type/capability 模型；Phase 7 依赖 Phase 1 和 Phase 2；Phase 8 与协议 controller 解耦，适合在协议能力稳定后交付；Phase 9 依赖 Phase 8 的导出 schema 和冲突处理模型。

## 10. 完成标准

本计划完成时，连接管理器应能明确区分并管理：

- SSH：密码、私钥、加密私钥、Agent、keyboard-interactive/MFA、代理、Jump Host、`-L/-R/-D`、SFTP。
- FTP/FTPS：none/explicit/implicit TLS 文件会话。
- Telnet：RFC 854 终端会话、默认 23 端口、代理。
- Serial：COM 与 Unix 设备路径、完整串口参数。
- 配置同步：可选的 WebDAV 手动上传/下载和冲突保护。

所有能力都必须通过 `packages/core -> main -> preload -> renderer` 暴露，并有对应的存储迁移、controller 生命周期测试和 Windows/macOS/Linux 回归检查。
