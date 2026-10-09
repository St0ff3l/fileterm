# sudo/su 与缺失 Home 的 CWD 跟随

日期：2026-10-09。

## 已确认的原因

- 第一张截图对应日志已经收到 `/root` 和 `RemoteUser=root`，但 sudo/su 使用相同密码时，捕获层只比较密码内容，未通知 worker 更新 su 的独立缓存。后台 su 读取没有密码，因此等待认证超时并保留旧文件列表。
- 第二张截图的会话日志只有初始 CWD，没有识别到提权命令。原始按键无法表示历史召回、粘贴或行编辑后的完整命令，因此新增从 PTY 已提交命令回显中识别 sudo/su。
- 原两秒 hook 重注入冷却会丢掉快速提权后的首次提示符；已有 root 身份也不能证明新登录 shell 保留旧 hook。改为每次已识别的交互式提权只安装一次，提示符/身份确认后消费该次状态。

## 最终边界

- 按远端 `pwd -P` 与 RemoteUser 同步；不按用户名拼 Home，也不要求 `/home/root` 或 `/root` 存在。
- Home 缺失但实际 CWD 有效时保留 CWD；权限切换即使 CWD 相同，也更新 root 文件访问方式并刷新。
- 正常提示符结束前一个命令的认证/echo 状态，防止旧 sudo 影响后续 history su。只记录方式/目标用户，不记录命令回显或凭据。
- 保留普通 `#` 不触发注入、网络设备/平台门控、sudo/su 密码分离和同目录静默更新。

## 验证范围

- 回归覆盖相同密码的 sudo→su / su→sudo、输入和密码提示两种到达顺序、历史召回命令、一次 hook 安装、缺失 root Home 时实际 CWD 与 root 身份。
- 在用户指定主机通过独立 SSH 诊断会话验证 Bash 5.3 的空回车/命令标记、sudo 和 su 登录后的真实路径、安装同一 hook 后 `cd /tmp` / `cd /` 的 OSC7 与 root 用户标记，以及 su 独立命令可读取 `/root`。未修改远端账户/Home/权限，诊断会话已退出。
- 开发窗口无法通过 Computer Use 解析 `com.fileterm.desktop.dev`，应用清单也没有对应条目；未声明桌面端 UI 已实测通过。

## 最终检查

- Tauri 全量 743 项通过：710 单元测试、10 与 23 项集成/contract 测试。
- Tauri typecheck、lint、指定范围 Prettier、Rust fmt、Clippy 与 diff 空白检查全部通过。
- 桌面端 hook 在连接/新交互式登录 shell 中安装，旧会话需重新连接后验收；FileTerm MCP 同样返回 APP_UNAVAILABLE，未绕过开发窗口身份要求操作安装版。
- 代码尚未提交或发布；未关闭任何 Issue。

## 后续 su 截图回归

- 会话 `tab-eb9c8c56-034a-4b6d-91c0-0d224615a709` 的 sudo 身份和 `/usr/bin` CWD 已上报；最初 su 没有进入命令跟踪。
- 修复可复现的解析漏洞：认证回显缓冲保留原始控制序列，在完整缓冲上去除 CSI/OSC，避免控制序列跨包时残片污染 `su` 命令。输入解析处理焦点事件、退格和 Ctrl+U 编辑。
- 新回归遍历 sudo/su 回显的每个分包位置，并覆盖焦点事件和编辑后提交。原生桌面 su 操作尚待实机确认，不将解析测试等同于截图场景验收。

- 此轮完整门禁通过：715 单元测试 + 10 CLI + 23 contract（共 748 项），Clippy、类型、Lint、格式与 diff 检查通过。
