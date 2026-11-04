# SSH 多级跳板计划

> 状态：实现完成（2026-10-14）
> 验证：本机真实 OpenSSH 覆盖直连、单跳、两跳、三跳、各跳独立密钥、目标执行/SFTP、
> 失败与取消回收；完整 Rust/contract 测试、类型、lint、本次改动格式、renderer 测试及 Clippy 通过。
> Windows/WSL、实体网络与完整桌面交互仍需手工验收。  
> 范围：让 SSH 连接可经多个普通 SSH 跳板到达目标；不扩展为任意协议传输层编排。

## 1. 结论

FileTerm 已支持最多 8 个普通 SSH 跳板，通过现有 `direct-tcpip` 通道逐层连接。
后端在连接前解析完整路径，每跳独立认证和校验主机指纹，并让整条链路随目标会话清理。

实现继续复用 SSH profile 的 `jumpProfileId` 链：

```text
FileTerm → A → B → T
配置引用：A 直连；B.jumpProfileId=A；T.jumpProfileId=B
```

每个 profile 仍保存自己的地址、账号、认证方式和主机指纹。编辑连接时展示解析后的完整
路径，避免用户只看到目标机直接引用的那一跳。此方案保留现有数据格式和单跳配置；不增加
“目标连接内拖动排序的跳板列表”。若后续确认必须在目标连接里一次编辑整条有序链，再单独
评估独立的有序跳板列表模型。

## 2. 交互方案

连接设置应显示从 FileTerm 到目标的完整路径，并能辨认每个跳板的上游路径。MVP 的路径由
profile 引用解析得到，例如 `FileTerm → A → B → Target`；不提供任意协议层混排或拖动排序。

连接应在最终目标上完成 SSH 握手，并保留终端、SFTP、MFA、主机指纹和会话生命周期。现有
SSH-over-`direct-tcpip` 通道可承载各跳之间的 SSH 流量。

## 3. 实施范围

### SSH 后端

- 将当前“禁止嵌套”的逻辑改为解析 profile 链，并在任何网络连接前校验：只允许 SSH profile、
  所有引用都存在、不能自引用或形成环；对链长设固定上限（建议最多 8 个跳板）。
- 从最外层跳板开始逐层建立 SSH 会话；每一跳通过上一跳的 `direct-tcpip` channel 建立，
  最终目标仍是当前 workspace 的 SSH 会话。目标会话建立后，现有 shell、SFTP、资源监控和
  `-L/-R/-D` 隧道继续复用目标 session。
- 每跳独立应用旧版 `connectionOverrides` 中的空密码和兼容算法选项，不将一跳的选项
  传给下一跳；解析结果不修改保存记录。
- 每个 hop 保留自己的认证、MFA 和 host-key 校验。交互事件中的 `hopIndex` 按真实链路
  从 0 递增；各跳板标记为 `jump-host`，最终目标标记为 `target`，直连标记为 `direct`。
- 连接取消、某一跳认证失败、超时或目标握手失败时，回收已经打开的各层连接；最后一跳断开
  时也确认外层连接不会遗留。
- 每个 profile 仍遵守现有 proxy/jump 互斥规则。MVP 只承诺透明 SSH `direct-tcpip`；
  不兼容只提供交互菜单的堡垒机，也不把 ProxyCommand、SOCKS/HTTP 与 SSH 跳板任意混排。

### Renderer 与 profile

- 保留 `SshProfile.jumpProfileId`，不做 profile schema 迁移，旧单跳配置继续有效。
- 在连接网络设置显示完整 route breadcrumb（例如 `FileTerm → A → B → Target`），并让
  下拉项区分直连跳板和已有上游路径的跳板。
- 选择跳板时排除当前 profile 和会形成循环的候选项；profile 被删除或链路失效时，显示明确
  的缺失跳板错误，不级联删除其他连接。
- 工作区底部通过 `activeJumpHosts` 显示整条已解析的跳板链，tooltip 包含各跳地址和端口。
- 测试连接使用编辑中的目标配置和凭据，不用保存记录覆盖当前表单；正常打开和重连继续
  使用最新保存配置。

主要落点：

- `apps/tauri/src-tauri/src/sessions/ssh/transport/route.rs`（路径解析、顺序连接、会话清理）
- `apps/tauri/src-tauri/src/sessions/ssh/transport/session.rs`（单跳握手和认证）
- `apps/tauri/src-tauri/src/sessions/ssh/shared.rs`
- `apps/tauri/src-tauri/src/sessions/ssh/worker/loop.rs`
- `apps/tauri/src/renderer/features/connections/connection-modal.tsx`
- `apps/tauri/src/renderer/features/connections/connection-network-section.tsx`
- `apps/tauri/src/renderer/features/layout/app-main-workspace.tsx` 与 transfer 路径摘要
- `apps/tauri/src-tauri/src/sessions/ssh/transport/tests/route.rs`
- `apps/tauri/src/renderer/app/ssh-jump-route.ts` 与 `apps/tauri/tests/renderer/ssh-jump-route.test.mjs`
- `apps/tauri/src-tauri/src/commands/ui_preferences.rs` 与测试连接调用方

已同步 `docs/architecture.md`、`docs/roadmap.md` 和
`docs/quality/tauri-rc-protocol-checklist.md` 的多级验收边界。

## 4. 验收

自动化覆盖：

- 单跳旧配置仍可连接；两跳、三跳按 A → B → target 的顺序到达正确主机。
- 每跳使用不同账号/密码或密钥时，凭据不会串到其他跳；未知/变化 host key 在对应 hop
  触发确认。
- 自引用、A → B → A、超过上限、跳板已删除、引用非 SSH profile 都在开网前失败并给出明确
  错误。
- 中间跳拒绝 `direct-tcpip`、认证失败、连接取消和目标不可达时能清理已有连接；成功连接
  后关闭标签和重连也不遗留跳板 session。
- 最终目标的 terminal、SFTP、重连和目标上的运行时 `-L/-R/-D` 正常；中间跳仅承载 SSH 流量。

已在现有 OpenSSH fixture 基础上使用四个临时 sshd，覆盖直连、单跳、两跳、三跳、
每跳不同密钥、目标执行/SFTP、失败和取消回收；新增前后端路径解析和连接表单渲染测试。
类型、lint、renderer、Tauri/Rust 测试与 Clippy 已通过；renderer 为 66 项，Rust/contract
为 762 项。本次改动的格式检查通过。真实 Windows/WSL 手测需记录平台和 FileTerm commit，不能由本机自动化代替。

## 5. Windows 本地测试拓扑

优先用 Docker Compose 起三个 OpenSSH 容器。它在 Windows 的 WSL2 容器环境和
macOS 都能复用；FileTerm 主机只连接映射到 `127.0.0.1:2221` 的入口机，后续跳转由容器网络
内的服务名解析，不需要给 Windows 配三条路由。中间 sshd 必须允许 `AllowTcpForwarding yes`
和目标端口转发。

在空目录创建以下三个文件：

`Dockerfile`：

```dockerfile
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends openssh-server \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -m -s /bin/bash lab \
 && mkdir -p /run/sshd \
 && printf 'Port 22\nListenAddress 0.0.0.0\nHostKey /etc/ssh/ssh_host_ed25519_key\nUsePAM no\nPermitRootLogin no\nPasswordAuthentication yes\nKbdInteractiveAuthentication no\nAllowTcpForwarding yes\nPermitOpen any\nSubsystem sftp internal-sftp\n' > /etc/ssh/sshd_config
COPY start-sshd.sh /usr/local/bin/start-sshd
RUN chmod +x /usr/local/bin/start-sshd
CMD ["/usr/local/bin/start-sshd"]
```

`start-sshd.sh`：

```sh
#!/bin/sh
set -eu
printf 'lab:%s\n' "$LAB_PASSWORD" | chpasswd
ssh-keygen -A
exec /usr/sbin/sshd -D -e
```

`compose.yaml`：

```yaml
services:
  entry:
    build: .
    hostname: entry
    environment:
      LAB_PASSWORD: jump-a-pass
    ports:
      - '127.0.0.1:2221:22'
  middle:
    build: .
    hostname: middle
    environment:
      LAB_PASSWORD: jump-b-pass
    ports:
      - '127.0.0.1:2222:22'
  target:
    build: .
    hostname: target
    environment:
      LAB_PASSWORD: target-pass
    ports:
      - '127.0.0.1:2223:22'
```

启动和清理：

```sh
docker compose up --build -d
docker compose down
```

在 FileTerm 建三个 SSH profile：

| Profile  | Host / Port      | User / Password       | Route        |
| -------- | ---------------- | --------------------- | ------------ |
| Entry A  | `127.0.0.1:2221` | `lab` / `jump-a-pass` | Direct       |
| Middle B | `middle:22`      | `lab` / `jump-b-pass` | Via Entry A  |
| Target   | `target:22`      | `lab` / `target-pass` | Via Middle B |

再打开 Target，逐个接受三台不同 host key；终端运行 `hostname`，应显示 `target`，随后验证
SFTP 目录、关闭标签和重新连接。测试密码只能用于这套绑定在回环地址的隔离环境。

也可以在一份 WSL2 Ubuntu 中启动三份不同端口的 sshd，减少 Windows 路由配置；不需要多开
三个发行版。WSL2 默认会将 Linux 服务端口转发到 Windows `localhost`；Windows 11 22H2+
还可以配置 mirrored networking。多个发行版之间互访和防火墙仍会增加变量，故 Docker
Compose 更适合作为第一套回归环境。参考 [Microsoft WSL networking 文档](https://learn.microsoft.com/windows/wsl/networking)。
