# Windows 现场日志：修补与诊断补充（2026-10-21）

本记录引用的筛选日志、环境数据及浏览器原始结果已移到项目外部归档；位置和文件清单见
[Windows renderer 归档说明](terminal-renderer-windows-2026-10-20/README.md#artifacts)。

## 旧日志确认了什么

输入是用户提供的 `app-log(1).md` 分析报告和其中选录的原始条目，不是完整两份 app.log。

| 事实                                                                            | 能支持的判断                                                 | 不能据此判断                                                                 |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------ | ---------------------------------------------------------------------------- |
| 最忙 WebView2 renderer 约 105–112% 单核 CPU，部分分钟长任务合计约 28.9 秒       | renderer 持续高负载，需分开解析、绘制、附加布局与 React 更新 | 不能只凭进程 CPU 把所有消耗归给 DOM 或某个适配器                             |
| 心跳间隔 9855ms、输入延迟 1821ms、单批解析/完成链路 1022ms、显示额度等待 7704ms | 发生严重响应停顿；解析/确认链路在该时段受到主线程拥塞影响    | 不能把相邻时间关系当作特定函数的因果证明；约 1000ms 的常态心跳不是一秒卡顿   |
| 同期 React #185，栈含 `_deliver` / `fire`                                       | 有更新深度超限异常，事件回调是排查入口                       | xterm 自带的 VS Code 事件基础层也有这些符号，不能归定为 Tauri/workspace 回调 |
| 多数时段 queued=0、parseFailures/ackFailures=0；JS heap 有回落                  | 这些采样内没有持续 Rust 队列积压或已报告的解析/ACK 失败      | 不能排除停顿期间背压、确认迟到、长期内存增长或泄漏                           |

用户栈的发布文件为 `index-DcpQ0jto.js`。已有[更新链审计](react-update-loop-audit.md#2026-10-20长会话日志与监控图表的重复更新)记录了同文件名重建与 source map 对齐：抛错 setter 是 xterm onData 中的 `setContextMenu(null)`，两层 Emitter 属于 xterm。不能用不同产物的行列号定位，也不能从这个抛错点推断此前全部更新来源。监控图表的上游重复更新已在合成提交洪峰中复现并修补；Windows 现场唯一原因仍未确认。

## 本轮已实现

1. xterm 选区、搜索结果不再直接在同步 emitter 回调里更新 React。按帧保留最新选区及搜索结果，统一发布；快速关闭查找时丢弃待发布的旧结果，卸载取消 pending frame。没有改解析完成/ACK 的等待条件，不丢终端输出。
2. 日志着色复用未变化行的正则匹配和单元映射，文本、列数或调色板变化会失效；仍检查/修补当前前景色，不直接跳过 ANSI 内容校验。WeakMap 不强持有已释放的 BufferLine，最多 256 次缓存未命中后清空，避免额外文本/范围缓存扩展到整个历史。reset/clear、列数变化和 dispose 清理缓存。
3. 新增命名事件边界/更新计算计时、失败阶段和活动阶段摘要。异常同时附带按后端及内部 tab ID 汇总的终端资源，renderer 初始化/回退日志带内部 tab ID。bridge 异步就绪后一次性报告诊断 schema 2。
4. 已从真实 app.log 观察到 `renderer-diagnostics-ready` 和带 schema 2、命名阶段、`byRenderer` / `perTerminal` 的健康记录。异常字段通过单元测试验证；未人为触发用户窗口崩溃。

第一项隔离同步嵌套与事件风暴风险，第二项减少实际重复计算；它们不构成旧 #185 已复现修复的证明。该日志修补记录形成时仍默认 DOM；当前三平台默认状态见 [renderer 验证报告](terminal-dom-webgl-comparison-2026-10-20.md)。

## 自动验证

- 完整 `npm run typecheck`、171 个 renderer 测试通过；当前缓存上限的后续改动由受影响 tsc/测试再次检查。
- 1000 次搜索/选区事件仅排队一个 UI 发布 frame，保留最后值；销毁后不再发布。
- 一个四字符行的 101 次相同输出，逐格映射从原算法的 404 次降到 4 次；文本与调色板变化会重新映射，600 次调色板变更仍遵守缓存预算。这里只是操作计数，不是用户长任务耗时改善率。
- 实际 WebView2 155、125% DPI 的 1502 行、12/14/16px 中文/符号/英文/数字混排、选择/搜索、context loss DOM 回退、三次创建/销毁通过；独立临时终端不发送 PTY 命令。
- Windows Chromium 的一对 1000 批对照完成，解析及模拟 ACK 全完成，最终积压为零，无着色失败或 context loss。原生 DOM/WebGL 网格分别 fit；只是自动合成负载，不是实际 WebView2/Claude Code 长任务，也不是修补前后 A/B。
- 定向 ESLint / Prettier、CSS contract 通过。全仓 lint 仍有 77340 条 `prettier/prettier` 历史格式错误；全范围 Prettier 有 482 个文件未通过，不重写无关文件。
- 严格 all-targets/all-features clippy 通过。`npm run test:tauri` 因运行中的 `target/debug/fileterm.exe` 被 Windows 锁定，无法覆盖该文件而失败；保留当前开发进程，改跑 `cargo test --locked --lib`：801 通过、1 忽略；直接执行本轮生成的 CLI/contract 测试程序：10/23 通过，既有 scheduler 测试程序：3 通过。主门禁命令仍标记受进程锁限制。

开发进程为当前工作目录的 PID 21628，Vite 5188、调试端口仅本机 9223，源码为 PR 分支 HEAD 加未提交修改。完整类型门禁本轮没有重启原生进程；Vite 源码/共享包构建期间可能刷新 renderer。

## 原始材料

数据目录：[terminal-renderer-windows-2026-10-20](terminal-renderer-windows-2026-10-20/README.md)。

- `user-log-stall-excerpt.log`：用户分析报告内选录的 WARN/ERROR，未声称是完整现场日志。
- `terminal-log-repair-diagnostics.log`：实际 app.log 中 schema 2 启动、健康及实验 renderer 事件的摘录。
- `log-repair-environment.json`：分支/HEAD、进程与端口、源文件 SHA-256、输入分析报告 SHA-256。
- `terminal-log-repair-comparison.json`、`terminal-log-repair-lifecycle.json`、`tauri-webview2-log-repair.json`：自动测试原始数据。修改后复测数据不与之前小数网格候选混算。

## 下一次真实长测

1. 在当前开发版建立与现场相同数量的连接，先确认字体、DPI 和终端大小不变；用同一组 Claude Code 长任务，保留监控侧栏状态，不因测试主动修改远端业务数据。
2. 连续运行至少 30–60 分钟。发生输入迟滞/冻结时记下本地时间、活动 tab、是否搜索或拖动选区、是否正在 resize、是否切屏；同时记录 CPU/进程树内存，而不只看 RSS。
3. 出现问题后尽快保留 `app.log` 和 `app.log.1` 两份，日志只保留一个轮转备份。先查 `renderer-diagnostics-ready` / `schemaVersion:3`，再把 `renderer:error.work.recentFailures`、异常时解析/ACK、同分钟阶段耗时、Long Tasks / Frames 和每终端积压串起来；没有字段或失败记录时明确记为未捕获，不能当作无故障。

真实 #185 复现、长任务输入延迟改善、跨分钟内存趋势仍待现场验证。当前无 RSS 或短测证据可以证明泄漏已解决。

## 2026-10-21：轮转日志卡顿时间线复核

在 FileTerm 的“设置 → 日志”中打开日志目录，检查 `app.log` 与 `app.log.1`。旧日志的一次 WebGL 双终端运行留下了清楚的卡顿窗口：新加坡时间 03:06:38.228–03:06:50.815，连续三个 WebView Long Animation Frame 分别约 3.879、4.407、4.256 秒。其 `scriptCount=0`、`blockingMs=0`、渲染/布局阶段约 0–1ms；同一健康样本显示两个 WebGL 终端，终端解析/ACK 队列为零、无失败、交互样本为零。这里的 frame 起点可用 `work.timeOriginUnixMs + longFrames.slowest[].startedAtMs` 还原。

`renderer-long-frame` 的 30 秒限频只留下首条 WARN，后续两帧保存在下一条分钟健康汇总中。旧原生采集白名单也没有接收这个 WARN，因此没有与卡顿重合的 ETW 窗口。

卡顿前约 22 秒及后约 44 秒的进程树样本（每次覆盖约 60 秒）合计单核 CPU 约 1.5–1.8%，private bytes 约 519 MiB、working set 约 701 MiB；这些采样粒度无法代表 12.6 秒卡顿窗口。Windows scheduler ETW 为 `status=unavailable`、`errorCode=1300`、`requiresElevation=true`，所以本次没有线程调度证据。现有数据能证明 WebView 帧停顿发生的时间，不能判定 GPU、合成器或系统调度中的根因。

当前 `app.log` 的 213 条健康采样没有对应的卡顿 WARN：最大长帧 138ms、最长 Long Task 84ms、输入派发延迟 12ms、ACK 最长 73ms；心跳最大间隔 1052ms（包含正常的一秒间隔）。最新健康样本为 DOM。检查时没有发现 FileTerm 主进程，因此这只是历史日志分析，不能确认当前打开窗口的构建。

已补充后续采集：Event Timing 汇总键盘、指针按下/抬起、文本输入、IME 和粘贴事件的最大事件类型及开始时间；输入派发延迟达到 250ms 即由现有心跳触发限频 WARN；可见页面心跳间隔阈值从 4 秒降至 2 秒，告警同时给出最后心跳、预期心跳和恢复时间锚点。Windows 调度采集现也接受 `renderer-long-frame` 和 `renderer-progress-slow`，因此后续同类帧停顿会触发短时 ETW 尝试。仍不记录输入文本，也不增加高频日志。

本轮验证：Tauri 类型检查、173 个 renderer 测试、802 个 Rust 单测（1 个忽略）、10 个 CLI 测试、23 个 contract 测试和 3 个 Windows scheduler 测试通过；改动文件的 ESLint、Prettier、rustfmt 与 `git diff --check` 通过。全仓 lint / Prettier 仍被大量无关 CRLF 格式问题阻塞；严格 Clippy 在当前机器因 AWS-LC 缺少 NASM / CMake 依赖无法构建。检查时 FileTerm 未运行，因此尚未在新构建的真实 WebView2 中观察这些字段。

## 2026-10-21：beta10 与本机 beta11 日志交叉复核

再次读取用户提供的 beta10 分析报告，并对本机读取时尚保留的两份轮转文件做独立筛选。输入 SHA-256、字节数和统计保存在项目外的 `local-log-followup-summary.json`；数值诊断原文保存在 `local-log-followup-filtered.log`，源码哈希和门禁状态在 `log-followup-environment.json`。归档仍使用上述仓库同级目录，未把完整业务日志或个人绝对路径加入仓库。

本机这次可读片段有 224 条 beta11 健康样本，未发现 renderer ERROR 或性能 WARN；采样中的待解析/待 ACK 数均为零，累计 ACK 最大耗时 256ms，输入派发最大延迟 12ms、长任务最大 136ms、已计时 xterm 解析/绘制最大 15/23ms。最新样本为一个 WebGL 终端。这是轮转后保留下来的低负载历史片段，不是当前窗口身份确认或多会话重绘压力测试。此前归档的无脚本归因 3.879–4.407 秒长帧仍未解释；新片段没有该事件不能推翻旧记录，也不能证明泄漏已解决。

本轮确认并修补了一处遗漏：已连接终端的自动 CPR/DA/颜色/状态查询回复原先经过活动标签、IME 过滤、清选区和 `setContextMenu(null)`。所以后台回复会丢失，前台自动回复会无谓进入 React 更新链。现在这些已识别的协议回复直接进入现有输入发送队列，串口协议占用时仍阻止写入；历史回放抑制和初始 ConPTY 握手规则保留。普通键盘与 focus tracking 行为不变。该缺陷由回调与真实 xterm 查询回归确认，但没有证据说明它单独造成了旧现场全部 #185 或 9.9 秒停顿。

诊断升级为 schema 3：`terminal-input` 记录 xterm 输入回调的同步耗时和失败阶段；异常报告增加发生时解析/ACK 状态、采集时间、可见/焦点状态；性能告警与健康报告保留最慢四个长任务的时间与持续量。最近四次失败跨分钟/可见性重置保留，避免心跳先清空证据、xterm 定时器随后才重抛；时间戳用于区分旧故障。仍使用现有心跳、分钟汇总和 30 秒 WARN 限频，不记录输入内容，也不增加高频日志。Long Tasks API 宣称可用但拒绝观察时明确降级。字段边界见 [日志文档](logging.md#schemaversion-3输入回调异常积压与长任务时间线)。

新增回归覆盖协议回复绕过 React/IME、后台回复、普通键盘、串口占用、断连、历史回放和 ConPTY 初始回复；异常日志与长任务时间线也有定向测试。生产 React 与真实 xterm 的 Chromium 夹具分别以 DOM/WebGL 完成 360 次监控/输入提交、菜单打开时查询、后台模拟 composition 查询和 200 批解析/ACK，卸载订阅清零，无 pageerror。原动画断言有一次固定 150ms 等待失败，复跑通过；改为等待实际 SVG transform 进度，避免依赖调度时间。结果见 `log-followup-monitoring-{dom,webgl}.stdout.log` / `.stderr.log`。

本轮完整类型检查、179 项 renderer 测试、803 项 Rust lib 测试（另 1 项忽略）、CSS contract 和改动文件静态/格式检查通过。全仓 lint / Prettier 仍受既有 CRLF 格式问题阻塞；完整 `npm run test:tauri` 被运行中开发版的 exe 文件锁阻塞，保留原进程；all-targets/all-features 严格 Clippy 因 AWS-LC 缺少 NASM 而构建失败。门禁输出归档为 `log-followup-gate-*.log`，没有把未完成门禁记为通过。

本轮仍未在真实 Windows WebView2 上复现旧 #185，也未完成实际 Claude Code 长任务和物理 IME 验证。无脚本长帧仍需要与卡顿重合的原生线程/GPU 证据，不能靠增加前端数字推断根因。
