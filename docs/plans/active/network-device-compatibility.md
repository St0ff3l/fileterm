# 网络设备 SSH 兼容 MVP 计划

> 状态：Active
> 关联：Refs #201
> 范围：H3C/Comware、Huawei/VRP、Cisco，以及实际提供 SSH/Telnet/Serial CLI 的其他网络设备

## 1. 范围判断

第一版只做到“网络设备模式”的基础兼容，不做全厂商命令库。

MVP 必须具备：

- SSH 连接可明确选择“网络设备”模式；
- 网络设备使用 raw PTY，不执行 `sh -c`、POSIX wrapper、CWD marker 或服务器探测脚本；
- 关闭 SFTP 浏览、CWD 跟随、Shell 探测、Shell 集成和资源监控；
- 允许选择 `vt100`、`ansi`、`xterm` 等 terminal type；
- 可选的 SSH Banner 自动识别，在打开额外 channel 之前完成判断；
- 主终端与 SFTP、exec、监控等可选能力相互隔离，后者失败不能关闭终端。

这已经足够覆盖 #201 的核心问题：设备登录后保持交互，不会因为 FileTerm 把交换机当成
Linux/Windows 服务器而发送不兼容命令。

暂不做：

- 厂商命令大全；
- 自动关闭分页；
- 自动进入 enable/config 模式；
- 复杂 prompt 状态机；
- 按品牌推断所有型号和固件的行为；
- 全局启用 DSA、CBC、3DES 等弱 SSH 算法。

FileTerm 当前已经有 Telnet 和 Serial 会话；本计划先解决 SSH 网络设备模式，不为了统一
概念而重写现有 Telnet/Serial controller。

## 2. 当前问题

现有 SSH 流程默认远端是完整服务器：

- `apps/tauri/src-tauri/src/sessions/ssh.rs` 固定请求 `xterm-256color`，建立 shell 后还会先
  做平台探测，再决定是否注入 CWD 集成脚本；
- `apps/tauri/src-tauri/src/sessions/system_metrics.rs` 会执行 POSIX/Windows 平台和指标探测；
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
  只用于识别和后续扩展，不代表所有型号都已适配。

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

第一版只维护少量保守 pattern：

| 设备族      | 识别线索示例                        |
| ----------- | ----------------------------------- |
| Cisco       | `Cisco-*`、IOS、IOS-XE、NX-OS 标识  |
| Huawei      | `HUAWEI-*`、`HUAWEI-VRP*`、`VRP-*`  |
| H3C/Comware | `Comware-*`、`3Com OS-*`、`mpSSH_*` |

自动识别未知时：

- 不把未知设备强行判定为 Linux；
- 用户手动选择 `network-device` 时优先级最高；
- 可以先按普通服务器连接，但必须有明确的诊断日志和可停止探测的保护；
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

### Phase 1：手动网络设备模式（先解决 #201）

- [ ] 在 `packages/core` 增加 device mode 和 SSH terminal type。
- [ ] 更新 profile 默认值、迁移逻辑、`CreateProfileInput` 和 workspace capabilities。
- [ ] Rust 在主 shell 前确定有效 mode；network-device 跳过所有服务器探测和 SFTP 初始化。
- [ ] PTY 使用 profile terminal type；网络设备命令路径不使用 POSIX wrapper。
- [ ] renderer 增加连接对象和 terminal type；隐藏或置灰不适用能力。
- [ ] 补 H3C/Huawei mock 行为，确认登录后主终端持续可用。

### Phase 2：Banner 自动识别

- [ ] 在握手阶段提取并规范化远端 identification。
- [ ] 增加 Cisco、Huawei、H3C/Comware pattern 和单元测试。
- [ ] 自动识别结果在任何 exec、CWD、指标或 SFTP 探测之前生效。
- [ ] 手动 mode 覆盖自动结果；未知设备保持安全 fallback。

### Phase 3：真实设备验证与必要的专项修复

- [ ] 使用 H3C/Comware、Huawei/VRP、Cisco 实机验证 SSH 登录、PTY、换行、resize 和断线行为。
- [ ] 再验证实际提供 CLI 的 TP-Link、水星、腾达、中兴、NETGEAR 型号。
- [ ] 只有真实握手日志证明需要时，才增加最小范围的旧 KEX/host key/cipher 兼容。
- [ ] 分页、prompt、enable/config 和型号 profile 另开计划，不塞进本 MVP。

## 7. 测试与验收

### 自动化

- [ ] 普通 Linux/Windows SSH：原有 CWD、SFTP、指标和文件区不回归。
- [ ] network-device：没有 `/etc/os-release`、`uname`、CWD marker、metrics script 或 POSIX wrapper。
- [ ] H3C/Huawei mock：额外 exec 被拒绝或关闭时，主 PTY 仍能输入输出。
- [ ] terminal type：网络设备默认 `vt100`，profile 选择值正确传给 PTY。
- [ ] Banner：Cisco/Huawei/Comware 在第一个可选 channel 前完成识别。
- [ ] 未知 Banner：手动 network-device 可用，普通 server 不因旧算法需求被误分类。
- [ ] SFTP、exec、metrics 独立失败时，只关闭对应 capability，不关闭 terminal。
- [ ] 旧 profile、snapshot、bridge、renderer 表单和能力矩阵通过 contract/type 测试。

### 实体设备记录

每台设备记录：品牌、型号、固件、协议、SSH identification、terminal type、登录首屏、
是否会关闭 exec、是否提供 SFTP、是否出现分页、断线原因和最终结论。

没有实体设备时，只能宣称“mock/协议层通过”，不能宣称某个品牌全部支持。

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

MVP 完成条件：

1. 用户能在 SSH profile 中选择网络设备模式和 terminal type。
2. H3C/Huawei 类设备登录后，FileTerm 不发送 Linux/Windows 探测命令，终端保持可交互。
3. SFTP、exec、监控不可用时，主终端不会被判定为连接失败。
4. 普通 Linux/Windows SSH 的现有能力不回归。
5. 自动识别可用但不是唯一入口，手动模式可以覆盖。
6. 未经实体设备验证，不在发布说明中扩大为“全品牌支持”。

Issue #201 只使用 `Refs #201` 关联；代码合入、发布和真实设备验证完成后，再由维护者决定
是否关闭 issue。
