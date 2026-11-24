# 二期 · Windows SSH 指定出站网卡

状态：已实现。Windows 双路径探针、开发版 SSH 主流程及构建通过，兼容场景待验收。与[一期代理分配](../completed/ssh-proxy-bulk-assignment.md)使用不同的网络入口，可以独立验证。

## 用户场景与交付范围

Windows 一台机器同时连着多个网络，包括 VPN 虚拟网卡。用户希望给不同的 FileTerm SSH 连接明确选择出站网卡，让它们同时访问各自网段的服务器。若每张网都已有可达目标的 SOCKS5 / HTTP CONNECT 端点，一期即可满足用户场景。优先请需求方提供至少一个只能经指定网卡访问的真实目标，连同网卡、VPN 类型和目标能否用 IP 直连的信息。

首版限定 **Windows 上普通 SSH 配置的直连，目标为 IPv4 或 IPv6 字面量**。连接编辑器提供「系统默认」和可选适配器。该选择只影响 FileTerm 本机建立的 SSH TCP 连接。代理出口由代理工具管理。全局路由、自动选择网络、私有 DNS、代理/跳板叠加、OpenSSH 配置别名、其他协议以及批量分配网卡另行评估。

## 先做实机验证，再决定是否实现

1. 用目标 Windows 机器准备两条可辨别的网络路径，最好有一条是需求方实际使用的 VPN 虚拟网卡。为每条路径准备一个授权 SSH 测试目标和端口，先记录各自用系统默认路由的结果。
2. 用一个最小 Rust 探针在拨号前为 TCP 套接字设置 Windows `IP_UNICAST_IF` 或 `IPV6_UNICAST_IF`。用 `GetAdaptersAddresses` 获得对应地址族的接口索引。IPv4 索引按网络字节序传给 Winsock，IPv6 使用主机字节序。先验证地址族匹配和 IPv4，再验证 IPv6；链路本地 IPv6 若需 scope ID，验证支持后再开放该类地址。
3. 同时建立两条指定出口的连接，从 Windows 抓包、目标端日志或可明确区分路径的网络夹具确认两条路径。单看连接成功或源 IP 不能替代路径验证。拔掉网线或断开 VPN，确认指定网卡的连接报错，检查没有从默认路线发出连接；随后重新启用并再次验证。
4. 记录网卡 GUID 与运行时索引在重新连接 VPN 前后的表现，以及 VPN 强制接管和远端路由的限制。只有两条路径都能确定、指定出口失效时行为明确，才进入产品化。若只有 IPv4 通过，可以把首版收窄为 IPv4，记录原因并在界面明确限制；若 VPN 或路由策略无法满足核心场景，保留实测结果，暂停这一场景的产品化。

参考 [适配器枚举](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getadaptersaddresses) · [适配器标识与索引](https://learn.microsoft.com/en-us/windows/win32/api/iptypes/ns-iptypes-ip_adapter_addresses_lh) · [IPv4 Winsock](https://learn.microsoft.com/en-us/windows/win32/winsock/ipproto-ip-socket-options) · [IPv6 Winsock](https://learn.microsoft.com/en-us/windows/win32/winsock/ipproto-ipv6-socket-options)。

### Windows 实机探针记录（2026-11-24）

- 一期 Windows SSH 实测结果已记录在本地完成计划，验收提交为 `4d61a450`。[PR #417](https://github.com/St0ff3l/fileterm/pull/417) 核对时仍开放，验收文档变更尚未提交。
- 环境为 Windows 11 专业版 26H2，OS build `26300.9974`，Rust `1.97.1`。受控目标为 WSL2 Ubuntu 24.04.4 LTS 上临时启动的 OpenSSH `9.6p1`，只监听 WSL `eth0` 的测试地址。用临时公钥认证，并校验夹具主机密钥。
- 独立 Rust 探针在 Windows 本机编译运行。它用 `GetAdaptersAddresses` 枚举 `AdapterName`、运行状态及双栈索引，每次拨号按适配器标识重新解析。IPv4 在 `connect` 前按网络字节序设置 `IP_UNICAST_IF`，IPv6 以主机字节序设置 `IPV6_UNICAST_IF`。OpenSSH 负责会话认证，探针通过标准输入输出传送 SSH 字节并负责实际的 Windows TCP 拨号。探针构建、Clippy 和 Rust 格式检查通过。
- IPv4 默认对照使用 Windows `Find-NetRoute` 和 TCP 连通性检查，WSL 目标可达，默认接口为 WSL 虚拟网卡，索引 `67`。指定该适配器后，两次连接均完成认证并由远端命令返回 `AUTHENTICATED_SSH_OK`。探针记录的两组源端口 `65511`、`65518` 与目标端在 `22417` 记录的 `Connection from` 和 `Accepted publickey` 一致。
- IPv6 使用同一 WSL 虚拟网卡，目标为链路本地 IPv6，Windows scope ID 为 `67`。指定接口后认证成功，探针的源端口 `49165` 与目标端在 `22418` 的认证日志一致。目标端连接日志也记录了 `rdomain "eth0"`。IPv4 与 IPv6 共用同一张虚拟网卡，合计只证明一条网卡路径。
- 负向对照中，同一 IPv4 目标持续可达，但指定 WLAN（Intel Wi-Fi，索引 `15`）时拨号超时，目标端没有额外的连接记录。指定处于离线状态的 QuickFox 虚拟适配器（IPv4 索引 `26`）在拨号前返回离线错误。当前 WLAN 无 IPv6 索引，测试在拨号前返回地址族错误。这些结果覆盖错误网卡和已离线网卡，尚未执行停用一条已验证路径的测试。
- ZeroTier（IPv4 索引 `24`）和 Tailscale（IPv4 / IPv6 索引均为 `62`）可枚举。Hyper-V 默认交换机可见，当前权限不能列出 VM。初期只使用 WSL 目标，后续增加了已授权的局域网 NAS。
- WLAN 本机地址对照：临时 Windows TCP 监听器绑定本机 WLAN IPv4。指定 WLAN（索引 `15`）的 Rust 探针连接成功，监听器记录对端和本地端均为同一 WLAN IPv4。Windows 查到目标为 `/32` 路由，协议 `Local`，下一跳 `0.0.0.0`。若在同一地址增加 `portproxy`，WSL 的 SSH 日志只会记录 Windows 转发器到 WSL 的连接，无法验证原始连接经物理 WLAN 出站。因此该对照不计为第二条路径。临时监听器已关闭，未添加 `portproxy` 规则或停用网卡。
- WLAN 与 WSL 双路径：需求方更正 NAS 地址后，指定 WLAN（索引 `15`）连接 Synology NAS `22/TCP`，校验已信任的主机密钥并完成认证。NAS 远端 `SSH_CONNECTION` 报告源端口 `62047`，与 Windows 探针一致。同一 NAS 会话保持 `Established` 时，指定 WSL 虚拟网卡的另一条 SSH 会话完成公钥认证，WSL `sshd` 接受了与探针一致的源端口 `58864`。
- 实际断开 WLAN 再恢复：Windows 报告 WLAN 为 `Disconnected`。指定该网卡访问仍由系统默认路由可达的 WSL `22417/TCP`，探针在拨号前报网卡离线并以 1 退出。目标端新增的两条日志均来自已知的默认路由对照连接（源端口 `55464`、`55468`），无额外连接。WLAN 重连后状态恢复为 `Up`，GUID 与索引 `15` 保持一致，绑定 WLAN 再次取得 NAS 的 SSH 握手。断开期间的默认路由对照目标是 WSL；NAS 的 WLAN 地址依赖 WLAN，当时未作为默认路径对照。
- 产品拨号模块的 Windows 验证：用独立命令行夹具直接调用产品的 Tokio 拨号代码。指定 WLAN 到 NAS 的 IPv4（源端口 `59806`），以及指定 Tailscale 到同一 NAS 的 IPv6（源端口 `58726`），均收到 OpenSSH 握手。IPv6 使用同一产品拨号代码再次连接并完成 SSH 认证，目标端 `SSH_CONNECTION` 回报源端口 `50261`，与 Windows 记录一致。这项 IPv6 认证由 OpenSSH 客户端完成。未覆盖公网 IPv6、VPN 强制接管、VPN 重连导致索引变化。首版暂不开放链路本地 IPv6 与 scope ID 输入。

## 产品化步骤

### 1. 定义存储和平台边界

- 在 [共享类型](../../../packages/core/src/index.ts)为 SSH 配置和保存输入增加可选的 Windows 网卡偏好，内含适配器稳定标识和可选显示名。以经过实机验证的接口 GUID 作为标识，每次建连重新映射为当下的 IPv4 / IPv6 索引。索引是运行时值。
- 老配置缺省即系统默认。清除偏好后可直连。网卡是本机资源，导入、同步或迁移到另一台机器时保留原偏好供用户辨认，无法解析就提示重新选择。确认现有表单序列化、Rust 保存、快照、备份往返均保留该字段。在非 Windows 平台打开含此偏好的配置时明确显示原因，并允许手动清除，普通缺省配置继续走既有路径。
- 明确兼容性校验：网卡选择与代理、跳板、OpenSSH 别名和首版不支持的地址冲突。Rust 在保存和建连时都校验，覆盖旧 `jumpProfileId` 与新 `jumpProfileIds`。具有网卡设置的配置作为跳板时同样拒绝并指出冲突位置。含网卡设置的保存配置在一期代理管理界面显示冲突，在 Rust 批量入口也禁止绑定。网卡暂时离线不影响保留现有偏好。

### 2. 枚举网卡并连接

- 在 Rust 通过 Windows 专用模块封装枚举和绑定，复用 [Cargo.toml](../../../apps/tauri/src-tauri/Cargo.toml) 现有 Windows 依赖并补齐所需 feature。枚举命令只给 renderer 提供稳定标识、显示名、运行状态和可用地址族。通过 `commands`、`lib/runtime.rs` 和 [tauri-api.ts](../../../apps/tauri/src/bridge/tauri-api.ts) 暴露。表单打开或主动刷新时枚举，支持物理网卡和符合条件的虚拟网卡。
- [SSH 传输入口](../../../apps/tauri/src-tauri/src/sessions/ssh/transport/proxy.rs) 的直连分支根据偏好决定是否使用指定网卡拨号。模块创建与目标地址族一致的非阻塞套接字，设置出站接口后再建立连接，将流交回现有 SSH 握手。沿用外层超时、取消、TCP_NODELAY、主机密钥校验和认证。默认直连路径继续走原有拨号。
- 对缺失、离线、地址族不匹配、绑定失败和目标不可达返回可操作错误，包含网卡名及失败阶段。指定网卡失败时结束这次连接，用户可以主动改回系统默认。用 IP 字面量保持首版的出站范围可验证。
- 核对 [会话启动](../../../apps/tauri/src-tauri/src/commands/session_spawn.rs)、[连接测试](../../../apps/tauri/src-tauri/src/commands/profile_commands.rs)及[手动重连](../../../apps/tauri/src-tauri/src/commands/tab_lifecycle.rs)都经过同一拨号逻辑。自动重连用该会话的现有偏好重新解析实时接口索引。设置变更在从连接库新建或重新加载配置建立 SSH 时生效。

### 3. 接入连接表单

- 在 [连接网络区](../../../apps/tauri/src/renderer/features/connections/connection-network-section.tsx)复用 `DropdownSelect`，显示「系统默认」及可用网卡。适配器重名时提供可区分的信息，已保存但缺失的适配器保留原显示名并提示重新选择。
- 对不支持的组合即时展示原因，显式提供切回系统默认的方法。保留现有代理和跳板配置，要求用户自行选择要使用的出口配置。现有「测试连接」应使用表单当前选择，测试通过只表明该次连接成功。

## 验证与完成条件

- [x] Rust 回归覆盖缺省、缺失标识、索引变化、地址族校验、旧/新跳板、代理冲突和失败不回落。模拟模块测试只确认决策逻辑，实机负责确认真实出站。
- [x] Windows 上至少两条实际可辨别的路线同时建立 SSH。用抓包或目标侧证据核实路径，拔掉或停用指定网卡后确认错误且无默认出口连接；重新接入后测试手动重连和自动重连。
- [x] 验证直连测试、默认直连、成功 SSH 会话内复用的 SFTP、一期代理分配、网卡偏好编辑与清除，以及代理管理页和独立连接表单的一致性；具体证据见下方 Windows 代理验收记录。
- [ ] 重启 FileTerm 并读取缺少 `sshNetworkInterface` 字段的既有配置，确认按系统默认处理。
- [ ] 导入带有另一台机器适配器标识的配置，确认显示网卡缺失原因并可切回系统默认。
- [x] 跑完 [AGENTS.md](../../../AGENTS.md) 的代码质量门禁，含实际 Windows 目标构建。桌面验收按[开发版识别规则](../../quality/desktop-ui-preview.md)绑定当前进程。记录已验证的 Windows 版本、适配器/VPN、IPv4/IPv6 和目标类型，单列无法取得实机证据的场景。

### Windows 开发版验收（2026-11-24）

- FileTerm Dev `2.3.4-beta.12` 运行本地二期改动，基线提交 `4d61a450`。核对了当前项目的可执行文件、Vite `localhost:5188` 和开发版窗口。使用隔离的数据目录和临时 WSL OpenSSH 目标 `22419/TCP`。
- 指定 WSL 虚拟网卡的正式会话打开了远端 Shell，文件面板列出了夹具目录。WSL `sshd` 记录源端口 `64148` 的同一条 SSH 连接打开 Shell 和 SFTP 子系统。
- 只终止夹具上现有的连接进程，监听服务保持运行。FileTerm 自动重连，目标端记录新来源端口 `63471` 并再次打开 Shell 与 SFTP。通过 UI 手动断开再重连，目标端记录另一条认证成功的连接，源端口 `63493`。
- 独立表单显示保存的 WSL 网卡和实时适配器列表。对同一目标，在草稿中改用 WLAN 并点击「测试」，FileTerm 返回指定 WLAN 的拨号超时（Winsock `10060`）。切回系统默认后，连接测试和目标端认证均成功。保存并重新打开显示「系统默认」，存储中不再有网卡字段。重新选择并保存 WLAN 后，存储中的适配器标识与系统 GUID 相符，字段仅包含 `adapterId`、`displayName`。
- 类型检查、ESLint、Prettier、CSS 契约、Rust 格式、Renderer 349 项、构建脚本 7 项、Rust / CLI / 契约测试和 Clippy 均通过。Windows x64 Tauri 调试可执行文件构建通过（`--debug --no-bundle`）。
- 待验收：缺少 `sshNetworkInterface` 字段的旧配置重启读取，以及导入另一台机器的适配器标识后对缺失网卡的提示。代理冲突与平台不支持情形已有代码分支和针对性回归。公网 IPv6、VPN 强制接管、VPN 重连后的索引变化和安装包验收未覆盖。兼容验收完成后再将计划移入 `docs/plans/completed/`。

### Windows 代理分配端到端复验（2026-11-24）

- 环境：Windows 11 专业版 26H2，OS build `26300.9974`；FileTerm Dev `2.3.4-beta.12`，仓库基线 `4d61a450`，Vite `localhost:5188`。使用隔离 portable 配置。目标为 WSL2 Ubuntu 24.04.4 LTS 内的三台临时 OpenSSH `9.6p1` 实例，经 Windows loopback 转发到 `127.0.0.1:22521`、`:22522`、`:22523`。代理夹具只允许这些 SSH 目标端口。
- 在 FileTerm 保存 SOCKS5 `127.0.0.1:11081` 和 HTTP CONNECT `127.0.0.1:18081`。A 分配 SOCKS5、B 分配 HTTP CONNECT、C 先分配 SOCKS5；三条连接均建立了真实 SSH shell。FileTerm 同一会话的 SFTP 面板加载了远端 `/home/stoffel` 目录。
- 脱敏路径证据：SOCKS5 日志分别记录 `127.0.0.1:22521 connected`（`2026-11-24T14:49:59.213Z`）和 `:22523 connected`（`14:52:59.843Z`）；HTTP CONNECT 日志记录 `:22522 connected`（`14:52:26.190Z`）。三个 WSL `sshd` 均记录该测试用户公钥认证成功，FileTerm 显示对应远端 shell。由代理端协议、目标地址/端口日志和 SSH 目标认证共同确认 SSH 流量经指定代理。
- C 保持 SOCKS5 会话时，将配置改为 HTTP CONNECT。FileTerm 提示变更在新建或手动重连时生效；关闭旧会话后手动重连成功。HTTP CONNECT 日志新增 `127.0.0.1:22523 connected`（`14:55:28.625Z`），C 的 WSL `sshd` 随后记录新的公钥认证成功。
- 删除保护：尝试删除仍被 A 引用的 SOCKS5 配置，Rust 命令返回“该代理正被以下 1 个连接使用，请先解绑再删除：E2E SSH A”；代理条目和引用保留。独立连接编辑表单的「网络与代理」页也显示 C 已绑定全局 HTTP CONNECT 配置 `127.0.0.1:18081`，与代理管理页一致。
- 不回落验证：确认 SOCKS5/HTTP CONNECT 测试监听端口均停止后，Windows OpenSSH 使用显式禁用 `ProxyCommand` / `ProxyJump` 的直连控制仍返回 `DIRECT_A_WHILE_PROXY_DOWN_OK`。随后 FileTerm A 明确报错 `SOCKS5 proxy connect failed: Proxy server unreachable`；没有建立 shell。WSL A 的 `sshd` 日志在直连控制后没有新增 FileTerm 连接或认证记录，证明代理不可用时没有静默走直连。
- 本次用例验证的是本机真实 SSH 协议流量通过 SOCKS5 / HTTP CONNECT 代理及故障行为；代理目标是 WSL2 loopback 转发，不代表公网或物理网卡路径。双物理/虚拟网络路径和指定网卡停用不回落证据沿用上方探针记录。本次未记录密码、令牌或私钥。
