# 二期 · Windows SSH 指定出站网卡

状态：待验证。先做 1–2 个工作日的 Windows 实机验证。最小范围跑通后，初估再用 3–5 个工作日做产品化和回归。与[一期代理分配](ssh-proxy-bulk-assignment.md)使用不同的网络入口，可以独立验证。

## 用户场景与交付范围

Windows 一台机器同时连着多个网络，包括 VPN 虚拟网卡。用户希望给不同的 FileTerm SSH 连接明确选择出站网卡，让它们同时访问各自网段的服务器。若每张网都已有可达目标的 SOCKS5 / HTTP CONNECT 端点，一期即可满足用户场景。优先请需求方提供至少一个只能经指定网卡访问的真实目标，连同网卡、VPN 类型和目标能否用 IP 直连的信息。

首版限定 **Windows 上普通 SSH 配置的直连，目标为 IPv4 或 IPv6 字面量**。连接编辑器提供「系统默认」和可选适配器。该选择只影响 FileTerm 本机建立的 SSH TCP 连接。代理出口由代理工具管理。全局路由、自动选择网络、私有 DNS、代理/跳板叠加、OpenSSH 配置别名、其他协议以及批量分配网卡另行评估。

## 先做实机验证，再决定是否实现

1. 用目标 Windows 机器准备两条可辨别的网络路径，最好有一条是需求方实际使用的 VPN 虚拟网卡。为每条路径准备一个授权 SSH 测试目标和端口，先记录各自用系统默认路由的结果。
2. 用一个最小 Rust 探针在拨号前为 TCP 套接字设置 Windows `IP_UNICAST_IF` 或 `IPV6_UNICAST_IF`。用 `GetAdaptersAddresses` 获得对应地址族的接口索引。IPv4 索引按网络字节序传给 Winsock，IPv6 使用主机字节序。先验证地址族匹配和 IPv4，再验证 IPv6；链路本地 IPv6 若需 scope ID，验证支持后再开放该类地址。
3. 同时建立两条指定出口的连接，从 Windows 抓包、目标端日志或可明确区分路径的网络夹具确认两条路径。单看连接成功或源 IP 不能替代路径验证。拔掉网线或断开 VPN，确认指定网卡的连接报错，检查没有从默认路线发出连接；随后重新启用并再次验证。
4. 记录网卡 GUID 与运行时索引在重新连接 VPN 前后的表现，以及 VPN 强制接管和远端路由的限制。只有两条路径都能确定、指定出口失效时行为明确，才进入产品化。若只有 IPv4 通过，可以把首版收窄为 IPv4，记录原因并在界面明确限制；若 VPN 或路由策略无法满足核心场景，保留实测结果，暂停这一场景的产品化。

参考 [适配器枚举](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getadaptersaddresses) · [适配器标识与索引](https://learn.microsoft.com/en-us/windows/win32/api/iptypes/ns-iptypes-ip_adapter_addresses_lh) · [IPv4 Winsock](https://learn.microsoft.com/en-us/windows/win32/winsock/ipproto-ip-socket-options) · [IPv6 Winsock](https://learn.microsoft.com/en-us/windows/win32/winsock/ipproto-ipv6-socket-options)。

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

- [ ] Rust 回归覆盖缺省、缺失标识、索引变化、地址族校验、旧/新跳板、代理冲突和失败不回落。模拟模块测试只确认决策逻辑，实机负责确认真实出站。
- [ ] Windows 上至少两条实际可辨别的路线同时建立 SSH。用抓包或目标侧证据核实路径，拔掉或停用指定网卡后确认错误且无默认出口连接；重新接入后测试手动重连和自动重连。
- [ ] 验证直连测试、成功会话内复用的 SFTP、默认直连、一期代理分配、旧配置读取、编辑与清除、跨机器或跨系统导入时缺失网卡和错误提示。
- [ ] 跑完 [AGENTS.md](../../../AGENTS.md) 的代码质量门禁，含实际 Windows 目标构建。桌面验收按[开发版识别规则](../../quality/desktop-ui-preview.md)绑定当前进程。记录已验证的 Windows 版本、适配器/VPN、IPv4/IPv6 和目标类型，单列无法取得实机证据的场景。实现和验收完成后将计划移入 `docs/plans/completed/`。
