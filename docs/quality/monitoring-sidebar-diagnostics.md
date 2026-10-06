# 监控侧栏诊断日志（Related to #259）

用于排查同一窗口中某个 SSH 标签无法展开系统侧栏，以及 CPU 总览与进程榜单的差异。
日志补充不代表问题已经修复；Issue 保持打开，等待发布后的复现和确认。

## 收集方式

1. 切到出问题的 SSH 标签，点击侧栏展开按钮。
2. 切到能正常展开的标签，展开侧栏，再切回出问题的标签。
3. 如果同时有 CPU 数据差异，保持连接至少一分钟，记录同一时刻的 top/htop 结果。
4. 从“设置 → 系统与日志”打开日志目录，提供上述时段的 app.log。

安装版与开发版共用日志目录，但 bundle ID 不同。收集日志时请同时记录问题发生时间和应用版本，以便区分运行中的版本；Windows portable 仍使用 exe 旁的独立配置目录。

## 日志含义

- `sidebar action`：按钮实际收到了点击；记录当前实际折叠状态、连接状态和监控状态。
- `sidebar collapse requested`：标签状态层接收到了展开/收起请求；记录原来的用户折叠状态和目标值。
- `resource monitoring UI state`：工作区最终的可见性、监控可用性和折叠原因。监控配置禁用、远端能力禁用或网络设备模式不再强制折叠侧栏；折叠原因只包含专注模式或用户主动折叠。
- `sidebar rendered`：侧栏组件渲染结果、监控 generation/phase/attempt 和是否已有指标。
- `sample summary`：首个健康样本、切换回标签后的样本，以及同一标签和 generation 下最多每分钟一条的指标摘要。

样本摘要包含远端平台、逻辑核心数、CPU 总览、当前进程榜单行数/CPU 合计/最大值、磁盘和网卡行数、样本年龄。
进程榜单仅是当前返回的有限列表，CPU 合计不能直接等同于整机 CPU 使用率。

所有新增日志使用 INFO 级别，写入现有 Rust 日志服务。
不记录连接地址、用户名、密码、密钥、进程命令行或远端原始输出。

## 初步判断

- 没有 `sidebar action`：检查按钮命中区域和遮挡层。
- 有 action 但没有 collapse requested：检查事件到标签状态层的调用。
- 请求展开后仍折叠：检查 `collapse_reason`，区分用户折叠与工作区强制折叠。
- 点击展开会退出当前标签的专注模式，并清除该标签的用户折叠状态；其他标签保持原状态。监控不可用时仍可查看连接摘要。
- `has_metrics=false` 或 phase 长期不健康：结合相同 tab_id 的 SSH collector 启动、通道失败、重试和样本日志排查。
- 指标正常但界面未更新：结合 renderer workspace snapshot/session metrics 日志检查状态传递。

## 2.2.21 日志确认的故障

- Armbian 的 `osName` 为 `Armbian ...`，旧发行版名称列表没有该标记，导致有效 Linux 身份被拒绝为 `target-identity-invalid`。除补充该标记，还采集 `os-release` 的 `ID` / `ID_LIKE`，避免仅根据显示名称漏掉 Linux 派生系统；保留旧采集结果兼容、未知身份与 JumpServer 拒绝规则。
- React #185 的组件栈可包含终端右键菜单，应用栈指向通用菜单的布局定位更新。开发版复测表明相同坐标保留原 state 的保护仍不足；最终删除定位 state，测量后直接定位菜单 DOM。关闭回调变化也不能触发监听器清理和终端焦点恢复。
- `tests/browser/context-menu-updates.py` 验证相同坐标下 100 次父级更新、边缘定位、菜单焦点与最新 Escape 回调；回归在旧菜单实现上因额外布局更新失败。

上述代码与浏览器验证不代表已发布或已在用户 macOS WebView、真实远端验证，Issue 保持打开。
