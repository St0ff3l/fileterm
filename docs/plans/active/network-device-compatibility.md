# 网络设备 SSH 兼容计划

> 状态：Completed（代码目标已完成；没有实体设备和型号级实测）
> 范围：只对齐已有网络设备 SSH 实现中的能力，不扩展为全品牌、全型号支持。

## 1. 范围判断

本计划的目标是把成熟 SSH 网络设备实现中的会话处理、profile 和能力隔离合并到 FileTerm，
不做超出已有实现的网络设备功能。

MVP 必须具备：

- SSH 连接可明确选择“网络设备”模式；
- 网络设备使用 raw PTY，不执行 `sh -c`、POSIX wrapper、CWD marker 或服务器探测脚本；
- 关闭 SFTP 浏览、CWD 跟随、Shell 探测、Shell 集成和资源监控；
- 允许选择 `vt100`、`ansi`、`xterm` 等 terminal type；
- 可选的 SSH Banner 自动识别，在打开额外 channel 之前完成判断；
- 对已开启 legacy SSH 兼容且识别为 Comware 的连接，支持旧设备所需的
  `diffie-hellman-group-exchange-sha1` `1024/1024/8192` 请求范围；
- 主终端与 SFTP、exec、监控等可选能力相互隔离，后者失败不能关闭终端。

这覆盖核心问题：设备登录后保持交互，不会因为 FileTerm 把交换机当成 Linux/Windows
服务器而发送不兼容命令。本计划不追踪品牌或型号特定的额外命令策略。

FileTerm 当前已经有 Telnet 和 Serial 会话；本计划先解决 SSH 网络设备模式，不为了统一
概念而重写现有 Telnet/Serial controller。

## 2. 当前问题

现有 SSH 流程默认远端是完整服务器：

- `apps/tauri/src-tauri/src/sessions/ssh.rs` 固定请求 `xterm-256color`，建立 shell 后还会先
  做平台探测，再决定是否注入 CWD 集成脚本；
- `apps/tauri/src-tauri/src/sessions/system_metrics/mod.rs` 会执行 POSIX/Windows 平台和指标探测；
- SSH 默认开启 exec、资源监控和 SFTP；
- `packages/core` 当前按 `type === 'ssh'` 直接开放文件、监控、Shell integration 等能力；
- `legacyAlgorithms` 只影响 SSH 握手算法，不能表达“这是网络设备，不要执行服务器探测”。

普通 Linux/Windows SSH 的现有行为必须保留，所以不能全局关闭这些能力，只能增加网络设备
模式并在后端做有效能力裁剪。

## 3. 数据模型

在 `packages/core` 的 `SshProfile` 增加向后兼容字段，字段名以实现时最终确定的类型为准：

```ts
type SshDeviceMode = 'auto' | 'server' | 'network-device'

type SshTerminalType = 'xterm-256color' | 'xterm' | 'vt100' | 'vt220' | 'ansi' | 'linux'
```

建议字段：

- `deviceMode?: SshDeviceMode`：旧 profile 缺省按 `server` 处理；
- `terminalType?: SshTerminalType`：服务器默认保持 `xterm-256color`，网络设备默认 `vt100`；
- `networkDeviceVendor?: 'auto' | 'generic' | 'cisco' | 'huawei' | 'h3c-comware' | 'custom'`：
  只作为本地识别提示，不触发厂商命令，也不代表所有型号都已适配。

`getConnectionCapabilities` 不能只看 `profile.type`，还必须看有效的 SSH device mode：

| 能力              | 普通服务器 | 网络设备 |
| ----------------- | ---------- | -------- |
| 交互式终端        | 开启       | 开启     |
| SFTP/文件面板     | 按 profile | 默认关闭 |
| CWD 跟随          | 按现有条件 | 关闭     |
| Shell integration | 按现有条件 | 关闭     |
| 系统指标          | 按现有条件 | 关闭     |
| SSH 隧道          | 保持现状   | 可保留   |

即使旧配置中的 `enableExecChannel`、`enableResourceMonitoring`、`sftpEnabled` 为 true，
网络设备模式下的有效能力仍由 mode 决定，不能只相信 renderer 的旧勾选状态。

网络设备模式只裁剪运行时有效能力，不覆盖用户保存的 SFTP、Exec、监控和 Shell
integration 偏好；切回普通服务器模式时应能恢复原配置。

## 4. SSH 后端行为

连接顺序调整为：

```text
TCP/socket
  -> SSH handshake
  -> 解析 device mode/banner
  -> 选择 terminal type 和有效 capabilities
  -> 打开主 shell channel
  -> network-device: 直接进入 raw terminal loop
  -> server: 继续现有 platform/CWD/SFTP/metrics 流程
```

### 4.1 网络设备模式

- 请求 profile 选择的 PTY terminal type，默认 `vt100`；
- 终端输入和输出保持原始交互语义；
- 不发送 `/etc/os-release`、`uname`、CWD setup、指标脚本或通用 exec capability probe；
- 不依赖 shell exit code、命令 marker 或 POSIX shell wrapper 判断命令完成；
- 不初始化 SFTP；
- 不启动 CWD 跟随、Shell integration、资源监控、远程文件探测和 sudo/root 流程；
- resize、keepalive、重连、主机指纹校验和 session log 继续复用现有能力；
- 可选 channel 的错误只更新对应 capability/error，不得关闭主终端。

### 4.2 普通服务器模式

- 保持现有默认行为；
- 继续支持 CWD、SFTP、系统指标和文件区；
- `legacyAlgorithms` 仍只负责 SSH 算法协商，不能与 device mode 混用；
- 旧 profile 不需要迁移即可连接。

### 4.3 自动识别

`auto` 模式只能使用 SSH 握手阶段已经拿到的远端 identification/banner，不能为了识别设备
额外打开 exec channel，也不能等首屏探测失败后再补救。

如果用户在 `auto` 模式明确选择了非 `auto` 的厂商族提示，且 identification 未命中保守
pattern，则把该本地配置视为网络设备模式提示；默认厂商族 `auto` 仍保持未知即普通服务器的
安全 fallback。这个提示只改变连接路径，不会触发任何厂商命令。

识别规则采用真实实现中使用的保守 `detectVendorFromSshVersion` 前缀规则：

| 设备族      | 识别线索示例                                       |
| ----------- | -------------------------------------------------- |
| Cisco       | `Cisco-*`、`CiscoIOS_*`、`CISCO_WLC`               |
| Juniper     | `NetScreen`                                        |
| Huawei      | `-`、`HUAWEI-*`、`HUAWEI_*`、`VRP-*`               |
| H3C/Comware | `H3C-*`、`H3C_*`、`H3C *`、`Comware-*`、`3Com OS*` |
| HPE         | `mpSSH_*`                                          |
| MikroTik    | `ROSSSH`                                           |
| Fortinet    | `FortiSSH_*`                                       |
| Palo Alto   | `PaloAltoNetworks_*` / `PaloAltoNetworks-*`        |
| Zyxel       | `Zyxel SSH*`                                       |
| Ruijie      | `RGOS_SSH`                                         |

自动识别未知时：

- 不把未知设备强行判定为 Linux；
- 用户手动选择 `network-device` 时优先级最高；
- `OpenSSH_*`、`Dropbear` 以及没有明确设备标记的 JUNOS/NX-OS 保持未知；
- 不因为品牌字段填写了某个值就自动发送厂商命令。

## 5. UI 与诊断

SSH 连接设置新增：

- 连接对象：普通服务器 / 网络设备 / 自动识别；
- terminal type：`vt100`、`vt220`、`ansi`、`xterm`、`xterm-256color`；
- 可选厂商族：自动 / 通用 / Cisco / Huawei / H3C/Comware / 自定义。

选择网络设备后，文件、系统监控、CWD 和 Shell integration 选项隐藏或置灰，并说明：
“网络设备通常没有 Linux/Windows shell，FileTerm 不会发送服务器探测命令。”

日志按阶段记录：

```text
socket connected
ssh handshake completed
device mode resolved
pty requested
shell ready
optional capability skipped/failed
```

日志不能包含密码、OTP、私钥、完整 host fingerprint 或完整终端内容。错误需要明确区分
TCP、SSH 握手、认证、PTY、shell 和可选 channel 失败。

## 6. 实施阶段

### Phase 1：手动网络设备模式

- [x] 在 `packages/core` 增加 device mode 和 SSH terminal type。
- [x] 更新 profile 默认值、迁移逻辑、`CreateProfileInput` 和 workspace capabilities。
- [x] Rust 在主 shell 前确定有效 mode；network-device 跳过所有服务器探测和 SFTP 初始化。
- [x] PTY 使用 profile terminal type；网络设备命令路径不使用 POSIX wrapper。
- [x] renderer 增加连接对象和 terminal type；隐藏或置灰不适用能力。
- [x] 补 H3C/Huawei mock 策略和协议行为，确认 network-device 保留主终端能力。

> 2026-08-28 实施记录：手动网络设备模式已落地。H3C/Huawei 已完成策略级和 russh 协议级 mock 覆盖（强制关闭 exec、资源监控和 SFTP，终端使用 raw PTY surface，并验证输入输出、PTY terminal type、resize 与可选 channel 拒绝不会影响主终端）；自动识别、终端类型选择和 capability 裁剪在 Phase 2 完成。运行时能力裁剪不会覆盖已保存的连接偏好，网络设备 / 普通服务器模式切换可保留原 SFTP、Exec 和监控设置。

### Phase 2：Banner 自动识别

- [x] 在握手阶段提取并规范化远端 identification。
- [x] 增加已覆盖的 Cisco、Juniper、Huawei、H3C/Comware、HPE、MikroTik、Fortinet、
      Palo Alto、Zyxel、Ruijie pattern 和单元测试。
- [x] 自动识别结果在任何 exec、CWD、指标或 SFTP 探测之前生效。
- [x] 手动 mode 覆盖自动结果；未知设备保持安全 fallback。

> 2026-08-28 实施记录：`ClientHandler::kex_done` 保存 russh 的远端 SSH identification，解析器按保守前缀规则匹配 Cisco、Juniper、Huawei、H3C/Comware、HPE、MikroTik、Fortinet、Palo Alto、Zyxel、Ruijie 线索，并将规范化结果写入运行时有效 profile。解析发生在主 PTY 建立前，因此识别出的网络设备不会进入平台探测、CWD 注入、指标、SFTP 或 exec 路径；`server` 手动模式始终覆盖 Banner，`auto` 未命中且厂商族仍为 `auto` 时按普通服务器兼容路径处理，明确厂商族提示则作为本地 network-device fallback。auto profile 未显式选择 terminal type 时，最终按识别结果使用网络设备 `vt100` 或服务器 `xterm-256color` 默认值。

> 2026-08-29 补充：自动识别支持老 Huawei VRP 的精确短横线
> Banner（`SSH-2.0--`、`SSH-1.99--` 及原始 `-` 形式），并完成 Comware legacy GEX 兼容；
> 匹配保持精确，不把任意未知 Banner 当成网络设备，也不扩大全局弱算法范围。

## 7. 测试与验收

### 自动化

- [x] 普通 Linux/Windows SSH：原有 CWD、SFTP、指标和文件区不回归（既有 OpenSSH fixture 与全量 Tauri 测试通过）。
- [x] network-device：策略路径不发送 `/etc/os-release`、`uname`、CWD marker、metrics script 或 POSIX wrapper。
- [x] H3C/Huawei mock：额外 exec/SFTP 被拒绝时，能力模型和 russh 协议 fixture 保留主 PTY 和 SSH 隧道 surface。
- [x] terminal type：网络设备默认 `vt100`，profile 选择值传给 SSH `request_pty`。
- [x] Banner：已覆盖的设备族在第一个可选 channel 前完成识别。
- [x] 老 Huawei VRP 的短横线 Banner（`SSH-2.0--`、`SSH-1.99--`）在自动模式下识别为网络设备。
- [x] 未知 Banner：手动 network-device 可用，普通 server 不因 Banner 被误分类。
- [x] SFTP、exec、metrics 独立失败时，只关闭对应 capability，不关闭 terminal。
- [x] 旧 profile、snapshot、bridge、renderer 表单和能力矩阵通过 contract/type 测试。
- [x] H3C/Comware 精确 GEX 参数请求（`1024/1024/8192`）：russh 兼容分支覆盖参数选择、
      普通 peer/非 SHA-1 GEX 不受影响，网络设备协议 fixture 覆盖 Comware 握手路径；默认 SSH
      算法不放宽。

> 2026-08-29 自动化验收记录：`npm run typecheck -w @fileterm/tauri`、`npm run lint`、`npx prettier --check apps/tauri packages/core packages/shared packages/storage`、`npm run test:tauri`、`cargo clippy --locked --all-targets --all-features -- -D warnings` 均通过。metrics 后台通道启动或运行失败时只撤销 `resource_monitoring` capability 并广播最新 snapshot，SSH 主终端和隧道保持独立。以上网络设备结论是代码路径、russh 协议 fixture 和策略级 mock 结论；没有实体设备时，不扩大为全品牌支持。

同日使用 Playwright 对本地 renderer 的连接表单做了 Tauri mock smoke：网络设备模式隐藏 Remote Path、Exec、SFTP、监控和服务器凭据入口，保留 Tunnel；厂商族选择、自动识别回退和 `vt100`/`xterm-256color` 终端默认值均通过，说明文字布局也完成截图复核。另用 MemoryProfileRepository 做了保存 round-trip 断言，确认网络设备模式不会覆盖用户原有的 SFTP、Exec、监控和 Shell integration 偏好。该 smoke 不替代实体设备验证。

### 验证边界

当前没有实体设备，因此验收结论只覆盖代码路径、mock 和协议 fixture；不宣称某个品牌或
型号的全面实机兼容。

### 项目门禁

代码阶段完成后执行：

```text
npm run typecheck -w @fileterm/tauri
npm run lint
npx prettier --check apps/tauri packages/core packages/shared packages/storage
npm run test:tauri
cargo clippy --locked --all-targets --all-features -- -D warnings
```

## 8. Definition of Done

本任务的代码完成条件（不依赖实体设备）：

1. 用户能在 SSH profile 中选择网络设备模式和 terminal type。
2. H3C/Huawei 类设备登录后，FileTerm 不发送 Linux/Windows 探测命令，终端保持可交互。
3. SFTP、exec、监控不可用时，主终端不会被判定为连接失败。
4. 普通 Linux/Windows SSH 的现有能力不回归。
5. 自动识别可用但不是唯一入口，手动模式可以覆盖。
6. 老 Comware 的 SHA-1 GEX 兼容只在显式 legacy 选项和精确 Banner 匹配时启用，不能影响
   普通连接的安全默认值。

以上代码条件已满足；没有实体设备不阻塞代码目标，但限制了实机兼容结论。
