# DOM / WebGL 终端对照：2026-10-20

> 2026-10-21 字形回归更新：用户真实会话发现先前小数网格候选存在断笔/拖影。已删除 WebGL 小数宽度覆盖，恢复上游整数像素图集，仅保留 backing resize 同步。52 组字体/DPI 与实际 WebView2 独立临时终端验证通过；初始已有会话回退 DOM，后续门禁触发开发版自动重启，详见记录。此前候选的网格一致和短测性能数据不能作为当前候选视觉或性能验收依据。详见 [字形回归修复](terminal-renderer-windows-2026-10-20/font-regression-fix-2026-10-21.md)。
>
> 2026-10-21 renderer 设置：Windows、macOS 和 Linux 均可在“设置 → 界面 → 终端显示”选择 DOM 或 WebGL。新安装及旧 `state.json` 缺少 renderer 字段时默认 WebGL；已保存选择会保留。已打开终端不会即时切换，新建终端采用所选项；WebGL 加载失败、激活失败或 context loss 会回退 DOM。本文的原生对照数据来自 Windows WebView2；真实 Claude Code 长任务和中文 IME 仍待交互验证。

本报告先记录独立 Chromium 对照；2026-10-21 已补上真实 Tauri WebView2 中的短 smoke。
真实 PTY、输入法和 Claude Code 长测仍待交互验证。
Windows Tauri 的 renderer 由上述设置决定；`VITE_FILETERM_TERMINAL_RENDERER=dom|webgl` 仅用于开发构建的显式对照。
插件 `@xterm/addon-webgl@0.19.0` 仍只列在 Tauri workspace 的 devDependencies。实际 Claude Code 长任务和中文 IME 仍待交互验证。

## 块字形兼容：2026-10-21

> 以下为历史实现。后续已移除块字符触发的 DOM 回退，改为保留 WebGL custom glyphs，并修复共享图集清理后其他终端坐标未失效的问题。复现和当前验证见[本轮报告](terminal-webgl-fallback-shared-atlas-2026-10-21.md)。

OpenCode 截图中的像素风块字可能被 WebGL 图集按字体字面度量错位。沿用 Ghostty 按终端 cell 尺寸和 glyph bearing 定位字形的思路，FileTerm 在 WebGL 终端检测到可见 U+2580–U+259F 后，让该终端回退到现有 DOM 字形适配器；适配器按列宽和设备像素网格绘制块字，不更改文本缓冲、字体或默认 renderer。参考：[Ghostty cell text shader](https://github.com/ghostty-org/ghostty/blob/main/src/renderer/shaders/glsl/cell_text.v.glsl)。

生命周期回归使用 OpenCode 风格的块字组合，确认该终端回退 DOM、单元格字形 canvas 已绘制且缓冲文本完整。尚未在真实 OpenCode 长会话中验证其余 TUI 元素和性能影响。

## Windows WebGL 网格修复：2026-10-21

新增 terminal-webgl-grid.ts，只用于显式开发版 WebGL。适配器按现有 DOM 的字体原始度量计算
WebGL 单元格与画布尺寸，保留 CharSizeService 测量；CSS 命中网格由实际取整后的画布尺寸计算。
没有使用实验夹具的 DOM 取整修正，也没有改变 DOM、WebView zoom 或全局字体尺寸。
激活和 DOM 回退后重新 fit，避免异步切换 renderer 时保留旧 PTY 列数。

截图复核另发现 backing buffer 改变后 GL viewport/字形 resolution uniform 仍使用旧尺寸，
导致 DPI/字号切换后首行裁切。适配器在原生 backing resize 后同步 viewport、字形 uniform 和
重绘模型。未知 xterm 私有结构会令激活失败并回退 DOM；dispose/失败时恢复两处方法。

原生 DOM 对照与修正后的 WebGL 已通过 52 组字体/DPI、延迟字体、3 档 resize、首中末列命中、
CJK/emoji、选区、搜索和回放；每组检查 GL viewport 与 backing buffer 相同。125% 截图确认
首行完整，两个后端文字网格相同；字形抗锯齿不要求逐像素一致。

真实 WebView2 155.0.4283.62 / NVIDIA D3D11 复测使用实际 terminal-host → terminal-inner 容器，
内容区为 1228×716 CSS px。当前字体初始网格两侧均 183×49，JetBrains Mono 14 均 144×37；
首/中/末列命中为 1/92/183，画布在内容区以内。切回原字体后两侧均为 184×49，context loss
回退仍为 184×49；三轮销毁均释放 context 且 inner host 子节点为零。源码 SHA-256、GPU/驱动和原始
验证数据（`tauri-webview2-grid-fixed-environment.json`、`tauri-webview2-webgl-grid-fixed.json`、
`terminal-renderer-production-grid.json`）已归档在
[Windows renderer 原始数据目录](terminal-renderer-windows-2026-10-20/README.md#artifacts)。
字体往返的一列变化在原生 DOM 参照同样出现，属于 FitAddon 基于取整 CSS 格宽的既有行为；
本轮保留该行为，没有另改生产 DOM 的 fit 策略。旧 188/194、147/155 数字来自缺少实际内层容器的
早期 smoke，不能当作当前窗口尺寸；相对的 WebGL 格宽取整差异已由独立字体矩阵确认并修正。

### 生产网格的 Windows Chromium 配对复测

FILETERM_GRID_MODE=production，原生 DOM 对修正后的 WebGL；相同 90×10、8.4×18px 网格，
画布 756×180px，其他负载和交替顺序沿用前述方法。常速 4,000 批与 6× 节流 200 批各三对，
以下为三轮中位数；原始记录 `terminal-renderer-production-comparison.json` 已归档在
[Windows renderer 原始数据目录](terminal-renderer-windows-2026-10-20/README.md#artifacts)。

| 指标                      | 常速 DOM | 常速 WebGL | 6× DOM | 6× WebGL |
| ------------------------- | -------: | ---------: | -----: | -------: |
| 完成时间（秒）            |    28.66 |      11.10 |  18.29 |     5.80 |
| 进程树 CPU（秒）          |    38.41 |       8.66 |  21.94 |     6.80 |
| CDP layout（秒）          |    3.949 |      0.005 |  3.240 |    0.036 |
| CDP style（秒）           |    1.497 |      0.010 |  1.013 |    0.046 |
| working set 峰值（MiB）   |    678.3 |      499.6 |  528.1 |    454.1 |
| private bytes 峰值（MiB） |    634.2 |      446.9 |  483.5 |    404.2 |
| 输入 dispatch p95（ms）   |     18.5 |        2.4 |  201.0 |     37.9 |
| 输入到 parse p95（ms）    |     31.0 |       19.5 |  295.0 |    119.5 |
| 输入到下一 rAF p95（ms）  |     43.8 |       34.6 |  380.6 |    123.8 |

12 轮全部解析并 ACK，最终在途解析/待 ACK/待字符为零，最大解析在途四批，context loss 和
着色失败为零；WebGL viewport 与 backing buffer 一致。ACK 仍是模拟即时回调，PaintDuration
仍不可用；以上是 headless Chromium 的合成负载改善，不能当作 WebView2 端到端或真实 CC 结论。
短测峰值也不能判定长期泄漏。benchmark 的销毁快照只执行 xterm.dispose，实际 Tauri 生命周期
另有显式 context release，不能把两种 teardown 的数字混算。

其余性能表和 WebView2 短测属于修复前。新 smoke 使用实际 terminal-host / terminal-inner 结构，
在激活后立即 fit，并增加 DOM 参照终端；两版 smoke 时间不能直接算作性能改善。真实 IME、
PTY/ACK 和 Claude Code 长测仍待用户交互；该轮之后默认仍为 DOM。

## Windows React workspace 两小时 renderer soak：2026-10-21

同机顺序运行 DOM、WebGL 各两小时，使用真实 React `TerminalView` / `useAppShellState`、10 个终端、
约 6,000 行预填充及合成输出/监控/传输事件，不连接 SSH、不调用模型。DOM 于 02:44:15–04:44:26，
WebGL 于 04:51:51–06:51:58（+08:00）；两轮分别有 239、237 个负载采样。WebGL 运行于 Chromium
156.0.8078.4、NVIDIA RTX 3070 Laptop GPU / driver `32.0.16.1742`、ANGLE D3D11，硬件加速为 true。

| 检查项                           | DOM                                                     | WebGL                                                                         |
| -------------------------------- | ------------------------------------------------------- | ----------------------------------------------------------------------------- |
| renderer 负载采样                | 239/239 为 10 DOM、0 WebGL                              | 237/237 为 10 WebGL、0 DOM；10 个 active renderer 事件持续存在                |
| React/pageerror 与 renderer 失败 | 未发现 #185、Maximum update depth exceeded 或 pageerror | 未发现上述错误；context loss、load/activation 失败、DOM 回退均为 0            |
| 最大 ACK 在途 / 历史             | 4 / 200；卸载后 ACK 归零                                | 4 / 200；卸载时 ACK 归零                                                      |
| 销毁结果                         | 终端、订阅、pending xterm 均归零                        | DOM/WebGL 终端、订阅、pending xterm 均归零；跟踪的 Chromium/Node 进程随后退出 |

WebGL 的 237/237 进程组采样（Chromium CDP 进程加 Node 测试进程）中，working set 为
860.1–877.2 MiB，private bytes 为 1249.1–1274.3 MiB；前/末十次均值分别为 872.3/874.2 MiB
和 1252.9/1269.2 MiB。单核 CPU 前/末十次均值为 12.8%/14.4%，采样范围 9.2–42.2%。强制 GC 后
JS heap 为 48.6–50.9 MiB，前/末十次均值 50.0/50.5 MiB；xterm backing storage 约 122.019 MiB，
挂载元素数恒为 263。DOM 轮的 JSONL 没有进程组采样字段，因此不能据这两轮比较 DOM/WebGL 原生进程内存。

另一个 CDP 信号需继续追查：`Memory.getDOMCounters().nodes` 从首个负载样本的 338 增到 574，237
次采样间恰好每次加 1；页面挂载元素数仍为 263，卸载后 CDP nodes 降到 135。它可能表示 renderer
活动期间保留的文本/脱离节点，也可能与工具计数有关；当前证据不足以判作泄漏。两小时合成 React
结果不代替真实 Tauri WebView2 PTY/Claude Code 长任务；该轮之后默认仍为 DOM。真实 WebView2 六小时观察结果见下节。

完整统计 `beta11-react-workspace-soak.json`、`beta11-webgl-react-workspace-soak.json` 以及两轮
stdout/stderr 均已归档在 [Windows renderer 原始数据目录](terminal-renderer-windows-2026-10-20/README.md#artifacts)。

## Windows WebView2 六小时 idle 观察：2026-10-21

beta11 Tauri 开发版 PID 45964 在预期 02:21:37–08:21:37 +08:00 窗口内持续运行，使用 Edge
WebView2 155.0.4283.62 / Chromium 155、RTX 3070 Laptop GPU、driver 32.0.16.1742、ANGLE D3D11，
硬件加速为 true。目标 PowerShell PTY 在 356/356 条 renderer-health 中均为 WebGL；整个窗口
未发现 renderer-error、React #185 / maximum update depth 文本、context loss 或 DOM 回退。另有
一个终端在 03:01 后也进入 WebGL，因此 canvas backing estimate 从 10.1 MiB 增至 20.2 MiB。

359 条 Windows 原生采样均覆盖 FileTerm 与 6 个登记的 WebView2 进程，无截断且内存读取完整。
进程组 private bytes 为 397.0–766.1 MiB，08:20 窗口末为 495.1 MiB；working set 为
606.7–910.5 MiB，末值 682.2 MiB。进程组 CPU 均值 1.60%、中位数 1.41%、p95 1.80%，峰值
25.05%（均按单个逻辑核=100%）。JS heap 为 53.0–89.2 MiB，窗口末 73.5 MiB。峰值后回落且
后段稳定，但 idle 观察不能证明泄漏已解决。GPU 专用内存没有被日志单独采集；GPU 进程包含在
WebView2 汇总中。

解析失败与 ACK 失败均为 0，待解析批数/字符始终为 0，待 ACK 峰值为 1，最大记录 ACK 耗时
202 ms。交互采样仅 2 次，最大输入派发延迟 25 ms、事件至下一帧 64 ms，不能当作交互延迟
基准。共有 23 个长帧样本；03:06:53 一条 WARN 记录 3.879 s，健康报告峰值为 4.407 s。该长帧
没有脚本归因、活动 xterm 工作或解析/ACK 积压，原因未知。Windows 调度 ETW 因 privilege
error 1300 全程不可用。app.log 没有记录 shell 提示符，因此六小时预期时长已经经过，但
`Start-Sleep` 返回本身无法独立确认；没有运行 Claude Code。

筛选原始记录、汇总与过程元数据（`beta11-overnight-filtered.log`、`beta11-overnight-summary.json`、
`beta11-overnight-environment.json`）已归档在
[Windows renderer 原始数据目录](terminal-renderer-windows-2026-10-20/README.md#artifacts)。

## Windows 预检：2026-10-20（headless Chromium，不是 WebView2）

基于 PR #348 分支 `feat/renderer-performance-diagnostics`，提交
`e8c5932b49f7f937de17c70a559c4828e40489ba`。Windows 11 Pro 10.0.26300，x64、20 个逻辑
CPU、42,749,083,648 字节 RAM；`Win32_VideoController` 报告 NVIDIA GeForce RTX 3070 Laptop GPU，
驱动 `32.0.16.1742`，日期 `2026-09-30`。Chromium 还报告 NVIDIA 和 Microsoft Basic Render Driver；
实际 WebGL renderer 为 NVIDIA ANGLE D3D11，
`webglStatus=enabled`、加速启发式为 true。Chromium 156.0.8078.4，Playwright headless；GPU/驱动
字段来自 Chromium CDP `SystemInfo.getInfo`，不是 `Win32_VideoController` 全机枚举，也不能说明
FileTerm WebView2 使用完全相同的设备选择或 GPU 策略。

### 对照条件与网格

- 每轮新建独立 Chromium，启用 GPU。10 个可见终端，每个 90×10 格、6,000 行 scrollback，先回放
  6,500 行并确认普通缓冲区为 6,010 行；JetBrains Mono 14px、line-height 1.05、viewport
  1600×1200、DPR 1。两组共用 Unicode 11、emoji 宽度、前景色适配、日志着色、相同输出解析和四批
  在途信用；DOM 使用符号/框线/选区适配，WebGL 使用插件字形/选区。
- 常速每轮 4,000 批、7,398,010 UTF-8 字节。6 倍 CDP 主线程降速每轮 200 批、367,610 字节：
  4,000 批 DOM 在原 180 秒门限内只确认 1,747 批；因此该轮中止且不纳入结果。较短压力探针不与
  常速绝对耗时横向比较，只在 DOM/WebGL 配对内比较。
- 两档各三轮，模式次序交替。浏览器输入探针以 CDP 键盘事件本地回显；`dispatch`、xterm 解析完成和
  下一次 rAF 分开计时。Browser/GPU/renderer CPU 使用 CDP 进程 PID；进程树由 Windows
  Toolhelp 快照取得，每秒累计子进程 working set/private bytes/CPU。12 轮根 PID 均和 browser PID
  一致，角色含 browser、renderer、GPU、NetworkService，CPU 和 working set 采样完整。
- Font grid 的未校准控制仍显示 DOM 字符宽 8.4px、WebGL 8px（原始字体测量约 8.399994px）。性能
  两组仅在实验夹具中统一到物理像素格宽；基线均为 90×10、8×18px、720×180px。52 组字体/DPI
  与延迟加载回归、3 档 resize 的最终列/行/画布尺寸完全一致；DOM 文本边界最大误差 0.285 CSS px。
  这项修正留在 `tests/browser/helpers/terminal-grid-calibration.mjs`，没有复制到生产 DOM 或 WebGL。

### 结果（三轮中位数）

| 负载与指标                                    |                    DOM |                WebGL |                    同条件变化 |
| --------------------------------------------- | ---------------------: | -------------------: | ----------------------------: |
| 常速，4,000 批完成时间                        |                25.47 s |              12.43 s |                WebGL 少 51.2% |
| 常速，进程树 CPU 总时间                       |                36.19 s |               9.63 s |                WebGL 少 73.4% |
| 常速，renderer / GPU CPU 时间                 |         27.50 / 8.61 s |        8.78 / 0.86 s |                    分进程记录 |
| 常速，CDP 主线程布局 / style recalculation    |        3.422 / 1.340 s |    0.0066 / 0.0123 s |       仅为该夹具的 CDP 累计值 |
| 常速，输入 dispatch / parse / 下一 rAF p95    |  20.8 / 26.6 / 39.7 ms | 2.6 / 19.8 / 30.5 ms | rAF 不代表 GPU 提交或屏幕扫描 |
| 6× 降速，200 批完成时间                       |                51.80 s |              11.50 s |                单独短压力探针 |
| 6× 降速，进程树 CPU 总时间                    |                61.36 s |              13.41 s |              不与常速负载混算 |
| 6× 降速，输入 dispatch / parse / 下一 rAF p95 | 465 / 1,611 / 1,762 ms |    53 / 247 / 254 ms |        每轮 37–118 个键盘样本 |

| 常速峰值内存                         |                      DOM |                    WebGL |
| ------------------------------------ | -----------------------: | -----------------------: |
| 浏览器进程树 working set             |                700.8 MiB |                492.3 MiB |
| 浏览器进程树 private bytes           |                657.1 MiB |                431.4 MiB |
| renderer / browser / GPU working set | 470.6 / 54.4 / 136.0 MiB | 275.4 / 54.3 / 130.4 MiB |
| JS heap 峰值 / GC 后 retained        |          53.9 / 19.5 MiB |          32.6 / 19.7 MiB |

每轮 4,000 批或 200 批均完整解析并确认，结束时待解析批次、待 ACK、待字符数和 parse/ACK 失败均为零；
最大解析在途四批、context loss 和着色失败均为零。夹具的 ACK 是立即完成的模拟回调，测得等待 0–2ms；
它不模拟 WebView2→Rust IPC 的 ACK 等待。CDP `PaintDuration` 不可用，绘制成本只能结合 renderer/GPU
CPU、布局/style 指标和 FileTerm 的 `work` 阶段日志分析。

销毁后 JS heap 中位数约 4.8/5.0 MiB、backing storage 约 1.0 MiB，但进程树 working set 仍约
375/318 MiB。Chrome 缓存、共享页和原生/GPU 分配都未被销毁后数字拆分；这些短测不能证明或否定长时间
泄漏，也不能据 RSS/working set 差异宣布泄漏已解决。

字体/DPI JSON、逐秒进程/输入/解析/ACK 原始记录、生命周期日志和全部截图保存在
[`terminal-renderer-windows-2026-10-20/`](terminal-renderer-windows-2026-10-20/)。真实输入法不支持
headless；没有在本轮实测 FileTerm WebView2、原生窗口 DPI、实际 Claude Code 输出或原生 IPC。
生命周期夹具将生产 `terminal-lifecycle-core.ts` 打包到 Chromium 中，验证 WebGL 激活、CJK/emoji/选区、
模块加载失败/初始化失败/context loss 后 DOM 回退及主动 dispose 后 context 丢失；这仍不等于 Tauri WebView2。

更多运行命令和人工验收步骤见 [`terminal-renderer-windows-2026-10-20/README.md`](terminal-renderer-windows-2026-10-20/README.md)。

## 实际 Windows Tauri WebView2：2026-10-21（网格修复前）

基于 PR #348 分支 feat/renderer-performance-diagnostics，HEAD 为
e8c5932b49f7f937de17c70a559c4828e40489ba，工作树包含未提交修改。真实 Tauri 开发版
（2.3.1-beta.10）内的 WebView2 runtime 为 Edge 155.0.4283.62，DPR 1.25，窗口视口
1280×820。Windows 11 Pro 10.0.26300 x64，20 个逻辑 CPU、39.8 GiB RAM。显示适配器为
NVIDIA GeForce RTX 3070 Laptop GPU，驱动 32.0.16.1742（2026-09-30）；WebView2 app.log
报告 ANGLE NVIDIA Direct3D11 且 hardwareAcceleration=true。

DOM 与 WebGL 各运行同一个真实 WebView2 smoke：临时内存终端写 1,500 行中文/emoji，检查
搜索、选区、resize、当前字体和 JetBrains Mono 14，再创建/销毁三轮 renderer。两种模式均无
page/console 错误，1,502 行解析，搜索命中、选区文本为 Header，缩窄和恢复网格正确；
WebGL context loss 后回退 DOM，主动 dispose 后 context 均丢失且 host 子节点为零。

| 单轮 smoke 指标                               |                      DOM |                      WebGL |
| --------------------------------------------- | -----------------------: | -------------------------: |
| xterm write callback（ms）                    |                     22.7 |                       34.9 |
| callback 到下一 rAF（ms）                     |                       10 |                         28 |
| CDP script / layout / style / task 累计（ms） | 54 / 42.6 / 38.3 / 263.5 | 184.9 / 9.8 / 22.2 / 722.6 |
| JS heap 采样（MiB）                           |                     32.9 |                       26.4 |
| 进程树 working set 峰值（MiB）                |                    637.6 |                      631.9 |
| 进程树 private bytes 峰值（MiB）              |                    679.6 |                      536.2 |
| 进程树 CPU 增量（秒）                         |                     6.59 |                       2.05 |

各模式仅一次约 38 秒的合成负载，采样含 Tauri、browser、renderer、GPU 和 utility 子进程；
不能据此判定端到端快慢或长期内存趋势。xterm callback 包含解析和回调排程，rAF 只表示帧机会；
CDP TaskDuration 也不是 GPU 完成时间。额外着色/字形布局没有从同步 callback 独立剥离。
本测试创建的是 WebView2 内存 host，不经过真实 PTY/Rust 输出通道，因此原生输入延迟和
parse/ACK 在途量无法采集；IME composition 也无法由该脚本触发。headless fixture 的 ACK 是模拟回调。

125% DPI 下发现生产 WebGL 格网列数更多，实验校准没有套到应用中：

| 字体                 | 同一 host 宽度 1,256 CSS px 下 DOM / WebGL 列×行 | DOM / WebGL 物理格宽 |
| -------------------- | -----------------------------------------------: | -------------------: |
| 当前设置，12px       |                                  188×51 / 194×51 |         8.247 / 8 px |
| JetBrains Mono，14px |                                  147×38 / 155×38 |         10.5 / 10 px |

差异来自 WebGL renderer 对字体宽度乘 DPR 后取整，会影响依赖固定列数的 TUI。仅实验夹具
的校准仍位于 tests/browser/helpers/terminal-grid-calibration.mjs；没有更改生产尺寸策略。
需要人工检查窗口 resize、字号、IME、选区和实际 TUI 行为后再讨论默认后端。

测试 JSON、35 次进程树采样、截图及只含 terminal-renderer 事件的过滤日志见
[Windows 测试记录](terminal-renderer-windows-2026-10-20/README.md)。

## 环境与范围

- 生产适配器基于提交 `1d6f0e97`；xterm `6.0.0`，实验插件 `@xterm/addon-webgl@0.19.0`。
- MacBookPro18,3 / Apple M1 Pro，8 个逻辑 CPU、16 GiB 统一内存；macOS 27.0.1
  （26A434），Chrome 155.0.8059.80，headless，ANGLE Metal 硬件加速。夹具拒绝
  SwiftShader / llvmpipe 等软件路径，并验证 WebGL 插件确实接管 renderer。
- 1600×1200 viewport、DPR=1；10 个可见终端，每个 90×10 格、6,000 行历史上限。
  JetBrains Mono 14px，line-height=1.05；中文使用系统 fallback。先写入 6,500 行，
  确认每个普通缓冲区保持 6,010 行，再预热 200 批、GC，随后开始测量。
- 两组共享 Unicode 11、emoji 宽度、对比度/字宽缓存限制、默认前景适配、日志着色和
  同一套解析/ACK 流控。DOM 保留符号、框线和选区适配；WebGL 用插件自身字形/选区
  渲染。同网格复测对生产 DOM 适配器组合做了测试专用归一化；不能把全部收益归因于 GPU 硬件。
- 每轮固定 4,000 批、7,398,010 个 UTF-8 输出字节。内容包含全屏擦除、光标移动、
  中文、符号、长横线和计数；4 批在途信用，解析完成才确认，保持既有输出语义。
  这是 CC 式合成重绘，不是录制的 Claude Code 会话；未覆盖网络、SSH 或模型推理。
- 常速和 CDP 6 倍 CPU 降速各三轮，按 DOM/WebGL、WebGL/DOM、DOM/WebGL 顺序执行。
  每轮启动独立浏览器，未并行运行构建/测试。6 倍降速不等同 Windows 节能/效率模式，
  也不会以相同比例限制 GPU。对照夹具允许 180 秒以取得完整结果；既有输出背压回归的
  120 秒门限未改。这里额外固定真实字体、满历史和连续终端键盘探针，与此前 87 秒夹具
  的负载不同，不能把两次耗时直接当作同一基准的前后变化。

## 计量方法

CPU 使用 [CDP SystemInfo.getProcessInfo](https://chromedevtools.github.io/devtools-protocol/tot/SystemInfo/#method-getProcessInfo)
的进程累计 CPU 秒差，包含浏览器、renderer、GPU 进程等。表中的 CPU 时间是完成同一工作
消耗的总 CPU 时间，不是任务管理器的瞬时百分比，也不是 GPU 利用率或功耗。

内存每秒通过 `ps` 读取该独立浏览器所有进程的 RSS，并分别记录 renderer/GPU 峰值。
RSS 合计可能重复计算共享页，不能当作私有提交量、准确物理内存或专用显存。
同时保留 JS 堆和 [Runtime.getHeapUsage](https://chromedevtools.github.io/devtools-protocol/tot/Runtime/#method-getHeapUsage)
的 backing storage（包括 ArrayBuffer 等）数据，避免把 JS 堆误当作总内存。
图集按每个 WebGL context 的 RGBA 尺寸估算，另列共享 canvas；该估算不覆盖 framebuffer、
驱动和其他 GPU 分配，不能再与 RSS 简单相加。

输入探针约每 250ms 通过真实键盘事件向首个终端发送 `x`，经 xterm `onData` 本地回显，
记录事件时间到 handler、解析完成及下一次 rAF 的间隔。下一帧只是绘制机会，不代表最终
GPU 提交/屏幕扫描完成，也不包含远端响应。另存自动化调用耗时，未将其混入浏览器侧延迟。
三轮分别计算 p95 后取中位数，避免慢组因样本更多而在汇总中获得更高权重。

## 网格校准：2026-10-20 补充

已定位截图中约 5% 的格宽差异：xterm 6 DOM 保留浮点字符宽度，WebGL 0.19 对
`measuredWidth × DPR` 向下取整。14px JetBrains Mono 的原始测量均为约 8.399994px，
但 DPR=1、90 列时 DOM 画布宽 756px、WebGL 为 720px。字体已经加载，不能通过
再次加载字体消除这种后端策略差异，也不能把旧结果称为同像素网格比较。

校准只放在 `tests/browser/helpers/terminal-grid-calibration.mjs`：实验 DOM 在计算布局
期间使用与 WebGL 一致的物理像素格宽，随后立即恢复原始测量；字体族、字号、字宽缓存
和生产 DOM 均不修改。两侧 CSS 格宽/格高从实际取整后的画布尺寸除以列数/行数取得，
让 FitAddon、鼠标命中和选区使用相同的显示网格，不拉伸 WebView 或直接覆盖截图宽度。
钩子在字号、字体、DPI 和 resize 的正常重算链路执行，移除或失败时恢复原方法。
这是实验基线归一化，不是把整数格宽接入已发布的 DOM 运行时。

同时补齐性能夹具遗漏的 `terminal-host` 类，并断言生产 `text-spacing-trim: space-all` /
`text-autospace: no-autospace` 规则实际生效；之前只是加载了 CSS，缺少作用域。旧对照
数据因此还包含这个夹具差异，复测不沿用它们。

浏览器回归 `terminal-renderer-font-grid.mjs` 已通过 52 组检查：DPR 1/1.1/1.25/1.5/2，
JetBrains Mono 与导入字体、12/13.65/14/16/24px，字体延迟加载及恢复默认 DPI/字体。
格宽、格高、画布尺寸和 FitAddon 列/行数的两组最大差值为零；物理格宽浮点差值最大
3.56e-15px。中文/全角标点、组合字符、emoji 占格与选区文本一致，实际 DOM 边界误差
最大 0.176 CSS px；首列、中间列和末列鼠标命中正确。延迟字体加载前后两组均从
54 列变为 32 列，且与新建终端一致。还验证失败时原始字体测量可恢复。

### 同网格性能对照

两组在 14px、DPR=1 时均使用 8×18px 单元格、720×180px 画布，原始字体测量仍为
8.399994px。以下为常速和 6 倍降速各三轮中位数，字体、数据量和实际网格均一致：

| 指标                        | 常速 DOM | 常速 WebGL | 6 倍降速 DOM | 6 倍降速 WebGL |
| --------------------------- | -------: | ---------: | -----------: | -------------: |
| 4,000 批完成时间（秒）      |    22.35 |      10.47 |       154.20 |          45.62 |
| CPU 总时间（秒）            |    31.10 |       9.52 |       172.97 |          51.12 |
| 平均 CPU（单核=100%）       |   140.02 |      90.90 |       112.17 |         112.04 |
| 主线程布局时间（秒）        |    2.803 |      0.004 |       21.058 |          0.109 |
| 进程 RSS 采样峰值（MiB）    |  1433.27 |    1254.81 |      1361.39 |        1251.42 |
| renderer RSS 峰值（MiB）    |   846.48 |     657.25 |       779.81 |         657.33 |
| GPU 进程 RSS 峰值（MiB）    |   164.48 |     179.53 |       160.03 |         174.97 |
| JS 堆采样峰值（MiB）        |    54.01 |      37.62 |        41.78 |          37.35 |
| 结束后 GC 的 JS 堆（MiB）   |    19.44 |      19.69 |        19.52 |          19.71 |
| backing storage 峰值（MiB） |    71.21 |      75.42 |        71.15 |          77.11 |
| 事件分派 p95（ms）          |    13.90 |       1.60 |        66.90 |          10.10 |
| 输入至解析完成 p95（ms）    |    21.30 |      17.10 |        83.50 |          44.00 |
| 输入至下一 rAF p95（ms）    |    35.30 |      32.20 |       116.20 |          54.80 |

12 轮均完整确认 4,000 批，待解析字符/回调/xterm 待写字节最终为零，context loss 和
着色失败均为零。持续饱和时两组仍可能接近满核；CPU 总时间是同样工作消耗的累计时间，
不能当作瞬时 CPU 百分比的降幅。所有截图在测量结束后保存，并核对了中文、符号和横线。

WebGL 图集的每 context RGBA 估算合计仍为 10 MiB，共享 canvas 为 1 MiB；不包含全部
GPU/驱动分配。两组结束后 GC 的 JS 堆相近，而 RSS 包含浏览器、共享页和缓存，不能把
短测峰值差直接解释成长时间常驻内存或现场泄漏的变化。销毁终端后两组均回到 21 个 DOM
节点、约 1 MiB backing storage；浏览器 RSS 不同步归零。

同网格的实验 DOM 对部分符号需要更多拟合节点，不能把它相对旧数据的耗时增加解释为
生产 DOM 回归。后端仍有绘制/字形处理差异，这里并不要求两个后端的字形抗锯齿逐像素相同。

| 降速 | 后端  | 轮次 | 耗时 s |  CPU s | RSS 峰值 MiB | GPU RSS MiB | 下一帧 p95 ms | 输入样本 |
| ---- | ----- | ---: | -----: | -----: | -----------: | ----------: | ------------: | -------: |
| 1    | dom   |    1 |  22.35 |  30.78 |      1433.27 |      164.19 |         35.30 |       86 |
| 1    | webgl |    1 |  10.19 |   9.52 |      1249.34 |      174.70 |         32.20 |       40 |
| 1    | webgl |    2 |  10.70 |   9.54 |      1254.81 |      180.25 |         33.50 |       42 |
| 1    | dom   |    2 |  22.21 |  31.10 |      1437.00 |      164.48 |         34.90 |       85 |
| 1    | dom   |    3 |  22.45 |  31.85 |      1428.27 |      165.28 |         41.20 |       86 |
| 1    | webgl |    3 |  10.47 |   9.52 |      1255.59 |      179.53 |         25.70 |       41 |
| 6    | dom   |    1 | 155.55 | 174.44 |      1361.39 |      161.36 |        116.20 |      524 |
| 6    | webgl |    1 |  46.55 |  52.14 |      1251.97 |      181.50 |         54.80 |      179 |
| 6    | webgl |    2 |  45.62 |  51.12 |      1249.69 |      172.30 |         60.10 |      175 |
| 6    | dom   |    2 | 154.20 | 172.97 |      1392.03 |      160.03 |        119.50 |      521 |
| 6    | dom   |    3 | 145.55 | 163.83 |      1356.98 |      158.77 |        106.40 |      508 |
| 6    | webgl |    3 |  43.59 |  49.06 |      1251.42 |      174.97 |         52.50 |      168 |

[校准后的逐轮汇总与字体校验](terminal-dom-webgl-comparison-2026-10-20-calibrated.json)
包含源文件 SHA-256、网格、CPU/内存/交互数据和检查状态。完整采样为
`/tmp/fileterm-renderer-comparison-calibrated.json`，SHA-256 写入汇总；未混入先前被中断的
不完整试跑或未校准的历史数据。

### 历史：未校准网格的对照

以下数据保留为最初的当前 DOM 栈/原生 WebGL 栈对照，网格像素面积不同。
完整数值仍保存在 [原始逐轮汇总](terminal-dom-webgl-comparison-2026-10-20.json)。

| 条件     | DOM 完成时间 | WebGL 完成时间 | DOM / WebGL CPU 秒 | DOM / WebGL 下一帧 p95 |
| -------- | -----------: | -------------: | -----------------: | ---------------------: |
| 常速     |       19.11s |         10.44s |       28.07 / 9.60 |          36.4 / 29.1ms |
| 6 倍降速 |      126.29s |         47.04s |     143.85 / 52.56 |         106.2 / 56.9ms |

旧的完整采样位于 `/tmp/fileterm-renderer-comparison-full.json`，SHA-256 已写入原始汇总。
这两种条件的旧数据不能与同网格对照混算中位数，也不能据旧结果宣称已消除了网格差异。

## 复现

Windows 命令、可选环境变量、原始数据和交互验收步骤见
[`terminal-renderer-windows-2026-10-20/README.md`](terminal-renderer-windows-2026-10-20/README.md)。性能脚本
现在明确要求 Windows，输出 Toolhelp 进程树 working set/private bytes/CPU；GPU/驱动字段来自 Chromium
CDP。6× CPU 节流探针默认 200 批，若修改批数必须同时检查 Playwright 180 秒等待上限。

## 边界

这组测试当时的生产默认 renderer 为 DOM；当前新安装及缺少 renderer 字段的配置默认 WebGL（见文档顶部）。真实 WebView2
中已确认硬件加速路径、context loss 回退和 dispose，但尚未验证普通/节能模式、实际 PTY ACK、TSF 输入法、
原生终端选区、挂起恢复或数小时真实 Claude Code。短时合成回放和 working set/private bytes 差异
不能证明现场泄漏已经解决，也不能据此承诺 WebGL 在所有机器上更省内存。

## 仓库验证与真实开发进程

- `npm run typecheck`、164 项 renderer 测试、801 项 Rust 单元测试（1 项忽略）、10 项 CLI、
  23 项 contract、3 项 Windows scheduler、严格 all-target/all-feature Clippy、`cargo fmt --check`、
  `git diff --check` 均通过。52 组浏览器字体/DPI 回归、12 轮配对性能对照和生命周期回退夹具通过。
- 本次改动的 TypeScript/JavaScript、JSON、Markdown Prettier 检查与定向 ESLint 通过。仓库级
  `npm run lint` 和全范围 Prettier 检查失败于广泛的 CRLF/格式问题（Prettier 报告 491 个文件）；
  未为通过全仓检查重写无关文件。
- 当前 WebGL 开发版由 `VITE_FILETERM_TERMINAL_RENDERER=webgl` 启动：PID 45964，执行文件
  `apps/tauri/src-tauri/target/debug/fileterm.exe`，产品版本 `2.3.1-beta.11`，启动时间
  2026-10-21 02:06。Vite 端口 5188，WebView2 9223 调试端口仅本机监听。源码基点为
  `e8c5932b49f7f937de17c70a559c4828e40489ba`，运行时含未提交工作区修改。
- Edge WebView2 155、NVIDIA RTX 3070 Laptop GPU / driver `32.0.16.1742`、ANGLE D3D11
  `hardwareAcceleration=true`。beta11 中同时保留一个 DOM 回退 PowerShell PTY 和一个 WebGL
  PowerShell PTY；混合中英、符号、数字和 emoji 可见，`Ctrl+Shift+F` 在两端均有匹配。真实 context
  loss 后观察到一帧白屏，稍后同一会话的 DOM 行恢复；切换帧时长尚未量化。IME、物理选区/复制尚未验收。
- 2026-10-21 02:21:37–08:21:37 +08:00 进行了六小时无输出观察；目标 WebGL PTY 有 356 条
  renderer-health，Windows 原生进程组有 359 条完整采样。结果、峰值、长帧与完成状态的限制见上节，
  过滤日志及环境元数据（`beta11-overnight-filtered.log`、`beta11-overnight-summary.json`、
  `beta11-overnight-environment.json`）已归档在
  [Windows renderer 原始数据目录](terminal-renderer-windows-2026-10-20/README.md#artifacts)。
- Claude Code 2.1.296 的本地低消耗尝试没有符合单命令预期：Custom Haiku（映射为
  `deepseek-flash`）运行约 22 秒、消耗约 2.4k tokens，并启动 12 个 shell。已终止该 Claude 进程及其
  子进程；没有将其计为 Claude 长任务通过。六小时 idle 观察使用本地 PowerShell `Start-Sleep`，不调用模型。
