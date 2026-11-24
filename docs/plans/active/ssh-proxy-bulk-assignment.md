# 一期 · 批量分配 SSH 代理

状态：待实施。初估 2–4 个工作日，含本地回归和一次 Windows 实机验收。实现完成后记录检查结果，移入 `docs/plans/completed/`。有正式 Issue 编号后使用 `Refs #编号` 关联。

## 用户场景与交付范围

用户在 Windows 同时运行两个代理端点，通过 FileTerm 把不同 SSH 连接分给不同代理。每个代理可以关联多台服务器，换出口时在「代理与隧道」集中修改，无需逐台填写相同的地址和认证信息。目标服务器能由所选代理直接访问时，可以省掉额外的服务器转发。

一期适用于手动建立的普通 SSH 连接，复用 SOCKS5、HTTP CONNECT。独立 SFTP 配置、FTP、OpenSSH 配置别名、隧道配置和拓扑编辑入口留在现有流程。SSH 工作区内复用该会话的文件访问仍随 SSH 会话走。每台连接同时只引用一条代理。网卡选择见[二期计划](windows-ssh-network-interface.md)。

## 可复用的实现

- [代理管理页](../../../apps/tauri/src/renderer/features/proxies/proxy-manager-page.tsx)列出代理和隧道，[首页工作区](../../../apps/tauri/src/renderer/features/workspace/home-workspace.tsx)已有脱敏的连接列表。
- [连接代理表单](../../../apps/tauri/src/renderer/features/connections/connection-proxy-section.tsx)已有直连、单独填写、引用已保存代理三种状态。真实关联使用 `ConnectionProfile.proxyProfileId`，代理库已经返回引用数和连接名，两边无需分别存一份关联关系。
- [代理服务](../../../apps/tauri/src-tauri/src/services/proxies.rs)已有代理 CRUD 与引用删除保护。[SSH 运行时入口](../../../apps/tauri/src-tauri/src/commands/ui_preferences.rs)会加载共享代理定义，[SSH 传输](../../../apps/tauri/src-tauri/src/sessions/ssh/transport/proxy.rs)已有拨号与认证逻辑。
- [连接写入命令](../../../apps/tauri/src-tauri/src/commands/profile_commands.rs)和[代理写入命令](../../../apps/tauri/src-tauri/src/commands/proxy_commands.rs)共用 `lock_library_after_transfer_hydration`，可用同一把锁串行化分配、单连接编辑、代理删除与导入。

## 实施步骤

### 1. 确定分配规则

- 每条代理增加「管理关联连接」。候选列表显示连接名、分组、主机和当前出口，用连接 ID 区分重名项。关联到当前代理的连接默认勾选，搜索只过滤显示范围。切换筛选条件或搜索时保留尚未保存的选择，取消弹窗则放弃草稿。
- 勾选直连连接即分配。勾选已有其他代理或自定义代理的连接时，保存前显示原出口和新出口，确认后替换。原自定义代理凭据随替换清除，确认文案写清楚。取消勾选已关联到当前代理的连接后恢复直连，重新关联可继续使用共享代理的认证。
- 配置了跳板的连接显示不可选原因，保留原路线。兼容旧 `jumpProfileId` 和新 `jumpProfileIds` 的优先语义。对于在其他连接路线里充当跳板的候选，允许作为首跳使用代理，拒绝会导致后续跳板与代理冲突的分配。上下游检查以 [Rust 路由规则](../../../apps/tauri/src-tauri/src/sessions/ssh/transport/route.rs)为准。
- 保存时只提交此次勾选发生变化的连接。成功后提示配置用于从连接库新建的会话。手动重连验证是否重新读取连接库并体现最新分配。已在线会话继续使用当前传输，后台自动重连沿用现有生命周期，不为一期改造会话重连策略。

### 2. 实现 Rust 批量写入和 IPC

- 在 `services/profile_ops` 增加仅修改代理字段的批量操作，在 `commands/profile_commands.rs` 增加批量命令并在 `lib/runtime.rs` 注册。输入包含目标代理 ID 和变更项，每项含连接 ID、操作和确认时看到的原代理状态。原状态使用脱敏字段区分直连、共享代理 ID、自定义代理类型和端点，凭据留在 Rust。
- 同一把库锁内读取最新连接和代理，校验目标代理存在、连接 ID 唯一且均为受支持的 SSH 配置，校验原状态与跳板规则，再应用到同一份连接库。原状态或资格已变化时整批失败，提示刷新后重试。重复请求不得覆盖新变更。前端弹窗打开期间其他窗口新加的关联通过增量输入保留。
- 使用现有持久化与脱敏路径一次提交整批。优先复用 `persist_profiles` 的公开文件与凭据回滚逻辑，保留分组、排序、私有备份身份、主机指纹以及 SSH/sudo/su 等非代理凭据。清掉被替换的连接级代理密码。尤其注意 `read_and_heal_profiles` 会把共享代理定义和密码水化到内存的 `proxy` 中，提交时不得把共享密码写成连接级秘密。共享定义仍由 `proxyProfileId` 在运行时解析。`build_profile_secrets` 同时供保存和读取自愈使用，检查两条路径，避免保存后下一次快照又写回共享密码。覆盖解绑、共享代理换绑和未变更连接的凭据回归。
- 成功提交后沿用 workspace snapshot 更新连接，并发出既有 `proxies:changed` 事件刷新引用计数。区分写入失败与写入成功后的刷新失败，后者提示重新加载，避免对已提交批次盲目重试。
- 在 [共享类型](../../../packages/core/src/index.ts)和 [Tauri bridge](../../../apps/tauri/src/bridge/tauri-api.ts)接入类型化方法。返回现有脱敏 workspace snapshot 或沿用其事件机制。

### 3. 实现管理界面

- 从 `HomeWorkspace` 把现有连接列表传入代理管理页，仅给 SOCKS5 / HTTP CONNECT 行增加入口。在 `features/proxies/` 实现关联弹窗，复用项目对话框、列表和滚动组件。
- 展示连接筛选、已选数量、当前出口、冲突说明及保存前变更摘要。提交期间阻止重复保存。校验失败时保留用户选择并展示原因，刷新后重新确认有变化的行。同步项目现有语言文案。
- 保存后核对代理页引用数、连接编辑器选项、快照和拓扑中已支持的代理关系。拓扑沿用现有渲染逻辑。

## 验证与完成条件

- [ ] Rust 回归覆盖同批全部成功、存在无效项时全部拒绝、陈旧修改、重名 ID 区分、代理删除冲突、旧/新跳板及首跳与后续跳板的差别。验证所有未编辑字段和凭据保留，公开快照无代理密码。
- [ ] 验证已有 SOCKS5 和 HTTP CONNECT（含认证）、直连、有跳板连接仍按原规则工作。使用仓库现有 SSH 软件夹具覆盖可自动化的路径。
- [ ] 桌面验收覆盖筛选、多选、换绑确认、解绑、重复点击和两个窗口之间的更新。按[开发版识别规则](../../quality/desktop-ui-preview.md)绑定当前进程。
- [ ] Windows 上启动两个可区分的 SOCKS5 / HTTP CONNECT 端点。给两台服务器分配不同代理，同时建立 SSH，结合代理日志核实各自经过指定端点。验证第三台复用其中一个代理、变更后主动重连，以及代理关闭时连接失败。现有「测试代理」仅用于端点检查，真实 SSH 建连才计入验收。
- [ ] 跑完 [AGENTS.md](../../../AGENTS.md) 的代码质量门禁，记录实机网络、代理类型、验证结果和遗留项。
