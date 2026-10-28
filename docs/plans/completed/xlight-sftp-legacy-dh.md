# Xlight SFTP 旧 DH 组兼容

## 问题与边界

v2.3.2-beta.2 的 `legacyAlgorithms` 追加 SHA-1 算法，但普通服务器的 GEX
仍请求 3072/8192/8192；返回 1024 位 DH 组时，在认证前失败。
已有 Comware 特例不覆盖此情况。

仅在明确开启兼容选项时，应用层请求 1024/8192/8192，覆盖 GEX-SHA1 和
GEX-SHA256；默认策略、现代算法优先级、主机密钥校验和 Comware 特例不变。
界面中英提示明确包含较弱的 1024 位 DH。无需修改 vendor 默认值或图标代码。

## 实施与验证

1. 提取生产算法配置，使用真实回环 TCP 握手先复现失败。
2. 设置显式兼容模式的 GEX 范围，运行默认拒绝、兼容成功、现代组与 Comware 回归。
3. 更新提示和协议约束，运行仓库质量门禁。
4. 协议夹具不等于真实 Xlight Windows Server 验收；记录未验证项。

## 完成结果

- 算法配置从 `transport/session.rs` 提取到 `transport/algorithms.rs`，实际连接和
  测试共同使用；vendor、图标逻辑及主机密钥验证未修改。
- 六项回归测试覆盖真实 TCP 握手：1024 位组在两种 GEX 哈希下兼容成功、默认拒绝、
  512/8200 位参数仍拒绝、4096 位组两种模式成功、Comware 保留原请求及现代算法优先。
  修复前测试复现 `KexInit`，修复后通过。
- Debian 13 容器中通过 typecheck、lint、Prettier、215 项 renderer 测试、
  `npm run test:tauri` 及全 targets/features Clippy（warnings denied）。
  首轮全量 Rust 测试因缺少 OpenSSH 和 init 出现环境失败；安装 OpenSSH、使用
  `tini -s` 回收孤儿进程后完整重跑通过，未跳过失败用例。
- 尚未验证 Windows 安装包和受影响用户的真实 Xlight 版本；固定组夹具只验证该
  握手失败路径。现有真实 OpenSSH 认证/SFTP/代理/跳板测试通过。

## PR 主分支适配

原 beta.2 标签还包含两个侧栏提交，与当前 main 比较会带入无关改动并冲突。
PR 分支改为从 `a7ccb060` 创建，仅应用本 SFTP 修复；相对 main 为一个提交、七个文件。
在该基础上重新通过全部门禁：939 项 Rust/契约测试（906 + 10 + 23）、239 项
renderer 测试，以及类型、Lint、格式和 Clippy 检查。main 新增的只读目录测试
需要普通用户权限；root 首轮执行失败，改用容器内普通用户后全量重跑通过。
