# RDP / VNC 支持调研与实施建议

状态：提案，尚未实现或完成技术验证。调研日期：2026-11-17。

本文评估在 FileTerm 标签页内连接已有 RDP/VNC 服务的方案，不要求远端安装 FileTerm agent。当前架构仍将 RDP/VNC 列为本阶段范围外；本文是下一阶段候选计划，不代表已修改产品范围或支持承诺。

## 1. 推荐结论

- 保持 Rust 负责连接、认证、协议和会话生命周期，renderer 负责画面展示与输入采集。
- RDP 优先验证 IronRDP 原生 Rust 库；以 FreeRDP 为兼容性对照。关键场景不通过则改用 FreeRDP，不同时维护两套生产 RDP 引擎。
- VNC 优先验证 vnc-rs，但它只是架构匹配的候选，不能宣称具备 noVNC / LibVNCClient 同等成熟度。
- noVNC 是成熟的备选及行为参考，直接集成需要另行形成允许前端 VNC 协议引擎的 ADR；纯 TCP 转发放在 Rust 并不能消除这个架构差异。
- 先验证协议、输入与图像通道，再做完整表单。工程交付建议先 VNC、再 RDP；RDP 的可行性验证应前置，避免最后才发现认证或图形能力缺口。

## 2. 可参考项目

以下能力来自上游文档；适配成本与推荐顺序是针对 FileTerm 的工程判断。上游主分支包含某能力不表示所选发布版本已经提供，实施时须固定发布版本或提交并复核依赖许可证。

| 项目                                                                                 | 上游能力与授权                                                            | FileTerm 的用途与限制                                                                                |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| [FreeRDP](https://github.com/FreeRDP/FreeRDP)                                        | C 实现的 RDP 库与客户端，Apache-2.0                                       | 成熟 RDP 基线与生产候选；需处理 FFI、原生依赖、回调线程及各平台构建                                  |
| [IronRDP](https://github.com/Devolutions/IronRDP)                                    | 模块化 Rust RDP，MIT OR Apache-2.0；有原生客户端及 WASM 绑定              | 首选验证其原生 connector/session/graphics 路线；借鉴 viewer 的会话驱动与输入，不嵌入现成 viewer 窗口 |
| [noVNC](https://github.com/novnc/noVNC)                                              | 浏览器 VNC，核心 MPL-2.0；多种认证、编码、剪贴板、缩放与光标能力          | 成熟 VNC 实现与兼容性对照；需要 WebSocket/RFB 通道，直接接入会改变当前 renderer 边界                 |
| [vnc-rs](https://github.com/HsuJv/vnc-rs)                                            | 异步 Rust VNC 引擎，MIT OR Apache-2.0；有 Raw、CopyRect、Tight、ZRLE 示例 | Rust 后端候选；认证、服务器扩展和维护覆盖必须实测，README 明确部分编码未实现或未验证                 |
| [LibVNCServer / LibVNCClient](https://github.com/LibVNC/libvncserver)                | 跨平台 C VNC 库，项目声明 GPL；有多种认证与编码                           | 协议与测试参考；不是可以按 MIT/Apache 条件直接链接的依赖，不默认纳入分发                             |
| [Apache Guacamole](https://guacamole.apache.org/doc/gug/guacamole-architecture.html) | JS 展示层、Guacamole 协议、guacd 原生代理的分层体系                       | 借鉴显示/输入与协议引擎隔离；整套引入会增加本地 daemon 和原生依赖，且需单独核对后端依赖授权          |
| [Remmina](https://github.com/FreeRDP/Remmina)                                        | 多协议桌面客户端，GPLv2+                                                  | 参考会话设置、全屏、快捷键和 RDP 插件边界；不复制 GPL 代码进入当前 MIT 实现                          |

补充：RustDesk 主要解决自身远控协议、ID/中继和被控端部署，可参考交互，不作为本次连接标准 RDP/VNC 服务的基础。

### 重点阅读入口

- [IronRDP README](https://github.com/Devolutions/IronRDP/blob/master/README.md)：当前列出 TLS、NLA/CredSSP、NTLM/Kerberos、剪贴板、显示控制等能力。逐项确认 native client 集成情况，不把协议结构存在等同于完整客户端支持。
- [IronRDP 架构](https://github.com/Devolutions/IronRDP/blob/master/ARCHITECTURE.md)：Sans-I/O 核心、原生 client/viewer、图形与虚拟通道拆分。
- [FreeRDP GDI API](https://pub.freerdp.com/api/gdi_8c.html)：后端解码与 framebuffer 路线的参考，避免通过操作系统窗口嵌入客户端。
- [noVNC API](https://github.com/novnc/noVNC/blob/master/docs/API.md)：每个连接一个 RFB 对象、只读模式、缩放/远端 resize、剪贴板事件。
- [noVNC 授权清单](https://github.com/novnc/noVNC/blob/master/LICENSE.txt)：核心与附带资源许可证分别核对。
- [vnc-rs README](https://github.com/HsuJv/vnc-rs/blob/main/README.md)：编码、既有验证范围及解码资源限制。
- [Guacamole 协议后端配置](https://guacamole.apache.org/doc/gug/configuring-guacamole.html)：RDP/VNC 功能与依赖边界参考。
- [RustDesk 自身架构说明](https://rustdesk.com/blog/rustdesk-vs-vnc/)：区分其远控体系与标准 VNC 服务接入。

## 3. 第一版范围

### 共用能力

- 连接列表新增 RDP/VNC 类型；保存地址、端口、协议专有设置，凭据沿用既有 backend 存储与公开快照脱敏策略。
- 在普通标签页内显示远端桌面，支持鼠标、滚轮、键盘、适应窗口、原始比例、全屏、只读、断开和手动重连。
- 提供显式发送组合键操作与释放远程输入焦点的快捷键；失焦、切换标签、断线时释放已按下的键和鼠标按钮。
- 文本剪贴板按连接开关，明确本地到远端/远端到本地的方向；通过原生 bridge 接入，限制大小并避免回传循环。
- TCP 直连为基础，复用现有代理和 SSH 路由能力；SSH 隧道资源由桌面会话持有，不依赖一个必须保持打开的 SSH 终端标签。

### RDP

- 主机/端口、用户名、密码、可选域；NLA/TLS、证书验证与交互确认。
- 单显示器、初始分辨率；服务器支持时动态调整远端分辨率，不支持时仅本地缩放。
- 重点验证 Windows 自带 RDP 服务与 xrdp；支持 Windows 的具体版本、认证组合以实测矩阵为准。
- RD Gateway、Kerberos/SSO 等企业场景先评估，不进入首版承诺；上游支持不代表 FileTerm 已适配。

### VNC

- 主机/显式端口、密码、共享会话、只读；首版明确支持经典 VNC 密码认证的已验证服务端。
- Raw / CopyRect 作为基础，Tight / ZRLE 作为性能验收项；服务器扩展须协商，不能假定全部存在。
- macOS 兼容 VNC 密码模式与 Apple 原生认证分开标记；后者未通过验证前不宣称支持。
- 加密方式必须明确：经典 VNC 认证不提供完整会话加密，公网使用走 SSH/VPN；VeNCrypt/厂商认证另列能力矩阵。

首版不加入音频、麦克风、磁盘/打印机/USB/智能卡重定向、多显示器、录像或桌面文件传输。既有 SFTP/FTP 保持自己的传输链路。

## 4. 接入当前架构

```text
RDP / VNC 服务
    ↕ TCP / 现有代理 / SSH direct-tcpip
Rust sessions/rdp/ 或 sessions/vnc/（认证、协议、完整 framebuffer）
    ↕ desktop service（输入、订阅、取消、窗口与会话归属）
Tauri commands + bridge（控制） / 独立二进制帧通道（显示）
    ↕
renderer remote-desktop workspace（Canvas、工具栏、焦点）
```

### 领域与状态

- `packages/core`：扩展 `SessionType`、`ConnectionProfile` 为独立 RdpProfile / VncProfile；增加桌面布局和 desktop capability。
- 当前 `getConnectionCapabilities` 与 `createTabLayout` 的兜底会把未知协议归入终端，必须同步改造并测试所有现有协议。
- 定义 desktop 连接状态、显示尺寸、能力和输入请求；会话状态仅含元数据。禁止把 framebuffer、密码、原始协议流放入 WorkspaceSnapshot。
- Rust `sessions/rdp/` 与 `sessions/vnc/` 分离；仅共享图形桌面的生命周期与显示契约，不模拟 terminal/file controller。
- backend 的 profile 校验、默认端口、secret 隔离、导入导出、备份同步、最近连接、标签恢复均需覆盖新类型。未知类型不得按 SSH 静默导入。
- renderer 接入连接管理器与 workspace stage；新桌面标签不显示 SFTP、CWD、sudo 或 SSH 系统指标。
- CLI/MCP 的现有会话入口要显式校验能力；本阶段不增加远控自动化或让终端命令入口误接桌面会话。

### 控制与图像通道

- 连接、断开、输入、剪贴板、resize 和证书确认继续经类型化 Tauri bridge；命令校验窗口、tab、session generation 与有效订阅。
- 先测 Tauri binary Channel 的真实复制成本及三平台表现；不使用 base64、JSON 像素数组或 workspace 广播传每帧图像。
- backend 保留最新完整 framebuffer，按脏矩形更新 renderer；帧序号、尺寸版本与 ACK 管理有限在途数据和总内存预算。
- 拥塞时在 backend 合并脏区域或重新发送完整基准帧。不能直接丢弃增量包，否则 CopyRect 和后续更新会使用错误基底。
- 隐藏标签暂停/降低展示投递；协议仍正确消费，恢复订阅时发完整基准帧。重连后拒绝旧 generation 的帧与输入。
- 只有实测 Channel 不达标再选受控 loopback 二进制 WebSocket；这只是图像传输，远端协议仍在 Rust。若采用，需限定 loopback、Origin、一次性会话授权、目标绑定及关闭回收，不提供任意 TCP 转发入口。
- 独立窗口和分屏先保证明确的输入所有者；不让跨窗口 snapshot 广播复制 framebuffer。

1080p RGBA 全帧按 30 fps 发送约 249 MB/s，4K 约 995 MB/s，均未计复制开销；因此脏矩形、有界队列和重同步是前置设计。Canvas 为初始显示实现，WebGL 或原生显示仅在测量后决定。

## 5. 实施阶段与退出条件

| 阶段              | 交付                                                                               | 通过条件                                                             |
| ----------------- | ---------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| P0 技术验证       | 最小桌面画布、二进制通道；IronRDP / vnc-rs 各连一台真实服务端；RDP 与 FreeRDP 对照 | 三平台构建和显示输入闭环；认证、证书、资源预算可控；记录引擎选型结果 |
| P1 领域与生命周期 | profiles、secrets、capabilities、desktop layout、取消/关闭/恢复                    | 旧连接与备份不回归；标签关闭不残留 socket/task；MCP/CLI 不误路由     |
| P2 VNC MVP        | 已验证认证、基础/压缩编码、焦点、剪贴板、缩放、SSH 路由                            | TigerVNC/TightVNC 与目标 QEMU 服务端通过；不支持的认证明确失败       |
| P3 RDP MVP        | NLA/TLS、证书交互、域用户、输入、剪贴板、单屏 resize                               | Windows RDP 与 xrdp 通过；本地/域用户和失败路径可解释                |
| P4 发行验收       | 三平台打包、混合 DPI、长连接、多标签/窗口、弱网与清理                              | 干净机器安装可用，无额外运行环境；完整质量门禁与真实桌面回归通过     |

P0 时间盒建议 3–5 个工作日，仅用于形成可行性结论，不承诺完整功能。后续排期以 P0 的兼容性、图像通道和打包结果重新估算。

### P0 必测与决策

1. IronRDP：本地账号和域账号 NLA、正确/错误密码、首次证书及证书变更、1080p/4K、按键组合、中文文本输入、远端 resize。无法在合理成本内补齐的关键缺口，切换 FreeRDP 原生库路线。
2. vnc-rs：目标服务器的认证协商、Raw/CopyRect/Tight/ZRLE、像素格式、光标、文本剪贴板、桌面尺寸变更及异常消息限制。记录 Apple ARD / VeNCrypt 是否可用，不依赖 README 推断。
3. 图像通道：1080p 与 4K，静态桌面/滚动/窗口拖动；统计输入到显示延迟、CPU、内存、复制量、队列深度。建议以 1080p 滚动接近 30 fps、4K 静态管理可用作为初始目标，硬件与负载同时记录。
4. 多会话：四标签运行、后台/前台切换、断线重连、重复开关 50 次；验证总内存有界、增量正确、旧帧不串会话、焦点不遗留修饰键。
5. vnc-rs 若无法满足首批目标服务端，暂停承诺 VNC 覆盖并提交 noVNC 边界 ADR 或重新选择引擎；不默认转为自研完整 VNC 栈或链接 GPL 依赖。

## 6. 验证与文档落地

- 自动测试聚焦：profile 兼容/secret 脱敏、能力路由、输入释放、帧顺序与重同步、尺寸/压缩数据限制、重连 generation、取消与端口释放。
- 真实协议：Windows RDP、xrdp、TigerVNC/TightVNC、QEMU VNC；macOS 屏幕共享按认证模式单列，不以单个服务端成功代表整个协议兼容。
- 三平台真实 Tauri WebView 验证：输入法、AltGr、Command/Option/Windows 键、DPI、全屏、系统快捷键和原生剪贴板；浏览器预览不代替桌面验收。
- 代码实施时执行 AGENTS.md 的类型、lint、格式、renderer、Rust/contract、clippy 门禁，并验证当前全部发行架构的打包依赖。
- 原生库选型须确认许可证与可选 codec 依赖；不得只根据顶层仓库 LICENSE 推断整个安装包。
- 确定实施范围后再更新 architecture/roadmap；如引入 noVNC 前端协议例外则增加 ADR。此次仅新增调研计划，不改现有产品事实。

## 7. 同类产品补充核查（2026-11-17）

| 产品                               | 核查结果                                                                                                                                                                                                             | 参考价值                                                                                                                          |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Netcatty                           | 当前 README 与完整主分支文件树未发现 RDP/VNC 实现入口；[VNC 请求 #4242](https://github.com/binaricat/Netcatty/issues/4242)仍开放                                                                                     | 暂无已确认的图形桌面接入实现可参考；这是公开资料核查结论，不覆盖私有开发                                                          |
| DBX：RDP Remote Desktop 插件       | 商店已发布 0.2.8；[源码](https://github.com/caizong10/dbx-plugin-rdp/tree/v0.2.8)为 Rust/IronRDP sidecar + Canvas 2D，二进制图像/输入通道，Apache-2.0                                                                | 优先参考脏矩形、输入扫描码、证书确认、动态分辨率与剪贴板；不是独立重写 RDP 协议                                                   |
| DBX：RemoteX 插件                  | [商店元数据](https://github.com/t8y2/dbx-store/blob/main/plugins/io.dbx.remotex.json)已列出 0.1.0；[发布标签源码](https://github.com/apolloqi123/dbx-plugin-remotex/tree/remotex-v0.1.0)有实际 RDP/VNC 会话模块，MIT | RDP 使用 IronRDP，VNC 使用 vnc-rs 0.6；后端 RGBA 补丁、输入、剪贴板、resize、重连与当前提案接近；版本很新，不能据此推断生产成熟度 |
| DBX：Windows 原生远程桌面插件      | [商店元数据](https://github.com/t8y2/dbx-store/blob/main/plugins/local.rdp.manager.json)为 mstsc 管理入口                                                                                                            | 与内嵌 IronRDP 实现分开判断，不能把启动系统客户端等同于内嵌协议支持                                                               |
| WindTerm 官方 kingToolbox/WindTerm | [当前 README](https://github.com/kingToolbox/WindTerm/blob/master/README.md)未列出内嵌 RDP/VNC；[Issue #1671](https://github.com/kingToolbox/WindTerm/issues/1671)分享的是 Shell 启动 mstsc/FreeRDP 的外部客户端方案 | 可参考连接入口，不作为内嵌 RDP/VNC 引擎参考；其他同名仓库不能代表官方实现                                                         |

RemoteX 的 [Cargo.toml](https://github.com/apolloqi123/dbx-plugin-remotex/blob/remotex-v0.1.0/backend/Cargo.toml)还包含 IronRDP/SSPI/TLS 等多个本地补丁依赖。应先核对补丁动机、上游状态和必要性，不直接照搬整条 vendor 链。本次已读取相关依赖、VNC facade 和 RDP worker，未构建或实机验证这些插件。

## 8. 本次调研的验证边界

已检查仓库现有领域类型、Tauri bridge、会话与工作区入口，以及上述上游文档。尚未编译候选引擎、运行协议 POC、连接真实 RDP/VNC 服务或测量桌面性能；兼容性、工作量与最终依赖版本均待 P0 确认。
