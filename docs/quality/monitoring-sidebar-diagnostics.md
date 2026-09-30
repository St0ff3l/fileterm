# 监控侧栏诊断日志（Related to #259）

用于排查同一窗口中某个 SSH 标签无法展开系统侧栏，以及 CPU 总览与进程榜单的差异。
日志补充不代表问题已经修复；Issue 保持打开，等待发布后的复现和确认。

## 收集方式

1. 切到出问题的 SSH 标签，点击侧栏展开按钮。
2. 切到能正常展开的标签，展开侧栏，再切回出问题的标签。
3. 如果同时有 CPU 数据差异，保持连接至少一分钟，记录同一时刻的 top/htop 结果。
4. 从“设置 → 系统与日志”打开日志目录，提供上述时段的 app.log。

安装版与开发版数据目录独立，必须收集发生问题的那个应用的日志。

## 日志含义

- `sidebar action`：按钮实际收到了点击；记录当前实际折叠状态、连接状态和监控状态。
- `sidebar collapse requested`：标签状态层接收到了展开/收起请求；记录原来的用户折叠状态和目标值。
- `resource monitoring UI state`：工作区最终的可见性、折叠状态和原因，包括配置禁用、远端能力禁用、网络设备模式、焦点模式或用户主动折叠。
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
- `has_metrics=false` 或 phase 长期不健康：结合相同 tab_id 的 SSH collector 启动、通道失败、重试和样本日志排查。
- 指标正常但界面未更新：结合 renderer workspace snapshot/session metrics 日志检查状态传递。
