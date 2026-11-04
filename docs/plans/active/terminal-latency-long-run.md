# CC 延迟与长期无响应：继续排查

## 2026-10-24：当前分支审查收尾

范围：纯 ASCII 格保持固定宽度 slot（advance 与校准格宽不能保证一致），保留缩放字形并入 slot 与样式类优化。

测试策略：浏览器夹具复用现有 TypeScript loader，不引入额外依赖；几何夹具补齐文本覆盖与逐格断言，选区夹具改为真实滚动并逐行校验包裹状态。

## 2026-10-24：block 适配器改用 xterm 校准网格

CDP 采样剖析（同一夹具、80 次 CC 风格整屏刷新）显示 `getBoundingClientRect` 自身占 **30.4%**，
是所有具名项中最大的开销；xterm 的 DOM renderer 自身不调用它，因此这部分全部来自 block 适配器
每帧两次读取（`screen` 与 `.xterm-rows`）。原因是 xterm 刚重写行 DOM，布局为脏，第一次读取会强制整屏布局。

几何探测确认可以完全替代：

- `.xterm-rows` 相对 `.xterm-screen` 的 top/left 偏移均为 0（应用 CSS 与 xterm CSS 都没有给该容器加 padding）。
- `_renderService.dimensions.css.cell` 为 9.6 × 21.6，与实测的 9.6 × 21.59375 一致。
- `dimensions.css.canvas` 为 1152 × 648，等于 `cols × cellWidth`。

改动：`terminal-block-glyph-renderer.ts` 优先从 `dimensions.css` 取 `cell.width/height` 与 `canvas.width`，
行顶位置改为 `index × cellHeight`；指标不可用（缺失、非有限或非正）时回退到原来的单次容器测量。
`replaceBlockGlyphs` 的 `screenBounds: DOMRect` 参数相应改为 `screenWidth: number`。

### 同机 A/B（Edge 内核、同一内容、各三轮）

| 运行      | `block-layout` 单次 | 单次最大 | `LayoutCount` | `symbol-layout` |
| --------- | ------------------: | -------: | ------------: | --------------: |
| HEAD #1   |            17.57 ms |    23 ms |           240 |          874 ms |
| HEAD #2   |            17.37 ms |    23 ms |           240 |          862 ms |
| HEAD #3   |            17.57 ms |    24 ms |           240 |          864 ms |
| 改动后 #1 |         **8.13 ms** |    11 ms |       **120** |          887 ms |
| 改动后 #2 |         **8.11 ms** |    12 ms |       **120** |          874 ms |
| 改动后 #3 |         **8.34 ms** |    14 ms |       **120** |          884 ms |

`block-layout` 单次降低约 **53%**，每帧强制布局次数减半（240 → 120）；`symbol-layout` 不受影响。
120 次刷新的总墙钟时间基本不变（约 6.1–6.4 秒），因为夹具每轮等待两个动画帧，掩盖了同步耗时差异；
真实应用中的效果需要现场复测。

### 验证

- 单测：`terminal-block-glyph-renderer.test.mjs` 增至 4 项，新增「校准网格下零几何读取」与「指标无效时回退」；
  连同 symbol 共 11 项通过。
- 像素回归：`terminal-horizontal-rules.mjs` 在 1 / 1.25 / 1.5 / 2 倍 DPR、14/24px、`─ ━ ═` 共 24 种组合通过（退出码 0）。
  夹具同时断言带适配器时 `uneven` 全为 0、内容文本不变。
- 全量 renderer 207 项中 206 通过，唯一失败仍是 Windows 上无法运行的 `linux-icon-files` 包检查。
- 改动文件 Prettier、ESLint、`typecheck` 通过。

### 剩余热点

同一剖析中 `replaceChildren` 占 6.3%（symbol 适配器每帧重建 span）、`replaceBlockGlyphs` 2.4%、GC 1.8%。
行级签名缓存与「先比较再重建」是下一批候选，尚未实现。

### symbol 层：成本拆解与只保留一项的改动

用临时 esbuild 插件逐项替换适配器内部步骤，在 100 次流式 CC 风格刷新下测量
（混合 ASCII 文本与符号，3.3 ms/次）：

| 变体                            | symbol-layout | 说明                    |
| ------------------------------- | ------------: | ----------------------- |
| 当前                            |    3.38 ms/次 | 基准                    |
| 跳过全部更新（不建 DOM）        |    1.12 ms/次 | 行扫描 + 拟合判断的底价 |
| 跳过 DOM 插入（仍建元素）       |    3.06 ms/次 | 仅插入约 0.27 ms        |
| 不调用 `measureText`            |    2.69 ms/次 | 测量约占 20%            |
| 不设置 canvas `font`            |    2.69 ms/次 | 字体解析约占 20%        |
| 用码点判断替代 Unicode 属性正则 |    3.38 ms/次 | 无可测差异              |
| 跳过 buffer 逐格映射            |    3.31 ms/次 | 映射约 1.5%             |

结论：**67% 的开销是逐格构建行内 DOM**（每格一个 slot，带 transform 的再加一层），
其余是 `measureText` 与 canvas 字体赋值。Unicode 正则和 buffer 映射都不是瓶颈。

试过并**回退**的方案：按源元素缓存拟合结果（文本 + 格宽 + 字体代次）并对纯 ASCII 源提前跳过。
流式内容下为中性（3.48 → 3.39 ms/次），固定内容下反而略差（7.3 → 7.6 ms/次）——
xterm 每次重绘会重建行内子节点，缓存键随之失效，跳过逻辑只剩开销。

像素回归 `terminal-symbol-glyphs`、`terminal-symbol-tables`、`terminal-selection-glyphs`、
`terminal-horizontal-rules` 均通过；`terminal-symbol-proportions` 的粗体 `▢` 比例断言（13×13 → 2×7）
在 HEAD 版本上以完全相同的数值失败，属既有问题，不是本次回归。

#### 逐格元素结构改造（2026-10-24 后续）

- **每格仍保留固定宽度 slot**：ASCII 的 advance 与校准格宽不能保证一致，把连续 ASCII 格合并成
  文本节点会引入累积位移，因此不采用；只把缩放字形从内层 span 移到 slot 本身，带校正的格由两个元素降为一个。
- **不变量样式移入 CSS 类**：`font-weight: inherit` 与 `position/z-index` 改由适配器按文档注入的一份
  样式表提供。注意 xterm 运行时会注入 `.xterm-dom-renderer-owner-N span:not(.xterm-bold){font-weight:normal}`，
  选择器必须写成 `.xterm .xterm-rows span.xterm-fitted-symbol-slot` 才能压过它；只用单个类会被覆盖，
  表现为粗体符号变回常规字重。

实测（100 次流式 CC 风格刷新）：`symbol-layout` 3.38 → 3.31 ms/次（约 2%），收益主要来自每格少一个元素
以及更少的行内样式写入。

新增 `tests/browser/terminal-cell-geometry.mjs` 作为选中几何回归：对混合 ASCII、符号与 CJK 的行，
逐字符用 `Range.getBoundingClientRect()` 取浏览器实际选中矩形，与 `列 × 格宽` 比较。三档 DPR
（1 / 1.25 / 2）最大偏差 0.81–0.90px，阈值 1px。这一层能直接发现逐格结构变化引入的累积位移。

回归结果：`terminal-cell-geometry`、`terminal-symbol-glyphs`、`terminal-symbol-tables`、
`terminal-selection-glyphs`、`terminal-horizontal-rules` 通过；`terminal-symbol-proportions` 仍是
HEAD 上相同的既有失败。渲染器 207 项测试 206 通过（仅 Linux 包检查在 Windows 上失败），
typecheck、ESLint、Prettier 通过。

#### 点击后框线变粗（2026-10-24 现场反馈）

现场现象：随机字节输出后按左键，`╏├☆╚┬┘┋╹╠├` 这类字符变粗；滚动一次恢复。

复现（`╏├☆╚┬┘┋╹╠├` 填充 6 行，单击终端）：墨迹像素 15,436 → 19,366（+25%），滚动后回到
15,436。同时 `.xterm-block-glyph-placeholder` 从 5 个变成 0 个。

根因：xterm 在选区或光标状态变化时会重写行的直接子节点，canvas 仍留在 `.xterm-screen` 里继续
绘制；block 适配器只订阅 `onRender`，没有对行 DOM 变化做响应，于是新写入的明文重新可见，与 canvas
叠在一起，视觉上就是笔画变粗。symbol 适配器有行级 `MutationObserver` 会重写共享的 source，
使问题在混合行上更明显；滚动触发 `onRender` 后两个适配器按 symbol → block 的顺序重跑，才恢复正确。

修复：block 适配器增加与 symbol 相同的行级 `MutationObserver`（只认 `target.parentElement` 为
rows 容器、即 xterm 重写整行的记录）。每次处理前先 `takeRecords()` 丢弃本次 `onRender` 已覆盖的
重写记录，处理后同样丢弃自身写入，避免循环，也避免「范围内没有图形行」提前返回时把记录留给观察器
再扫一遍——纯文本刷新因此从 100 次刷新 200 次调用降到 100 次。注册顺序是 selection → symbol → block，
因此同一微任务内 symbol 先重写、block 最后恢复占位符。旧实现（HEAD）在该测试上失败，修复后通过。

新增 `tests/browser/terminal-block-glyph-selection.mjs`：单击后断言占位符数量不变、墨迹增量
≤0.5%，并断言 250ms 内没有新的行级变更（无观察器循环）。

## 2026-10-23：DOM 后端 block / symbol 的行级 DOM 扫描

用户提供的 `app-log.md`（10-23）为最新分析；同目录其余 `app.log`、`app-log-1021`、`app-log-1022` 属 beta.10–beta.12 的较早现场，不能与最新代码状态混读。本轮先核对既有 commit，再只做仍未被覆盖的部分。

### 既有修复核对

以下提交均已在分支基底（`2.3.1-beta.14`，`af89b139`）中，本轮未重复修改：

| 提交       | 覆盖内容                                                                                                      |
| ---------- | ------------------------------------------------------------------------------------------------------------- |
| `451a8bd7` | DropdownSelect / ContextMenu / SessionSendTargetPicker 定位 state、连接表单 `updateForm` 循环等 setState 循环 |
| `80d5285c` | workspace updater 重放、symbol observer 重复整行扫描                                                          |
| `de4019f2` | 共享字体测量、纯文本行跳过布局                                                                                |
| `b3ece66f` | 终端字形布局削减与卡顿进度记录                                                                                |
| `05f3ace1` | 终端滚动工作去重                                                                                              |

复核 Renderer 侧仍在的 setState effect（监控浮层计时器、磁盘挂载点同步、网卡回退、分屏权重恢复、审批请求清理）均有值比较或一次性门控，本轮未发现新的无界更新链。因此 React #185 不再作为本轮改动对象；其真机复现条件仍见 [更新链证据](../../quality/react-update-loop-audit.md)。

### 本轮改动

`terminal-block-glyph-renderer.ts`：

- 行筛选原先对每行执行 `textContent` + Unicode 正则 + `querySelector`。现在直接读 buffer cell 的数值 codepoint，且筛选结果（`bufferGlyphs`）直接复用于绘制，省掉对同一批行的第二次全列扫描。
- 行几何原先为每个受影响行调用 `getBoundingClientRect()`，即每行一次强制同步布局。现在只读一次容器边界，按 `rowTopBase + index × rowHeight` 计算，画布高度改用容器高度除以行数。

`terminal-symbol-glyph-renderer.ts`：

- 在既有 DOM 文本判断前增加 buffer 非 ASCII 预检，ASCII-only 行（普通终端的大多数）不再读取 `textContent` 和两次 Unicode 正则。buffer 行不可用时回退到原 DOM 判断，不把「无法确认」当成「确认是 ASCII」。
- 保留纯框线行排除、已拟合来源排除与批量测量前不写 DOM 的顺序。

未改：块字形的像素几何、颜色解析、占位合并、选区层级与字体适配规则。`replaceBlockGlyphs` 内按文本节点读取的 `getComputedStyle` 保留——它取的是实时颜色，移入背景缓存判断会沿用过期主题色。

### 验证

新增 `tests/terminal-block-glyph-renderer.test.mjs`，用计数 mock 驱动真实适配器：纯 ASCII 行不读 DOM 文本、不做逐行选择器、不读逐行几何；含框线行只读一次容器边界。`terminal-symbol-glyph-renderer.test.mjs` 增加三个用例，覆盖 ASCII 跳过、非 ASCII 进入、buffer 不可用回退。

同一组用例在 HEAD 旧实现上 3 项失败（逐行 `textContent`、逐行 `getBoundingClientRect`、symbol 无预检），修补后 9 项全部通过。

### 同机 A/B 实测（2026-10-24）

用 `terminal-dom-layout-load.mjs` 在同一台机器（Ryzen 7 7800X3D）、同一 Edge 内核上分别加载 HEAD 与工作区适配器，内容与刷新次数相同（30 行 CC 风格 `✻─┬█ ▢⚠`，120 次整屏刷新），各跑三轮：

| 运行      | `block-layout` 单次 | `symbol-layout` 合计 | `LayoutCount` / `LayoutDuration` |  总耗时 |
| --------- | ------------------: | -------------------: | -------------------------------: | ------: |
| HEAD #1   |             18.1 ms |               872 ms |                    240 / 1508 ms | 6201 ms |
| HEAD #2   |             17.7 ms |               889 ms |                    240 / 1478 ms | 6031 ms |
| HEAD #3   |             17.6 ms |               884 ms |                    240 / 1469 ms | 6024 ms |
| 工作区 #1 |             17.5 ms |               862 ms |                    240 / 1471 ms | 6014 ms |
| 工作区 #2 |             18.1 ms |               932 ms |                    240 / 1517 ms | 6109 ms |
| 工作区 #3 |             17.9 ms |               912 ms |                    240 / 1502 ms | 6107 ms |

纯 ASCII 行对照（临时夹具，30 行文本、120 次刷新）：改动前后 `block-layout` 都是每次 0.03 ms 量级，无可测量差异。

真实应用侧（同机、2 终端、6,092 行历史）：纯文本流时 0.13 ms/次；切到 CC 风格满屏框线后 17.1 ms/次，与夹具的 17.6–18.1 ms 一致，说明夹具能复现现场成本。

**结论修正**：本轮改动把行筛选改为 buffer 码点、行几何改为算术定位，但 A/B 三轮差异都在噪声内（17.6–18.1 ms/次），**在块字符密集场景没有可测量的性能收益**。原因是每行真正的开销仍在 `replaceBlockGlyphs` 内部（text 节点遍历、`getComputedStyle`、span 重建、逐行 canvas 重绘），而旧实现的逐行 `getBoundingClientRect` 本来就在 DOM 未变更时读取，并未产生额外的强制布局。纯文本路径新旧实现本来都很便宜。

不同机器、不同内容的数据不能并列比较。本机 beta.12 的原始日志已被单备份轮转覆盖，因此无法做日志级的前后对比。

要真正降低 CC 场景成本，需要行级内容签名缓存、`getComputedStyle` 缓存或单画布脏矩形方案，这些尚未实现。上述几何改动随后被 2026-10-24 的校准网格方案取代；行筛选（buffer 码点）与 symbol 的非 ASCII 预检保留在最终实现中。

门禁：`npm run typecheck -w @fileterm/tauri` 通过；renderer 205 项测试 204 通过，唯一失败为 Windows 上运行 `linux-icon-files` 包检查的既有环境性失败，与本轮无关；改动文件 ESLint 与 Prettier 通过。

未验证：浏览器像素回归（`terminal-box-drawing.mjs`、`terminal-horizontal-rules.mjs`）在本轮未运行；随后已在同机夹具运行 `terminal-horizontal-rules.mjs` 并通过（见上方 2026-10-24 节），真实 WebView2 的像素对齐仍待确认；没有真实 Tauri/Windows 长会话性能数据，不能据此宣称每批成本已按比例下降。

## 2026-10-22：输入 Promise 链在关闭后仍占额度与重连迟到回调

继续检查异步队列的资源所有权。原 renderer 输入队列以 Promise 链保序，`dispose()` 仅设置标记；一个已经发出的 IPC 长期不返回时，后续尚未发送的输入、失败回调及其全窗口额度都无法释放。改用明确的待发送列表，仍在微任务中按序发送并捕获事件发生时的 tab ID；销毁立即结算尚未发送项并清空数据/业务回调，已发送项的额度仍等实际完成或失败后归还，不提前放大共享预算。已发送项销毁后不再调用旧终端的失败处理。

重连调用移入 Promise 的回调，以便同步抛错也进入既有失败提示和状态收尾；发起、失败和完成路径均检查 runtime 是否已经销毁，避免关闭后继续写提示或改动共享 refs。焦点和输入失败回调增加相同守卫。本轮仍按用户要求不新增或运行测试、不执行门禁或桌面验证；这些源码修补不能解释或确认修复此前 1 GiB 的 WebView 占用。

## 2026-10-22：继续对照关闭标签后的订阅与延迟回调保留

继续沿 Tabby 的 IPC listener 保留和后台输出路径检查。`subscribe` / `subscribeReady` 在原生退订失败时保留空转回调，原实现仍通过闭包引用业务 listener，可能持有已关闭终端。改为取消订阅时立即清空业务 listener 引用，原生退订确认和迟到事件防护仍按原顺序执行。

终端输出模块取消几处重复嵌套、未登记的 rAF，统一走已有可取消的 resize 帧；单独的 surface refresh 帧增加登记和销毁取消。合并 resize 时累积保留视口的请求，避免后续普通 resize 覆盖。输出、断线后的写入回调和 resize 增加销毁守卫；OSC 52 查询在异步剪贴板读取后检查销毁状态与目标标签，避免关闭后向复用的标签发送迟到回复。

代码路径检查未发现 Tabby 未聚焦标签那样的无界前端初始输出缓存：后台终端保持挂载并订阅，renderer transcript 限制 200,000 字符，native 输出按窗口最多四批在途且等待解析 ACK。以上仅为源码结论，本轮仍按用户要求不执行验证，不能证明此前约 1 GiB 的 WebView 占用已解决。

## 2026-10-22：对照 Tabby 的销毁与未终止 OSC 问题补齐清理

对照 [Tabby #11718](https://github.com/Eugeny/tabby/issues/11718) 检查现有实现：WebGL 光标闪烁管理器已有显式释放，输出 ACK 已等待 xterm 解析完成，renderer transcript 也已有长度限制。本轮补齐 xterm 6.0 隐藏 resize idle 队列与 addon-webgl 0.19 context 恢复定时器的销毁，将私有清理集中到 `xterm-adapters/terminal-disposal.ts`；runtime 销毁增加重复调用守卫。

另外发现 Rust 本地终端 OSC 7 CWD 跟踪器的长度限制失效：未终止 marker 位于偏移零时，原来的 `rfind` 裁剪保留全部缓存。改为超限只保留可能跨包的 marker 前缀，完整超长 OSC 不再用于目录更新；CAN/SUB 或新 escape 会中止当前 OSC，并保留下一个 escape 供后续目录标记解析。该旁路跟踪器不修改送往终端的原始输出，不能据此归因先前 WebView 约 1 GiB 的占用。

按用户要求本轮仅修改代码与维护记录，未新增或运行测试，未执行类型、静态、格式或桌面验证。改动效果与长期 native/GPU 内存趋势待后续验证，整体任务保持进行中。

## 2026-10-22：现场报告的 workspace 重放与 DOM 开销

核对用户 app-log-1021.md 的完整内嵌样本为 beta.11；内容触发 DOM 回退已有修复，不能据此认定 WebGL context 崩溃。生产 React 受阻渲染复现 300 个监控事件执行 90,600 次 updater，改为按 IPC 顺序更新独立快照并由 useSyncExternalStore 订阅后为 300 次。减少 symbol observer 重扫、复用实际字体样式与校准网格；十终端目录/监控/传输对照通过，但原生远端刷新错位、长期 native/GPU 增长、DOM block 开销和既有粗体比例失败仍未解决。证据与门禁见 [本轮复核](../../quality/terminal-log-followup-2026-10-22.md)。

## 2026-10-21：Windows WebView2 纹理槽保留

在当前 Windows Tauri 开发版和独立 Claude Code PTY 中接续 PR #351。真实 WebView2 复现字号变化后 CPU 图集降至 1 页，而历史纹理请求仍为 47,535,432 字节；修复未使用槽与旧 mip levels 后降至 1,398,160 字节，临时 runtime 销毁归零。两轮真实 Claude 短请求完成；12 分钟逐进程记录有峰值后回落，但不能归因于修复，也不能代替数小时/整夜负载验收。环境、原始本地归档、门禁与限制见 [Windows 纹理接续记录](../../quality/terminal-windows-texture-storage-2026-10-21.md)。整体长期卡死任务继续保持进行中。

## 2026-10-21：RGB 图集增长与清理后旧字形保留

先提交上一轮修复 `ae67e84f`。新增 RGB 输出对照，修复前 6,000 批图集约 127 MiB、GC 后 heap 约 82 MiB；修复后同批并额外持续五分钟，图集稳定 33 MiB、GC 后 heap 约 9.5 MiB；修复重复清理仍保留旧字形，以及清空后的合并页面未复用。缓存回收按共享终端最大视口保留工作空间，不改原始颜色与终端历史。扩展长测和 Windows 验收边界见[缓存增长复核](../../quality/terminal-webgl-cache-growth-2026-10-21.md)。

## 2026-10-21：块字符误回退与共享图集失效

先提交日志设置与字重改动 `91eac3cd`。已复现块字符强制回退 DOM，以及一个 WebGL 终端清理共享图集后另一个终端保留旧坐标的错画；移除内容回退并同步共享图集模型失效。双终端像素差异由 1,316 降至 0，失败回退保留并改为 WARN。五分钟混排持续输出未复现卡死，图集稳定于 13 MiB，关闭后登记归零；该结果仅来自本机 Chrome，不能代替 Windows 现场验收。范围和待验收项见[本轮报告](../../quality/terminal-webgl-fallback-shared-atlas-2026-10-21.md)。真实 Windows/Claude Code 与整夜冻结仍未确认解决。

## 2026-10-21：协议回复绕过 React 更新与 schema 3

交叉核对 beta10 报告和本机剩余 beta11 轮转日志，修复 xterm 自动查询回复误入活动标签/IME/菜单更新路径：已连接后台终端也能回复查询，自动回复不清选区或更新菜单；串口占用、历史回放与初始 ConPTY 回复规则保留。新增 `terminal-input` 阶段、异常时解析/ACK 快照和最慢四个长任务时间线，启动标记升级 schema 3。DOM/WebGL 生产 React + xterm 查询与监控回归通过，现场 #185 唯一原因和无脚本长帧仍未确认。证据、边界与外部归档文件名见[本轮复核](../../quality/terminal-log-analysis-repair-2026-10-21.md#2026-10-21beta10-与本机-beta11-日志交叉复核)。

## 2026-10-21：根据 Windows 现场日志修补事件与着色热路径

选区/搜索的 React 状态发布改为按帧合并；日志着色复用未变化行的匹配/映射，并限制附加缓存预算。补充命名回调失败/耗时、按终端/后端资源与 schema 2 就绪标记，已核对真实 app.log 落盘。171 个 renderer 测试、类型、定向静态/格式、WebView2 混排/回退检查通过。旧 #185 尚未复现确认根因，不能标为完全解决；完整 Rust 门禁受运行中 exe 文件锁限制，lib/CLI/contract/scheduler 分项及 clippy 通过。详情与后续长测步骤见 [日志分析与修补记录](../../quality/terminal-log-analysis-repair-2026-10-21.md)。

> 2026-10-21 字形回归更新：用户真实会话发现先前小数网格候选存在断笔/拖影。已删除 WebGL 小数宽度覆盖，恢复上游整数像素图集，仅保留 backing resize 同步。52 组字体/DPI 与实际 WebView2 独立临时终端验证通过；初始已有会话回退 DOM，后续门禁触发开发版自动重启，详见记录。此前候选的网格一致和短测性能数据不能作为当前候选视觉或性能验收依据。详见 [字形回归修复](../../quality/terminal-renderer-windows-2026-10-20/font-regression-fix-2026-10-21.md)。
>
> 2026-10-21 renderer 设置：Windows、macOS 和 Linux 均可在“设置 → 界面 → 终端显示”选择 DOM 或 WebGL。新安装及旧 `state.json` 缺少 renderer 字段时默认 WebGL；已保存的选择会保留。已打开终端需重启后切换；WebGL 加载失败、激活失败或 context loss 会回退 DOM。真实 Claude Code 长任务和中文 IME 仍待验证，不能把自动 soak 当作交互长测结论。

## 2026-10-21：Windows DOM / WebGL 两小时 React workspace soak

同机顺序运行 DOM 与 WebGL 两小时合成 workspace 负载，分别 239 和 237 个采样。两轮均无 React #185、
Maximum update depth exceeded 或 pageerror，ACK 在途最多 4、历史最多 200，卸载后终端与订阅归零。
WebGL 轮全程 10/10 硬件加速 renderer、无 context loss 或 DOM 回退；237 次进程组采样显示 working set
860–877 MiB、private bytes 1249–1274 MiB，前末窗口均值变化较小。JS heap 约 48.6–50.9 MiB。

CDP `Memory.getDOMCounters().nodes` 在 WebGL 负载中每个采样恰好增长 1，338→574；挂载元素数固定
263，卸载后 nodes 降至 135。记录为活动期节点计数待查信号，不据此判泄漏；DOM 轮没有进程组数据，
不能作 renderer 进程内存对照。WebGL 原始记录和详细统计见 [Windows 对照报告](../../quality/terminal-dom-webgl-comparison-2026-10-20.md#windows-react-workspace-两小时-renderer-soak2026-10-21)。

此结果仅覆盖合成 React / Chromium 负载。真实 Tauri WebView2 六小时 idle 观察已采集 356 条
renderer-health 与 359 条完整 Windows 原生样本：目标 WebGL PTY 全程为 WebGL，未见解析/ACK 错误、
React 错误签名或 context loss；一条无脚本归因的 3.879–4.407 秒长帧仍未解释。进程组内存峰值后
回落，但 idle 观察不能证明泄漏已解决。六小时预期时长已过，shell 提示符未被 app.log 记录，故
`Start-Sleep` 返回无法独立确认；筛选日志和摘要见 [Windows 记录](../../quality/terminal-renderer-windows-2026-10-20/README.md#current-development-window)。
该 soak 记录形成时，真实 Claude Code、持续 PTY/ACK、中文 IME 与物理选区仍待交互验证，默认仍为 DOM；当前三平台默认状态见本计划顶部更新。

## 2026-10-21：修复 Windows WebGL 网格与 backing resize

显式开发版 WebGL 按现有 DOM 原始字体度量计算网格，未套用实验夹具的 DOM 取整修正。
同时修复 ResizeObserver 更新 backing buffer 后 GL viewport/字形 uniform 未同步、首行被裁切的
问题。激活与回退后重新 fit；未知私有结构回退 DOM，销毁时恢复方法。

52 组原生 DOM / 生产 WebGL 字体/DPI 矩阵、延迟字体、3 档 resize、首中末列命中、选区、
搜索、回放和 GL viewport/backing 一致性检查通过。真实 WebView2 155.0.4283.62 按实际容器复测：
1228×716 内容区两侧初始均 183×49，JetBrains Mono 14 均 144×37；字体切回后原生 DOM 与 WebGL
均为 184×49，context loss 回退保持该网格，三轮 dispose 清理正常。FitAddon 字体往返时原有的一列
取整差保留，两后端结果一致；早期无实际内层容器的 188/194 列记录仅作历史。
环境、源码 SHA-256、JSON 和截图见 [Windows 记录](../../quality/terminal-renderer-windows-2026-10-20/README.md)。
生产网格 Windows Chromium 12 轮完成：常速 4,000 批 DOM/WebGL 中位数 28.66/11.10 秒、
CPU 38.41/8.66 秒、下一 rAF p95 43.8/34.6ms；6× 200 批为 18.29/5.80 秒、380.6/123.8ms。
全部解析与模拟 ACK 完成、最终积压为零、context loss 为零；这不是 WebView2 端到端结论。
此前短测属于修复前且新 smoke 补充实际容器结构/初始 fit，不直接混算性能。该次验证之后默认仍为 DOM；真实 IME、
PTY/ACK 和 Claude Code 长任务仍待交互验收。

## 2026-10-21：Windows WebView2 renderer smoke（修复前）

在 PR #348 分支 Windows 11 Tauri 开发版中分别运行 DOM/WebGL，WebView2 155.0.4283.62，
RTX 3070 Laptop / driver 32.0.16.1742 / NVIDIA ANGLE D3D11 hardwareAcceleration=true，
显示缩放 125%。真实 WebView2 内创建临时内存 xterm host，回放 1,500 行中文/emoji，检查搜索、
选区、resize、当前字体与 JetBrains Mono 14，并执行三轮 renderer create/dispose。context loss
成功回退 DOM，dispose 清空 host 节点。真实过程树各采 35 次（约 38 秒），JSON、截图及过滤的
renderer 事件保存在 terminal-renderer-windows-2026-10-20 目录。

本次单轮 smoke 中 xterm write callback 是 DOM/WebGL 22.7/34.9ms，callback 到下一 rAF 是
10/28ms；短时过程树 working set 峰值 637.6/631.9 MiB，private bytes 679.6/536.2 MiB，
CPU 增量 6.59/2.05s。只有一次合成 run，不能当作性能结论或泄漏判定；callback/rAF 与 CDP task
不能分别代表绘制完成。125% DPI 下生产格网仍不一致：当前字体 188/194 列，JetBrains Mono 14
为 147/155 列；实验校准没有接入生产代码。WebView2 in-memory smoke 不经过 PTY/Rust IPC，
原生输入延迟、parse/ACK 积压和 Windows IME 均无法采集；该次验证之后默认仍为 DOM。

下一步由用户在当前显式 WebGL 开发窗口先完成真实终端的字体/DPI/resize、CJK/emoji、IME、
选区、搜索和回放检查，再用同一窗口/会话/只读 Claude Code 工作负载分别跑 DOM 与 WebGL 长测。
采集 app.log 阶段日志、native ACK 年龄和进程树连续趋势后再判断是否有实测改善或内存回归。
交互步骤与具体复现命令见 ../../quality/terminal-renderer-windows-2026-10-20/README.md。

## 2026-10-20：Windows GPU Chromium 预检与 Tauri 开发开关

最初基于 PR #348 分支增加 `VITE_FILETERM_TERMINAL_RENDERER` 开发版对照开关；后续 Windows 设置页已提供
DOM/WebGL 选择：新安装和旧配置缺少 renderer 字段时默认 WebGL，之后保存的用户选择跨版本保留。
加载失败、WebGL 初始化失败和 context loss 都回 DOM；终端销毁时释放 addon、主动丢失
WebGL context，并在 renderer 变化后重新挂接 xterm 阶段诊断。普通生产版不记录默认 renderer 事件。

Windows 11 Pro 10.0.26300 / RTX 3070 Laptop / driver 32.0.16.1742 上，headless Chromium 156
使用 NVIDIA D3D11 硬件路径。52 组字体/DPI、3 档 resize、中文/emoji、选区、搜索和回放检查通过；
真实 IME 及 Windows 原生 DPI 交互仍需 Tauri 手测。独立浏览器 4,000 批常速三轮中位数：DOM/WebGL
完成 25.47/12.43 秒、进程树 CPU 36.19/9.63 秒、下一 rAF p95 39.7/30.5ms；峰值进程树 working set
700.8/492.3 MiB。6 倍节流的 4,000 批 DOM 超过 180 秒门限，仅完成 1,747 批；因此最终 6 倍短压测
每组改为相同的 200 批三轮，DOM/WebGL 完成 51.80/11.50 秒。两档不混算，ACK 在浏览器夹具里是即时
模拟。所有已纳入轮次解析/ACK 最终积压为零。它们是硬件加速 Chromium 预检，不是 WebView2 结论，
也不构成长时泄漏判定。逐秒数据和复现步骤见[Windows 对照报告](../../quality/terminal-dom-webgl-comparison-2026-10-20.md)
及其 [Windows 原始记录和交互步骤](../../quality/terminal-renderer-windows-2026-10-20/README.md)。

当时启动的 Tauri 开发版 PID 为 40504，版本 `2.3.1-beta.10`，WebView2 runtime `155.0.4283.62`；
PID 40504 后续已退出。2026-10-21 的真实 WebView2 smoke、当前开发窗口和仍待执行的交互长测，
以本计划顶部的最新记录为准，不据短测推断长期泄漏已解决。

## 2026-10-20：补足下一轮现场日志的阶段证据

沿用既有 app.log、每分钟健康报告和限频卡顿告警，增加八个同步阶段的调用数/累计/最大
耗时、有限慢步骤、终端网格/历史/解析积压数字、canvas 存储估算和有界 ACK 年龄跟踪。
解析/绘制使用带兼容性检查的 xterm 6 私有方法，关闭时恢复方法并注销资源引用；不改
解析结果、信用额度或确认顺序。异常附当前阶段摘要，仍按原错误正文去重。

可用时追加 Long Animation Frames 最慢四帧、最多三项脚本归属和强制布局耗时；至少
一秒长帧在恢复后额外触发每 30 秒最多一次的 WARN。只保留本应用打包文件名/函数名/
字符位置，无终端正文、完整 URL、远端路径或逐批日志。API 不可用会明确标记。
这些是同步墙钟时间和存储估算，不能直接相加为 CPU 时间或解释全部 RSS，也不包含
GC、原生分配与完整调用栈。旧日志不能补采，Windows 原生数小时复测及发布仍待完成。
字段边界见 [日志文档](../../quality/logging.md#解析绘制与内存占用的补充采集)。

本轮类型检查、162 项 renderer、801 项 Rust 单元、10 项 CLI、23 项 contract、3 项 Windows scheduler、
严格 Clippy 与 `cargo fmt --check` 通过；仓库级 lint/Prettier 受 491 个既有 CRLF/格式文件阻塞，
本次变更文件的定向 ESLint/Prettier 检查通过。
Chromium 真实终端回放确认阶段采集有效，4,000 批背压回归全部确认、最大在途四批；
监控更新深度回归覆盖 360 次提交与 200 批 ACK。
Chromium 十终端混合负载 60 秒确认 4,758 / 4,758 批，最大在途四批；反复替换后
诊断注册和实际存活终端仍为十个，全部卸载后均为零，解析队列、marker 和订阅清空。

## 2026-10-20：定位 React #185 并移除监控图表的重复状态更新

重建得到与现场相同的 `index-DcpQ0jto.js`，hidden source map 将 `131:29` 精确定位到
xterm `onData` 中的 `setContextMenu(null)`。两层 Emitter 是 xterm 正常转发，不是
Tauri/workspace 递归。继续用实际监控侧栏和终端组件构造同步提交洪峰，旧代码的图表
派生状态和磁盘回退 effect 会留下重复更新，触发 #185；修复后相同夹具通过。

图表直接从指标生成曲线，420ms 滚动只更新 SVG transform，不再保存样本副本或每帧
写 React state；磁盘失效回退和网卡恢复先比较实际值。未改变终端输入/解析/ACK，也
未切换生产 DOM 后端。浏览器覆盖 360 次监控/输入提交、网卡选择、断点、动画和卸载，
另确认 200 批输出全部解析/ACK。现场此前的全部提交来源仍未知，不能把合成复现写成
Windows 真机根因已最终确认；详见 [React 更新链证据与边界](../../quality/react-update-loop-audit.md#2026-10-20长会话日志与监控图表的重复更新)。

类型、lint、格式、155 项 renderer、879 项 Rust/contract、严格 Clippy、CSS contract 与
diff 检查通过。Chromium 十终端 60 秒混合负载完整确认 4,766 批，替换/卸载后解析队列、
marker 与订阅正常回收；这仍不能代替 Windows 原生数小时复测。

## 2026-10-20：校准 DOM / WebGL 实验网格

截图差异已定位为 xterm 6 DOM 的浮点格宽和 WebGL 0.19 的物理像素向下取整策略。
只为实验夹具统一物理格宽和 CSS 命中网格，不改生产 DOM、不切换默认渲染器；原始
字体测量、字体加载/DPI observer 和字形缓存策略保留。52 组浏览器回归通过，覆盖
字体/小数字号/DPI 切换、延迟导入、FitAddon、中文/emoji 占格、选区和首末列命中。
同网格性能矩阵已完成 12 轮，全部确认 4,000 批、最终积压为零。常速 DOM/WebGL
完成时间中位数为 22.35/10.47 秒，6 倍降速为 154.20/45.62 秒，下一 rAF p95
为 116.2/54.8ms。校准只作用于实验夹具；旧未校准数据单独保留，不能混算。
类型、lint、格式、155 项 renderer、879 项 Rust/contract、严格 Clippy 和 diff 检查通过。
方法、数据及平台边界见 [对照报告](../../quality/terminal-dom-webgl-comparison-2026-10-20.md)。

## 2026-10-20：初轮 DOM / WebGL 对照实验（未校准）

着色/布局去重和输入总预算已提交 `1d6f0e97`。下一步只在独立浏览器夹具比较当前 DOM
栈与 WebGL 候选栈；插件固定 `0.19.0` 并放在临时目录，不增加生产依赖或改变默认渲染器。
相同 6,000 行历史、10 个终端、4,000 批/4 信用输出，常速与 6 倍降速各三轮，交替顺序。
采集浏览器进程 CPU 秒、RSS、JS 堆/backing storage、图集估算及真实终端键盘事件延迟，
12 轮均完整确认 4,000 批且最终积压为零、context loss 为零。三轮中位数：常速
DOM/WebGL 完成时间 19.11/10.44 秒、CPU 时间 28.07/9.60 秒；6 倍降速为
126.29/47.04 秒、CPU 时间 143.85/52.56 秒，输入至下一 rAF p95 为 106.2/56.9ms。
重绘期间采样 RSS 峰值约低 10%，但 GPU 进程 RSS 更高、GC 后 JS 堆基本相同，不能
推断现场泄漏已解决或长期常驻内存一定减少。Windows 原生和数小时 CC 尚未验证。
完整方法与结果见 [对照报告](../../quality/terminal-dom-webgl-comparison-2026-10-20.md)。

## 2026-10-19：减少重复着色/布局并限制输入积压

上一轮输入合并已提交 `90b754df`，本轮按着色/布局、输入总预算的顺序推进，WebGL 未改动。

- 日志着色先从 xterm 文本匹配，命中规则才建立 UTF-16 到终端列的映射；未命中行不再逐格
  建映射。已有语义颜色仍适用时保留原值，只恢复失效的范围；最终前景没有变化就不额外
  refresh。普通输出与连续 100 次相同着色行回归验证零额外刷新；`clear` 的历史扫描由两次
  减到一次。解析回调及 ACK 仍在解析完成后返回，不等待绘制或 DEC 2026 结束。
- 符号适配器在读几何前排除已拟合的来源节点；重复通知不再触发布局读取。真实符号、
  CJK 标点、框线与混合行仍走既有规则，没有新增逐行持久缓存。
- 主进程统一输入额度为 16 MiB，单次输入最多 1 MiB（UTF-8），每条消息至少记账 64 字节
  以约束小消息/空消息的队列开销；另有 4 KiB 控制字节余量。额度随输入所有权穿过入口、
  SSH 启动暂存、合并和后台写入，直到写入完成/失败、取消、队列关闭才释放。普通 worker
  命令和精确活动终端命令也通过同一入口校验；活动标签/会话 revision 安全锁保持不变。
- 本地 PTY 的线程队列也持有额度；Telnet 写入及串口行/Hex 未完成输入保留额度。串口
  行发送/取消后释放保留对象及空缓冲容量；本地内部查询回复遇到满额不因此断开 shell。
- renderer 在每个 WebView 内跨终端共享 1 MiB/4,096 条待发送额度，另留 1 KiB/16 条控制
  消息空间。超限拒绝整次输入并明确提示，不拆开粘贴、不按断线处理；提示最多每五秒
  一次，主进程不对预期满额错误逐次写 WARN，避免背压转成诊断洪峰。

上述是排队输入额度，不是应用 RSS 上限；字符串/编码副本、队列对象与协议库内部缓冲另有
开销。控制余量避免仅因普通字节额度耗尽而拒绝控制字节，不绕过既有队列顺序和容量限制。单次超过 1 MiB 的粘贴
会被明确拒绝，需分段发送。现场 Windows 节能模式和数小时 CC 占用仍需新版真机复测。

验证通过：类型、lint、全量格式、153 项 renderer、879 项 Rust/contract（846 单元、10 CLI、
23 contract）、严格全目标/全功能 clippy、rustfmt 与 diff 空白检查。真实 xterm 的 6 倍
CPU 降速、4,000 批回放约 87.45 秒完成，4,000 次全部确认，最终待解析字符/回调均为零，
10 次输入/点击完成（组合样本 399–1,164ms）；首轮只做文本预筛选约 94.13 秒，前次合成
检查约 92–93 秒。单次运行存在噪声，不能据此认定真机有固定比例提速。4 组 ANSI/反色/
鼠标选区浏览器回归中，修补后符号像素丢失均为零。Windows 原生及长时间现场未验证。

## 2026-10-19：参考开源终端的 Agent 长任务处理

核对官方文档与实现，而非用不同架构的 RSS 数字直接排名：

| 参考                                                                                                                                                                                   | 已核对的做法                                                                   | 对 FileTerm 的判断                                                                                                                                               |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [VS Code 终端渲染](https://code.visualstudio.com/docs/terminal/appearance#_gpu-acceleration)                                                                                           | 默认 WebGL，失败回退 DOM；图形字符和重叠符号有 GPU 绘制支持                    | Windows、macOS、Linux 设置支持 DOM/WebGL；默认 WebGL，块字 glyph 可触发该终端回退 DOM。真实负载仍需现场对照，不能直接宣称 WebGL 更省内存。                       |
| [VS Code 输出流控](https://github.com/microsoft/vscode/blob/main/src/vs/platform/terminal/node/terminalProcess.ts) 与 [xterm 流控指南](https://xtermjs.org/docs/guides/flowcontrol/)   | 依据尚未解析确认的字符水位暂停/恢复 PTY，回调代表解析完成；批量确认减少调用    | FileTerm 已有每窗 4 批解析信用和有界输出队列，过载会跳过显示。共享 SSH 不能照搬暂停整个连接；输入方向仍缺两级通道的总字节预算。输出与键盘输入是不同方向的背压。  |
| [Ghostty 同步输出说明](https://ghostty.org/docs/help/synchronized-output)                                                                                                              | 明确列出 Claude Code 的大范围重绘/撕裂案例，建议应用使用同步输出和减少全屏擦除 | 所安装 xterm 6 的 `InputHandler.ts` / `RenderService.ts` 已实现 DEC 2026、行范围合并和安全超时。先验证碎片化帧及额外适配器开销；不能把撕裂报告当成内存泄漏证据。 |
| [Windows Terminal 渲染源码](https://github.com/microsoft/terminal/blob/main/src/renderer/base/renderer.cpp)                                                                            | 对可见区域与 dirty areas 求交后绘制，支持同步输出                              | 额外符号/着色工作也应围绕实际变化行合并。`TerminalLogColorizer` 当前每次写入完成都在解析确认回调之前扫描着色，是待剖析的实际热路径。                             |
| [kitty 性能设计](https://sw.kovidgoyal.net/kitty/performance/) 与 [WezTerm 帧率](https://wezterm.org/config/lua/config/max_fps.html) / [历史上限](https://wezterm.org/scrollback.html) | kitty 将子进程 I/O 与绘制分离、缓存字形；WezTerm 控制绘制帧率并限制历史        | 保留输入/输出字节处理，合并画面刷新；历史填充导致的内存增加应与无限积累分开。FileTerm 已设 6,000 行历史，不能仅靠调小历史掩盖其他问题。                          |

本地还确认 xterm 自带 IntersectionObserver 不可见暂停；确认回调属于解析而非画面完成。
因此后台节流下也必须能归还解析信用，不能等动画帧或 DEC 2026 结束才 ACK，否则完整帧
尚未接收完就可能用尽信用而互相等待。不能任意丢弃 VT 中间字节来合并刷新，这会丢光标、
模式和 OSC 状态；协议流必须保持完整，过载跳过仍是当前明确记录的保底限制。

建议先推进两项：针对现场用 CC 式重绘回放测量额外着色/布局开销，验证 DEC 2026 分片、
安全超时及隐藏恢复；同时收敛已确认的输入总字节积压风险。随后对照 DOM/WebGL 的 CPU、WebView2/GPU
进程内存和输入延迟，再决定是否引入 GPU 路径。当前是源码/文档对照，没有新增渲染后端，
未进行其他终端与 FileTerm 的同机 Claude Code 长时间性能对比。

## 2026-10-19：SSH 输入洪峰的单轮合并与剩余积压风险

WebView2 普通权限采集已提交 `db6c3144`。继续检查 SSH 输入、写入任务、Telnet 协商、
会话日志和终端 transcript。确认 SSH 输入的 `coalesce_terminal_input` 原先同步排空整个
无界队列；持续生产/大批小片段会拉长一次处理，公平 select 必须等它返回后才能再处理
输出与取消。现在每轮最多合并 64 条消息，累计达到 64 KiB 后停止继续取消息；不拆分
单条粘贴/转义序列，因此单条大输入可以超过阈值，该阈值不是输入总内存上限。

写入 SSH 前也将已排队的小片段按相同预算合并，避免主循环让出执行权后增加逐片段写入。
两侧均保留字节顺序，未引入输入丢弃策略；UTF-8 粘贴、原始非 UTF-8 字节、NUL、ESC 和
Ctrl+C 回归覆盖。修改前两条回归失败：洪峰在单次被完全取走、越过字节阈值后仍取走后续
Ctrl+C；修改后聚焦的 6 条合并相关测试通过。

**当时确认、已由本文顶部本轮改动收口的内存风险**：SSH 入口 `terminal_inputs` 及后台 `terminal_write_tx` 是无界
通道，对端持续不消费且仍有输入生产时，待写入数据可能累积。既有单次写入 2 秒超时
不等于队列总容量限制。本轮没有以满队列丢弃命令或断开整个 SSH 会话的方式替代输入
背压；后续需要覆盖两级通道、启动暂存、忙错误回传和 Ctrl+C 的端到端字节预算。
这项代码风险尚不能认定为现场重度 CC 的内存增长根因。

类型、lint、全量格式、149 项 renderer、876 项 Rust/contract、严格 clippy 与 rustfmt
检查通过。单轮合并预算已提交 `90b754df`；Windows 节能模式现场及慢 SSH 写入积压仍未真机验证。

## 2026-10-19：普通用户权限下补充 WebView2 内存

上一轮诊断限频/连接冷却记录清理已提交 `23d313ca`。现场两组原生记录都只包含 FileTerm
主进程，不能据此推断 WebView2 没有内存占用。新增主窗口/子窗口的浏览器 PID 登记，
利用 WebView2 自身给出的身份补充 Toolhelp 父子树；采样前校验创建时间防止 PID 复用。
采样仍在既有独立线程每分钟进行，不依赖页面心跳，不新增定时器或逐事件日志。

去掉 `PROCESS_VM_READ`，普通采样只用 `PROCESS_QUERY_LIMITED_INFORMATION`。登记、查询
和枚举失败明确保留阶段/错误码；记录实际找到的 WebView2 数、成功内存读取数、登记状态
和截断情况，缺失不是零。窗口关闭删除登记，单条采样内 PID 去重；共享浏览器可能跨实例
重复出现，不能把同 PID 内存重复归到各实例。详细字段见 [本地诊断日志](../../quality/logging.md)。

验证：类型、lint、全量格式、149 项 renderer、874 项 Rust/contract、严格 clippy 与 rustfmt
通过；进程筛选测试覆盖外部浏览器根、逆序枚举、去重、其他应用/命令后代排除。新增 Windows
模块及其测试使用仓库锁定的 Tauri 2.11.5 / WebView2 COM 接口交叉编译、严格 clippy 通过，
覆盖有限权限调用及窗口登记释放测试的编译。Windows 整包交叉检查仍被本机缺少 SDK 的
`assert.h` 阻断，不能把独立模块检查当作整包构建或 Windows 测试执行通过。

WebView2 采集改动已提交 `db6c3144`，尚未发布。Windows 原生权限/域策略与实际 WebView2 进程内存
尚待真机验证；不要求反馈者提权。

## 2026-10-19：检查订阅与后台记录生命周期

上一轮纯框线候选过滤已提交 `01e2b7f1`。继续检查监控事件订阅、原生窗口输出订阅、
后台命令、连接测试和会话关闭路径。确认两处独立于终端历史的少量累积：

- `use-workspace-ipc-sync.ts` 的监控日志限频 Map 在整个 hook 生命周期保留每一个
  用过的标签 ID；关闭会话后不删除。现在只在接受有效工作区快照后清理已移除会话，
  旧版本快照不会触发清理。
- 连接测试的冷却 Map 在进程生命周期保留每一个测试过的 profile/endpoint 键。
  现在在下一次测试时移除超过既有 5 秒冷却期的记录；并发测试的独立 in-flight
  防重入表不受影响，没有加入定时任务。

直接调用生产快照回调的临时断言验证 1,001 条限频记录清至 1 条，过期快照保留活动记录；
从生产连接测试冷却块提取的独立 Rust 断言验证 10,000 个过期键释放、有效键仍拒绝重试。
这些是小型历史记录累积，不足以认定是 beta.9 卡死或百 MB 增长的原因。
原生终端订阅已在 Destroyed 时移除并取消转发任务；本轮没有改变该机制。

类型、lint、全量格式、149 项 renderer、872 项 Rust/contract、严格 clippy 和 rustfmt
检查通过。该轮两处清理已提交 `23d313ca`；Windows 无响应现场及数小时进程内存变化仍未验证。

## 2026-10-19：提交后继续检查累积开销

上一轮 DOM 合并与现场诊断已提交 `b3ece66f`。继续检查符号/框线、选区、日志着色、
共享字体测量缓存、终端生命周期、SSH 输出泵、CWD/root 提示解析与刷新队列。
新增确认的重复工作：纯框线行在符号适配器仍触发布局读取与缓冲行扫描，随后因框线由
图形适配器负责而没有任何字体适配更新。过滤候选行时先排除 U+2500–259F；真实的
非图形符号、CJK 标点和混合行保留原处理。只修改候选判断，不改字体测量/绘制结果。
回归在修补前因纯横线触发一次布局读取失败，修补后为零；混合符号路径仍有布局读取。

一分钟真实 xterm 检查使用 10 个终端，每个先填入 6,500 行以占满 6,000 行历史，
再持续高变化 RGB/组合字符重绘：每终端完成 575 次写入，颜色对峰值 7,991、字宽
12,547，均在已有上限内；未变化的 palette 扫描历史行数为零，空闲无重复布局检查。
终端销毁并反复创建/销毁 20 次后，DOM 从 7,920 回落至 30。该结果确认本次合成负载的
缓存/节点有界，没有测量 Windows 进程堆，也不证明不存在数小时后出现的泄漏。

本轮验证通过：类型、lint、全量格式、149 项 renderer、872 项 Rust/contract 与严格
clippy；24 组横线像素回归和符号的 ANSI、反色、鼠标选区回归通过，修补后的符号
像素丢失计数为零。该轮符号候选过滤已提交 `01e2b7f1`；Windows 节能模式现场未实测。

## 2026-10-19：补充现场输出进度与交互耗时

现有 beta.9 附件已读取；无需用户重复导出同一批日志。当前分支补充应用内诊断，
新版构建才会包含这些字段：窗口订阅的接收/解析/确认计数与最近批次 ID、解析/确认失败、
待处理批次/字符、距处理进展时间；原生饱和告警附带窗口输出进度，页面心跳停止时也有证据。
交互通过 Event Timing 汇总 keydown/click 的派发等待与至下一次绘制耗时，不记录内容，
支持情况明确标注。仍使用既有每秒心跳和每分钟健康记录，异常最多每 30 秒一次，
不添加逐批/逐按键日志或后台任务，不要求管理员权限。

局限：页面完全卡住时前端采样需恢复才能提交；原生过载告警与页面记录需要交叉对照，
不能保证捕获永久停顿期间每一次交互。详情见 [本地诊断日志](../../quality/logging.md)。

验证：类型、lint、全量格式、147 项 renderer、872 项 Rust/contract、严格 clippy 与
rustfmt 全部通过。新增回归覆盖解析挂起、确认拒绝、进度 ID、隐私、告警限频、空闲恢复、
Event Timing 不支持及清理；真实 Chrome 的点击/键盘采集得到 3 个样本，最大绘制耗时
160ms。6 倍 CPU 降速的 4,000 批正式回放约 92.5 秒完成，峰值 4 批待确认、最终积压为零，
原 120 秒超时未调整。该轮改动已提交 `b3ece66f`，尚未发布；未实测 Windows 节能模式现场。

## 2026-10-19：6 倍 CPU 降速的 4,000 批超时

上一轮异常恢复已提交 `630a69f9`。继续使用相同十终端、每窗四个待确认批次和
4,000 批 ANSI / CJK / 横线 TUI 输出，临时允许更长观测并每十秒记录进度与 CPU profile。
旧代码在 110 秒已确认约 2,664 批，最终约 164.7 秒全部确认，待显示正文、completion
和 xterm 队列均为零，DOM 节点稳定约 6,369。因此此夹具的 120 秒超时来自吞吐不足，
没有复现 ACK 永久停住；这不能排除 Windows 现场另有冻结路径。

主线程 CPU profile 的 `getBoundingClientRect` 自耗时合计约 64.7 秒，主要发生在图形
DOM 更新之后的符号适配布局读取。临时关闭符号适配仅小幅改善，关闭块 / 框线适配则
大幅加快，指向后者生成的 DOM 与其后的布局成本，未将这些关闭措施用于产品。

修补保持按格位图绘制，仅将同一原始文本节点里的连续 U+2500–U+259F 图形合为一个
透明占位 span。原来 55 字符横线会创建 55 个占位 span，十终端夹具约 6,369 个 DOM
节点；现在约 1,509 个。不会跨原始 ANSI / 选区样式来源合并，普通文字段、原始文本、
背景、逐格像素几何和复制内容沿用原路径，不修改字体测量或关闭适配器。

同机一次带 profile 的 6 倍降速完整回放约 86.3 秒，4,000 批全部确认，布局读取自耗时
降至约 26.3 秒，在途不超过四批，末尾积压为零；该模拟与计时不等于 Windows 真机
节能模式、主进程内存或长期冻结已解决。正式回归保留原来的 120 秒时限，补充十秒
进度和总耗时，超时时能区分仍在消费与停止消费；不会在应用中新增轮询或日志流。
无 profile 的正式 6 倍降速 / 4,000 批回放约 93.2 秒完成，6 个故障恢复探针通过，
在途最多 4 批、末尾待显示正文与 completion 为零，十次输入 / 点击全部完成。

反馈者补充了保留性能数值的同时间段日志，328 条附录记录的时间与前版一致。
`PROC-1` 对应 portable（采样器约 35–46 分钟），`PROC-2` 对应安装版（约 216–227 分钟）。
12 条 renderer-health 的起点与 PROC-1 一致，未见对应 PROC-2 运行起点的页面健康采样。
安装版原生私有内存约 131.5–154.4 MiB，期间有回落；两组采样都只包含 FileTerm 主进程，
没有 WebView2 子进程。`batterySaver=0` 与主进程 throttle mask 为 0 不能据此排除
未采到的 WebView2 节流，也不能等同用户所指的任务管理器效率模式。新增数值未证明
安装版 JS 泄漏或解析异常，保留现场根因不确定性；无需管理员权限才能使用本轮修复。

新增连续横线占位数量回归在旧实现以 24 对 1 失败。横线像素 / ANSI / 选区 / 主题 /
清理回归在 1、1.25、1.5、2 倍 DPI 和 14 / 24px 下通过。另完成 JetBrains / Cascadia、
12 / 13 / 14 / 24px、四种 DPI 的 32 组框线目录与生命周期回归，以及两种 DPI / 两种字号
的选区像素回归；覆盖 ANSI / 反色、颜色恢复、普通 / 备用屏幕、resize、静止停止重绘与
清理。类型、lint、全范围 Prettier、Rust 格式、严格 Clippy、142 项 renderer 与
872 项 Rust/contract（839 unit、10 CLI、23 contract）通过。CSS contract 前后相同，
保留既有 11 处 !important 告警，无新增样式债务。未做 Windows 原生实测，计划保持 active。

## 2026-10-19：现场日志对照与解析异常恢复

反馈者确认安装版与 portable 同时运行：约 35 分钟的一组为 portable，约 3.6 小时
的一组为安装版。安装版重度使用 Claude Code，节能模式会立即增加 skipped，并报告
提示数值约 3,200 时整窗无响应。该数值表示跳过字节，不是触发冻结的内存阈值。

提供的约 12 分钟脱敏日志保留 23 次队列告警，但 PID 和部分性能数值被替换为 PORT，
因此不能把 portable 的健康页面与安装版告警直接对应。JS 堆平稳的记录不能排除另一个
实例的故障；两组原生私有内存都有回落，且缺少 WebView2 样本，不能坐实或排除长期泄漏。
`maxEventLoopGapMs` 包含 1,000ms 定时器间隔，报告中的约 1,002ms 不等于阻塞 1 秒。
普通账号不能采 ETW 不影响解析链路修复，不要求修改域权限。

本轮先复现并修补两个代码中的永久停流缺口，尚未确认它们就是现场根因：

- xterm `write` 同步抛错后，busy 标记永久保留，后续批次的 Promise 不再完成。
  现在清空 busy 和待显示正文，reject 所有受影响的 completion；bridge 沿用已有
  失败诊断和归还额度逻辑，下一批仍可写入。异常时受影响的显示批次被舍弃，
  原生 transcript 与已启用的会话日志仍在显示入队前处理。
- 可选日志着色在 xterm 的写入完成回调内执行，原先一旦抛错会跳过业务 completion，
  还会中断 xterm WriteBuffer 递增内部偏移与继续排空。现在捕获该后处理异常，
  禁用当前终端后续写入的语义着色，只写一次不含正文的 WARN，清理 marker 并继续
  completion；不会通过超时提前 ACK 一个仍在解析的批次。

回归先确认旧实现失败，覆盖首次写入拒绝、排空中途拒绝、多个合并批次失败后恢复，
以及着色故障不抛出、不漏 completion/marker、不重复告警。xterm 入队本身拒绝时也清理
预先创建的 marker；着色降级后 resize、clear、reset 与销毁不再访问失败的着色路径。浏览器回归使用真实 xterm
与 bridge，模拟四个同标签在途批次与着色异常，并在 CPU 降速下回放多标签 TUI 输出。
类型、lint、全范围 Prettier、Rust 格式和严格 Clippy 通过；142 项 renderer 与
872 项 Rust/contract（839 unit、10 CLI、23 contract）通过。真实 xterm 回归在正常
速度下完成 4,000 批、6 倍 CPU 降速下完成 600 批，6 个故障恢复探针均归还显示额度，
在途最多 4 批，最终待显示正文与 completion 均为零，输入和点击可完成。这是恢复和
容量验证，不作为吞吐或交互延迟改善证明。CSS contract 前后一致，无新增样式债务。

初次对照的 6 倍 CPU 降速 / 4,000 批超过 120 秒测试预算，未算通过，后续使用明确的
600 批降速回归；正常速度的 4,000 批完整回放通过。现场 Windows 节能模式和持续数小时
负载未实测，不能宣称现场冻结已解决，计划保持 active。

## 2026-10-19：限制诊断日志告警洪峰

上一轮已提交为 `c6f557d8`。用户明确要求继续修补日志洪峰，不让诊断本身拖慢应用。
旧实现普通队列满后，WARN/ERROR 每条创建 Tokio blocking task 或原生线程，慢磁盘下
已复现 4,096 条 WARN 积累 3,071 个兜底任务。

- 删除逐条兜底和写盘全局锁，所有级别共用一个 1,024 项队列与一个专用写盘线程。
  非阻塞接纳为 WARN/ERROR 预留 128 项，另为 ERROR 预留 32 项，保持实际入队的 FIFO。
  超过上限只累计对应级别的丢弃计数，不在业务线程写盘，也不再产生后台任务或线程洪峰。
- 写盘失败同样计数，丢弃摘要通常每 30 秒最多一条，排空时强制尝试；摘要失败保留计数。
  退出沿用总计 500ms 的入队 / 确认等待预算，不保证慢磁盘下完整保存日志。
- 回归阻塞真实 writer 的写入端，覆盖普通 / WARN / ERROR 容量保留、FIFO、恢复与摘要；
  八个生产线程分别在有 / 无 Tokio runtime 下各投递 80,000 条 WARN/ERROR，验证队列
  不超过 1,024 项且实际写入始终来自同一线程。另覆盖发送和确认两个阶段的退出超时、
  接收端关闭、摘要限频 / 失败重试、写盘失败计数与日志轮转失败保留原文件。

代价：持续过载时 WARN/ERROR 也可能丢失，摘要只能说明数量，不能恢复原文。完整策略见
[日志写入和限流](../../quality/logging.md#写入和限流)。本轮消除该积压路径，不据此认定
Windows 节能模式或用户现场的长期卡顿已经解决，计划保持 active。

验证：7 项 writer 回归通过；类型、lint、全范围 Prettier、Rust 格式与严格 Clippy 通过，
138 项 renderer 和 872 项 Rust/contract（839 unit、10 CLI、23 contract）通过。
原生 Windows 节能模式、实体慢盘和现场长时间负载未实测。

## 2026-10-19：Serial / Telnet 显示背压与日志分配排查

在 `fix/windows-terminal-memory` 保留并核对合入 main 的小批次合并、按标签轮转与模块拆分，
继续检查原生 / renderer 长期状态。本轮修补两条路径：

- **Serial / Telnet 绕过窗口队列。** beta.9 的 `emit_terminal_data` 直接向 IPC Channel
  推送，页面消费变慢时没有显示额度限制。现有窗口队列已经是非阻塞入队，无需为共用
  收发 worker 的协议绕过它。统一到该队列，删除直推方法与重复 emitter；在显示入队前
  处理原生 transcript 与已启用的会话日志，保留当前每标签 / 每窗口容量和轮转顺序。
  新回归在旧路径上因 100 批全部直推（预期最多 4 批待确认）失败；修补后分别验证
  Serial / Telnet 发布不等待页面、待发送容量 / 正文字节数有界、恢复确认后继续显示
  并带跳过计数。过载时屏幕显示可能缺少输出，设备是否支持 Ctrl+L 重绘由远端决定；
  未启用会话日志时，不能依赖有限的屏幕历史 / 原生 transcript 保存全部数据。
- **日志格式化逐字符分配。** `sanitize_fragment` 原先为每个普通字符临时创建一个
  `String`。改用栈上 UTF-8 编码和静态转义片段，保留脱敏、单行化、字节上限和控制
  字符格式。独立分配计数对照中，一万条 304 字节 ASCII 消息从 3,050,000 次分配
  降至 10,000 次，耗时样本从约 109ms 降至 18ms；这是该函数的 macOS 微基准，
  不能外推整机性能。新增字节边界回归，另对所有 Unicode scalar value 的八种截断
  长度与旧实现做结果相等自检。

另保留两条需要现场区分的风险，不作为已修复或已确认的现场根因：

- **慢磁盘 / WARN 洪峰下日志兜底任务无上限。** 普通队列为 1,024 项，但满载的
  WARN / ERROR 每条创建一个 blocking task；无 Tokio runtime 时每条创建线程。
  使用生产 writer 的临时夹具持有写盘锁，连续投递 4,096 条 WARN，实际积累 3,071 个
  fallback task；释放锁后全部排空。夹具限制 blocking pool 为四线程，避免测试自身
  大量创建线程。这证明存在普通队列之外的积压路径，不能证明反馈者遇到了日志洪峰。
  本轮保留既有 WARN / ERROR 语义；后续限制需同时明确满载时的优先级、合并 / 丢弃
  计数和退出排空，不能只把无界任务换成无界队列。
- **SSH 输入的两级无界队列。** renderer 的输入调用在原生入队后即可完成，输入接收
  与 shell writer 都使用 unbounded channel；大量粘贴 / 自动输入而远端长期不读时
  可以积压。这要求持续输入，不能解释纯输出或空闲会话增长。写入已有两秒超时，连接
  child token 有 drop guard；本轮未发现无条件永久存活的 writer，未改输入与取消行为。

监控网络历史在 Rust / core 均限制为 600 个采样，传输历史与已测终端缓存的上限仍在。
首次快照等待期间的 metrics / remote-files 暂存数组及按 tab ID 保存的诊断时间表仍值得
结合现场检查；暂存不参与水化完成后的长期输出，诊断表也不是逐消息增长。
Windows 节能模式、真实设备及长期无响应现场仍待日志与真机验证，计划保持 active。

迁移到合并后的目录结构后，类型、lint、全范围 Prettier、Rust 格式与全目标 / 全特性
Clippy 均通过；138 项 renderer 和 865 项 Rust / contract（832 unit、10 CLI、23 contract）
测试通过。新增回归确认页面无确认时设备输出仍能返回、每标签待发送批次与字节数有界，
恢复确认后带跳过计数继续显示。此次未运行 Windows 原生长测，未发布版本。

## 2026-10-19：轮转、容量与重订阅的再次复查

本轮未发现需要修改生产逻辑的并发问题。检查范围包括所有发布和清理调用、
窗口注册与销毁、确认处理，以及 bridge 到 xterm 的解析完成链路：

- 轮转名单、批次和标签跳过计数由同一把队列锁维护；标签排空、关闭后重新加入，
  都只占一个轮转位置。等待显示额度时不提前取批次，不会额外保留一批等待额度的正文。
- 合并、新增批次及每标签 8 批、每窗口 128 批限制在同一临界区内执行。
  正文上限不包含已发送待确认数据、字符串分配余量和 IPC 开销，不作为进程内存上限。
- 重订阅先关闭旧额度并清空缓存；关闭与入队使用同一把队列锁。发送前登记待确认
  额度，释放锁后调用 IPC，允许确认同步返回；旧发送与关闭交错时，迟到确认仍按
  窗口、通道和批次匹配，不会释放新通道额度。已进入 IPC 的旧发送仍可能完成。

补充两项回归：标签排空、关闭、重新加入时的动态轮转顺序；以及把旧通道暂停在
发送回调内部，再重订阅、发送新数据并恢复旧确认，检查新额度不受影响、旧任务退出。
20 项输出链路回归以四个测试线程重复 20 轮（400 次执行）通过；120 项 renderer
测试与 824 项 Rust/contract 测试通过，类型、lint、Prettier、Rust 格式和全目标/
全特性 Clippy 检查通过。

未做 Windows 实机长测，未发布版本；持续过载时仍可能触发显示跳过保护。

## 2026-10-19：按标签隔离待发送容量与轮转

继续修补窗口过载：两个新增回归先确认旧实现会在第九个短消息标签入队时失败，
也会在一个标签填满八批后丢弃另一个标签的提示符。此前的小块合并没有解决窗口共享
八项容量的争用。

- 现在每标签最多 8 批待发送（单批最多 16 KiB，合计最多 128 KiB），窗口最多
  128 批（正文最多 2 MiB）；全窗待确认仍最多 4 批。单标签过载不再耗尽正常多标签
  使用所需的全部窗口待发送位置；真正超过每标签或窗口上限时仍按标签标记显示丢失。
- 就绪标签按轮转发送，各标签内部顺序不变；每轮每个就绪标签获得一次发送机会，
  不让队列头部的刷屏标签连续排空后才轮到低输出标签。不改变 SSH 收包、输入或
  SFTP 处理，也不让慢窗口等待阻塞其他窗口。
- 先取得显示额度，再从队列选择下一批，避免等待额度时提前固定一个标签的数据；
  队列统计也不再漏掉转发任务持有的一批待发送正文。没有新增定时器或凑批等待。
- 健康日志补充 `queuedBytes` / `queuedTabs`，与 `queuedBatches` 同一次非等待锁采样，
  锁忙时共同为 `null`。保持跳过计数、窗口关闭、重订阅和标签清理的既有隔离。
- 将接近 900 行的单文件按职责拆成 `terminal_output/mod.rs`（转发与订阅 facade）、
  `queue.rs`（容量与轮转策略）、`tests.rs`（原回归与新增回归），模块路径和外部 API 不变；
  测试单独维护，业务文件均小于 800 行。

原生链路回归现在以十个线程向十个标签各发送 128 KiB ANSI/CJK/emoji 数据，分成
5,120 个 256 字节小块。先占住四个已发额度，再积压完整 1.25 MiB；恢复确认后检查
逐标签内容与顺序、每轮发送机会、批次大小和零跳过。另覆盖低输出标签不受单标签
过载挤出、128 个微小流的窗口总上限，以及清理标签后的轮转名单一致性。

验证：18 项输出链路回归以四个测试线程重复 20 轮（360 次执行）通过，十标签累计
回放 25 MiB、102,400 个输入小块；类型、lint、全范围 Prettier、Rust 格式、120 项
renderer 测试、822 项 Rust/contract 测试与全目标/全特性 Clippy 均通过。拆分后
`mod.rs` 为 283 行，`queue.rs` 为 69 行，测试文件为 666 行；测试属于文件规模豁免。

本次没有把有限缓冲改为无限缓存，也没有把等待显示额度放回共享 SSH 收包循环。
现场长时间阻塞或持续输出超过解析能力仍需 Windows 原生诊断与长测，本计划保持 active。

## 2026-10-19：小批次合并的多标签与重订阅复查

复查先补回归，确认以下两项失败后修复：

- 原窗口级跳过计数可被其他标签的输出带走。A 丢失后，B 会显示 A 的跳过提示并
  清零计数，A 的后续小块还可能合入丢失点之前的旧批次。现在计数与合并屏障按标签
  隔离；A、B 分别发生丢失时，恢复提示只附在各自恢复后的数据前。
- 旧订阅关闭时只释放额度，保留待发送缓存；关闭检查也在入队锁外。现在关闭清空
  队列与计数，并在队列锁内检查关闭状态，避免生产者持有旧订阅时在关闭后重新写入。
  已经交给 IPC 的消息不作撤回，旧窗口/通道的确认不能释放新订阅的额度。

标签关闭、分屏关闭与创建回滚同步移除各窗口对应的缓存和跳过计数。SSH pump 可能
在标签关闭后继续排空已读数据，因此排空结束时再次检查标签是否仍存在并补做清理，
避免新引入的按标签计数保留已关闭标签。断开但仍保留的标签不执行该清理。

回归补充六个线程同时向六个标签各写 1,000 块 Unicode 输出，先占住四个已发额度，
生产完成后再归还确认，逐标签比对完整内容与顺序；另用持有旧订阅的生产者与窗口
重订阅并发，检查旧缓存释放、迟到写入、旧/外窗/重复确认隔离与新通道发送。

验证：上述两项回归在修复前失败、修复后通过。14 项输出链路回归以 4 个测试线程
重复运行 20 轮（280 次测试执行）全部通过；全量类型、lint、Prettier、Rust 格式、
120 项 renderer 测试、818 项 Rust/contract 测试与全目标/全特性 Clippy 均通过。
未做 Windows 实机长测，未发布版本；持续过载时仍保留显示跳过保护。

## 2026-10-19：窗口待发送小批次合并

确认一个与硬件型号无关的丢显示路径：窗口队列原先逐条保存输出，短小消息也独占
一项容量。连续入队的小块可在转发任务尚未获得调度时占满 8 项队列；这不要求
renderer 已有 4 批待确认，也不能单凭跳过标记断定窗口发生长时间卡顿。

本次在既有容量内合并待发送数据：

- 同一标签只合并到该标签最后一个待发送批次；单批上限仍为 16 KiB，待发送最多
  8 批，已发送待确认最多 4 批。保留各标签内部的输出顺序，不回填该标签的较早批次。
- 不跨丢显示点合并，跳过提示仍先于恢复后的新数据；真正过载时保留有限队列与原有告警。
- 通知唤醒转发任务，不新增定时器或凑批等待；输出 worker 不等待 renderer，窗口
  关闭、重订阅及其他窗口隔离沿用现有取消与确认机制。
- 保留工作区已有的 `tab` / `since_last_ack_ms` 诊断。最近确认时间只能说明确认进度，
  长时间无输出也会让它变旧，不能仅凭这个值认定 renderer 挂起。

初次回归覆盖六个标签交错的 6,000 个短小 ANSI/CJK 块、批次边界下的同标签顺序、
真饱和后的跳过标记位置，以及四个已发批次停住后输入 2,000 块 Unicode 输出的完整恢复。
这修补小批次占满消息项数的路径，不代表 Windows 长时间卡顿的现场根因已经确认；
本计划保留 active，仍需 Windows 实际负载和长时间回归。

初次验证：类型、lint、全范围 Prettier、Rust 格式、120 项 renderer 测试、815 项
Rust/contract 测试与全目标/全特性 Clippy 均通过；队列锁忙时诊断立即返回 `null`
的回归也通过。未运行 Windows 实机长测，未发布版本。

## 2026-10-19：beta.3 现场出现窗口显示跳过标记

用户反馈：`2.3.1-beta.3` 在 Win11 + 285H 上长时间运行后，终端出现
`[FileTerm: window display skipped 123 bytes while busy; press Ctrl+L to redraw]`；
同版本在 Win11 + HX370 上连续运行一天，未出现该标记，也未报告卡顿。

已确认：该标记只由窗口输出队列饱和产生（`services/workspace/terminal_output/mod.rs`
与 `bridge/events/terminal-output.ts`）。触发条件是窗口的 8 项待发送队列已满，
并不要求达到某个字节总量。每批最多 16 KiB，已发待确认最多 4 批，转发任务还可能
持有一批等待发送额度。它表示显示层丢弃了字节，不等于输入或整个应用卡顿；
会话日志与终端 transcript 在进入该队列前写入，
不受跳过影响。beta.3 的队列参数与当前一致，但该版本早于 #327 的滚动通知去重与
Windows 原生健康日志、#328 的符号表修正和 #330 的采集收紧。

现有信息还不能区分两种来源：

- renderer 停止消费：窗口隐藏、WebView2 挂起或主线程长时间阻塞；
- 输出突发超过“解析后确认”的吞吐，队列在正常使用中瞬间填满。

下一次现场需用同一份 `app.log` / `app.log.1` 对照：`terminal:output` 的
`window display queue saturated`（现新增 `tab` 与 `since_last_ack_ms`）、
`renderer-health` 的 `maxEventLoopGapMs`、`event-loop-gap` WARN，以及
`windows-native-health` 的 `appForeground`；同时记录发生前运行的程序或 TUI、
是否只有一个终端、窗口是否在前台。原因确认前不扩大队列容量或取消过载保护：若
`appForeground=false`，优先按 WebView2 后台节流处理；若窗口可见且 `since_last_ack_ms`
很短，再评估每通道接收额度或提高确认吞吐。

## 2026-10-19：beta.9 安装版 / portable 内存与节能模式反馈

排查分支 `fix/windows-terminal-memory`，从用户当前项目的 `37697fbd` 创建，未使用 worktree。
用户报告 Windows beta.9 四个“窗口”下，安装版约 120 MB、portable 约 57 MB；portable
使用一段时间后也升到约 80 MB 并明显延迟，约 100 MB 时无响应。开启 Windows 节能模式后，
安装版反复出现 `window display skipped … bytes while busy`。尚未明确“窗口”是系统窗口、
标签还是拆分终端，也未明确内存统计属于主进程、WebView2 还是任务管理器进程组。

### 已核对的代码事实

- 已核对发布 tag `v2.3.1-beta.9`（`0f33ade7`）。首次排查时，输出队列、终端生命周期、
  xterm 适配器、原生 transcript 与性能日志实现和分支起点 `37697fbd` 一致。
- 提示来自 bridge 对 `skippedBytesBefore` 的显示：某个原生窗口的输出队列已满。
  SSH / 本地 PTY 的窗口队列最多 8 批、4 批待解析确认，另有最多 1 批出队后等待额度；
  每批正文不超过 16 KiB。它可由消费变慢或输出突发触发，不等于证明泄漏，也不是 OOM 提示。
- 多个标签共享各自窗口的额度；每个订阅窗口都会收到输出，慢窗口独立跳过显示。
  Serial / Telnet 沿用不等待显示确认的路径，不能把 SSH / PTY 的上限套到这些协议。
- xterm 普通历史为 6,000 行，Rust transcript 上限 200,000 UTF-8 字节，renderer
  transcript 上限 200,000 UTF-16 单元；颜色、字宽与共享测量缓存已有容量限制。
  历史逐渐填满可以增加内存，但这些上限不能证明其他原生 / WebView 分配没有泄漏。
- Windows 安装版与 portable 使用相同终端实现与 release 配置；portable 额外编译
  `FILETERM_PORTABLE_BUILD=1`，配置根目录改为 exe 旁的 `config/`。目前没有发现
  只给安装版使用的终端缓存策略；配置、窗口数、运行时长和进程统计口径仍需控制变量。

### beta.9 现场取证

beta.9 默认记录 DEBUG 及以上，无需先开额外性能开关。让反馈者分别在安装版与 portable
的“设置 → 系统与日志”打开日志目录，复制 `app.log` 和存在时的 `app.log.1`，标明版本类型。
portable 对应 exe 旁的 `config/logs/`。每份日志仅保留当前文件与一个约 2 MiB 备份，
复现后应尽快复制，避免关键时段被后续日志轮转覆盖。

同时记录卡顿与节能模式开关的大致时间、四个窗口的实际含义、使用的协议，以及任务管理器
内存列和进程行的截图。先收取现有 app.log，不需要发送含终端正文的 `session-logs/`
或凭据配置文件。若能再次复现，用同一组会话 / 工作负载分别测试节能开关，并记录关闭
持续输出、关闭终端后的变化；不要为了对照删除用户配置或结束其他仍在工作的会话。

对照同一时段的既有事件（字段解释见 [本地诊断日志](../../quality/logging.md)）：

- `windows-native-health`：原生独立线程每分钟记录主进程 / WebView2 各进程的
  `workingSetBytes`、`privateBytes`、CPU 与 power throttling masks；页面完全卡住时
  仍可能留下记录。先确定增长发生在哪个 PID / 角色，再判断是否有持续增长趋势。
- `renderer-health`：可见页面每分钟记录 JS heap、回调 / DOM / 终端数量和 native
  队列 / 任务数量；与进程内存分开比较。修改日志级别、页面隐藏或队列饱和会影响完整性。
- `terminal-output-parse`、`terminal:output` 和 `event-loop-gap`：关联解析耗时、
  跳过显示及页面停顿。心跳停止、WARN 缺失或没有系统“未响应”弹窗都不能排除卡顿。
- `windows-thread-scheduling`：已有 WARN 触发的短时 ETW 汇总；权限不足会明确降级，
  不要求用户为本轮取日志提权，也不能用缺少 ETW 或节能 masks 直接认定大小核因果。

本轮不增加日志采样器、不修改缓存上限、不禁用系统节能，也不据聊天中的内存数字宣称修复。
Windows 原生节能模式和实际无响应现场仍待上述日志与真机验证，计划继续保持 active。

### 首轮验证（合入新队列前）

- 输出交付的 4 项 renderer 单元检查通过，包含十监听器 / 20,000 批交付和异常隔离。
- 原生窗口队列 5 项 Rust 检查通过，包含慢窗口隔离、UTF-8 分片、错误确认与重订阅回收。
- Chromium 真实 xterm 背压回归：4,000 批全部确认，待确认峰值 4，结束时待写字符与
  完成回调均为 0；十次输入 / 点击均完成，组合操作样本为 66–201ms。
- 复用 `terminal-workspace-soak.mjs`，在 macOS / Chrome 155 执行 180 秒真实 React
  十终端混合负载（renderer 平台分支为 Windows，SSH / 传输 / 监控事件为模拟）。
  共发送 / 确认 14,142 批，待确认峰值 4，传输历史保持 200，稳定负载时强制 GC 后
  JS heap 约 47.28–47.83 MiB，backing storage 约 116.18 MiB；后者单列，不能当作
  JS heap 或进程总内存。十轮实例替换后仅存活十个终端，全部卸载后实例 / 订阅 /
  marker / 待解析数据均为 0，heap 约 7.31 MiB、backing storage 约 1.75 MiB。
  短测没有复现持续增长或无响应，不能推导 Windows 节能模式下数小时运行没有泄漏。
- 本轮只修改排查文档，Prettier 与 `git diff --check` 通过；未改运行代码，未重跑全量门禁。

## 2026-10-17：诊断实现复查的五项修复

按复查结果收紧采集，优先避免影响正常使用与误报：

- 取消周期全局 ETW 跟踪，启动 helper 仅回收孤儿会话；仅既有卡顿 WARN 触发 3 秒采集，
  最短间隔 30 秒。原有每分钟原生健康日志继续运行，完全冻结时可能没有新的 WARN，
  不把后续窗口当作已经错过的卡顿现场。
- 会话名称/GUID 同时验证 FileTerm 标记、PID 和进程创建时间；进程退出或 PID 已复用时
  回收孤儿。每次采集前也检查；身份不明、查询不完整或清理失败时不开新会话。父进程超时
  清理也先查询并校验 GUID，按查询到的实例句柄停止，不能绕过所有权校验误停其他跟踪。
  枚举从 64 个槽位最多扩至 256 个，回收在采集的进程数量/拓扑校验之前进行。
- 迟到的同时间戳 Ready 不覆盖已知运行状态；其他运行/等待矛盾令证据失效。UI 与
  renderer-main 排在 compositor/IO 前，同优先级考虑未结束等待/运行；报告体积裁剪
  保持该顺序并预留预算字段空间。
- 消费尾段及汇总后复核 50ms CPU 预算，整个 helper 在报告序列化/裁剪后复核 200ms
  CPU 预算；超预算或 CPU 时间不可读则证据失效，停止后续自动采集。
- `processorBusyMs` 改为 `processorNonIdleScheduledMs`，明确只统计非 idle 线程驻留，
  `interruptTimeAccounted=false`、`availableCpuCapacityKnown=false`；不据此推断大核
  是否可用，也不输出“小核导致卡顿”的因果结论。

回归覆盖已复现的同时间戳乱序、关键线程裁剪、尾段预算及中断语义；Windows 无桌面 DLL
夹具补充启动仅回收、进程创建时间、存活所有者保护、强杀所有者后的会话回收，以及相同
名称但 GUID 不匹配时不清理（包括父进程兜底路径）。原生生命周期测试需要 Windows 和相应
权限；不能将 macOS 的可移植回归或 Windows 目标编译视为这些原生测试已经通过。

系统边界：整个进程树同时被强杀时，用户态代码无法立即清理 ETW；旧会话需在下次具备权限
启动时回收。没有增加系统服务、自动提权或常驻系统跟踪，不能承诺强杀瞬间零残留。

本轮验证：10 项可移植诊断回归、114 项 renderer、802 项 Rust/contract、全量类型/lint/格式、
Rustfmt 和严格 Clippy 通过；Windows 夹具全部目标（含原生清理测试）严格 Clippy 通过。
尚未在 Windows 执行强杀/孤儿回收、权限降级和真实开销测试，新增原生用例由 Windows CI
运行，不能将跨目标检查记作真机通过。所有改动留在当前工作区，未发布。

## 2026-10-17：自动线程调度诊断（首次实现记录，现行策略见上）

目标：用户发送现有 app.log 即可检查 FileTerm/WebView2 关键线程的实际核心效率等级驻留、
Ready 到运行的调度等待，并与页面卡顿时间对照。真实运行核心和等待必须来自内核
CSwitch/ReadyThread 事件，不能用 CPU 占用、理想处理器或节能策略代替。

实现范围：既有 Windows logging 模块、独立短时 ETW helper、二进制内部 helper 入口、
平台无关的事件汇总/回归和诊断文档。不改 IPC、终端内容、绑核、业务线程优先级或系统策略。
原生侧自动周期采集，并接受现有页面卡顿 WARN 触发；限时、限频、限制缓存/事件数/消费 CPU，
只把本应用进程树中线程的统计和各 CPU 忙碌总量写入轮转日志。helper 与主进程隔离，主进程退出后也负责停止
自己创建的短时会话；不接管 WPR 或其他程序的会话。

权限不足明确降级并停止重复尝试，不弹 UAC，不自动提升权限。日志说明线程识别、采集窗口、
未覆盖的新线程、事件丢失和拓扑不明等证据限制；只有完整切换区间可计运行时间，只有已知
Ready 起点可计调度等待。观察到在较低效率等级运行不等于已经证明卡顿因果。

首次实现的策略（已由上节替换）：启动约 10 秒后首次尝试，每次 3 秒，通常每分钟一次；WARN 可触发下一窗口，
起点至少相隔 30 秒。请求 1 MiB ETW 缓冲、最多消费 250,000 个事件，消费线程 CPU 超过
50ms 时提前结束；失败、丢失、未知载荷或超预算停止本次运行中的重复采集。普通权限不足
明确记录 requiresElevation，保留进程健康日志，不弹 UAC。线程栈换入不算 Ready 调度等待，
窗口末端仍在运行或等待的区间单列下界，未观察到的处理器不视为空闲。

验证：7 项合成回归覆盖载荷校验、运行/等待/抢占/跨核、换入、事件不连续、容量和隐私边界；
114 项 renderer、799 项 Rust/contract、全量类型/lint/格式与 Clippy 通过。
新增无桌面 DLL 的 Windows 夹具复用生产模块，Windows 目标全 targets 编译及严格 Clippy
通过；本机夹具的 7 项可移植回归通过。Windows CI 增加该夹具执行步骤，覆盖 headless helper
拒绝无效参数、真实采集或明确权限不足、报告上限和正常退出；本机没有运行 Windows 原生测试。
完整应用的 Windows 交叉编译被本机缺少 Windows C 头文件阻挡（ring 的 assert.h），
不能将夹具的跨目标编译当作整个应用或实际采集通过。数小时真实 CC、权限行为、清理与整机开销
仍需 Windows 真机验证，计划继续保持 active。字段和解读见
[自动线程调度采集](../../quality/logging.md#自动线程调度采集)。

## 2026-10-17：跳过滚动状态未变化的解析通知

复查已合入的 PR #327：同一解析任务内的通知已经合并，但不同解析任务仍各自通知
滚动条。fullscreen 重绘或普通历史达到上限后，即使行数、历史长度和 viewportY 未变，
仍重复重设隐藏计时器和申请滚动条更新帧。

滚动控制器现在在合并通知时对照这三个数值，仅状态变化才通知，不读取 DOM，也不跳过
终端解析或输出确认。元素高度由滚动条既有的 ResizeObserver 处理；历史增长、手动滚动、
终端行数变化及从备用屏幕恢复历史仍更新。未修改历史上限、绑核、优先级或电源策略。

新增单元回归在旧实现上因 100 次状态不变的解析产生 100 次通知而失败；补丁后为 0。
真实 Chromium/xterm 回归中，100 次 fullscreen 写入和 100 次达到历史上限后的普通写入
均为 0 次滚动条通知，退出 fullscreen 恢复历史为 1 次。原有批量输出仍为 0 次布局读取、
12 次缓冲行读取和 1 次通知，图形滚动/缩小窗口清理断言继续通过。

全量类型、lint、格式、114 项 renderer 测试、Rust/contract 测试、Clippy 和 CSS contract 通过（保留既有
11 处 CSS 提示）。这只证明减少了一条无效工作路径；本机为 macOS，尚未验证 Windows
原生 WebView2、实际 CC 长上下文和数小时运行，不能据此认定大小核或长期卡顿根因。

## 2026-10-16：终端滚动条通知去重

继续检查发现滚动条同时订阅逐行滚屏、解析完成和 `onRender`；同一批输出会多次重设
隐藏计时器、取消并重新申请动画帧，光标/选区重绘也会触发无关的滚动条更新。
将原有滚动控制器移到同目录 `terminal-scroll-controller.ts`，保留像素与终端行的映射，
滚动、resize、解析完成和缓冲切换只在同一微任务通知一次，不再订阅普通绘制。
退订或终端实例替换后取消迟到通知；不暂停后台输出解析或原生额度确认。

新增单元回归中，同一任务的 1,000 次滚屏与一次解析完成通知由 1,001 次降到 1 次，
修改前不能通过断言。真实 xterm 的 200 行滚屏用例只有一次滚动条通知，单独重绘无通知；
保留历史滚动、resize 图形清理和前景适配的回归。混合工作区负载夹具原有 Windows
绝对路径被嵌入 TS 字符串后丢失反斜杠，现统一为正斜杠，使该夹具可以在 Windows 运行。

类型、100 项 renderer 测试、改动文件 lint/格式与 CSS contract 通过（保留原有 11 处 CSS
提示）。Windows 主机 Chromium 的 60 秒真实 React 十终端混合负载完成 4,526 批，全部
确认，待确认峰值为 4。十轮终端替换后只保留十个存活实例；卸载后实例、原生订阅、marker
和未解析输出均为 0。该测试包含模拟传输/监控事件，不包含原生 WebView2、SSH 或 ConPTY，
不能代替数小时现场验证。本轮未改 Rust，沿用上一轮 Rust/contract 结果，不重复运行。

## 2026-10-16：Windows 原生专项日志

增加 `app:windows-performance` / `windows-native-health` INFO 记录，启动后立即采样，
随后每分钟一次。原生独立线程采样 Windows build、CPU 型号/效率等级、电源状态，以及
本进程与所属 WebView2 进程树的 CPU 时间增量、工作集、private bytes、优先级和节能策略。
静态硬件信息在每条记录中复用，避免日志轮转后丢失对照条件。进程与句柄状态有界，不调用
PowerShell/WMI、不记录命令行或终端内容，不依赖页面恢复后才能记录。

与现有 renderer 心跳和输出背压日志按时间对照；这组日志不能证明线程实际运行在小核，
不据此自动禁用节能或绑核。字段、错误值和解释限制见
[Windows 系统与大小核对照](../../quality/logging.md#windows-系统与大小核对照)。

Windows 本机的 4 项新增原生采样测试和完整 Rust/contract 回归通过，改动 Rust 文件的
rustfmt 与文档格式检查通过；Clippy 仍被前述未修改文件的既有问题阻挡，没有新增诊断。
尚未完成正式应用内的每分钟持续采样和真实卡顿现场验证；需要包含本次代码的新构建。

## 2026-10-16：合并输出解析期间的滚屏适配

用户确认本轮反馈来自 `2.3.1-beta.1`：有时约六小时后终端输入延迟，有时重启后不到
半小时整个应用卡顿。Win11/Intel 与 Win10/AMD 的实际负载不同，尚不能归因于系统或大小核。

代码检查和真实 xterm 浏览器回归确认：输出解析每滚过一行，就同步执行图形适配器的
整屏布局读取和前景色适配器的可见行扫描。此时 DOM 仍可能对应上一帧。修补范围：

- 图形适配器只在 xterm 的 `onRender` 通知中处理实际绘制范围；滚动和 resize 由 xterm
  已有的渲染调度合并，不再在解析中反复测量旧 DOM。
- 滚动、resize 和缓冲区切换的前景适配合并到一个微任务；`onWriteParsed` 仍立即适配，
  并取消同批次的重复扫描。销毁取消待执行扫描，不依赖隐藏窗口可能暂停的动画帧。
- 不改变协议输出、滚动历史上限、字体校准、符号几何或颜色规则。

新增 `terminal-scroll-output.mjs` 使用 Chromium 与真实 xterm，预填图形后一次写入 200 行。
解析期间布局读取从 2,400 次降到 0 次，缓冲行读取从 4,612 次降到 12 次；原代码不能通过
新增的开销上限断言，修改后通过。输出、滚动历史与缩小窗口后的图形画布仍存在且数量有界。
单元回归覆盖批次解析取消重复扫描、事件合并与销毁清理。

本次类型检查、97 项 renderer 测试、Rust/contract 测试与 CSS contract 通过。Chromium 的
普通输出、box drawing 字体/DPI 矩阵、真实选区和 60 秒混合重绘回归通过；持续负载的
1,190 次写入全部解析，空闲后无继续刷新。改动文件的 lint/格式检查通过；全仓 lint/格式
被现有 CRLF 行尾问题阻挡，全目标 Clippy 被未修改 Rust 文件中的未使用 import、
`byte_char_slices` 和 `needless_return` 问题阻挡。没有改动这些无关文件。

以上证明消除了一条同步开销放大路径，不证明它是现场长期卡顿的唯一根因；Windows 原生
数小时真实 CC 会话仍待验证，计划保持 active。

## 当前继续检查（2026-10-15）

- 追踪 6–10 个终端的输出/输入、字体缓存、事件订阅、传输进度、会话销毁与后台工作。
- 已发现传输历史仅在 journal 序列化时裁剪；运行内存和 renderer 列表持续增长，
  完成目录任务还保留全部 manifest。修补运行中保留策略，并保留所有可恢复/待清理任务。
- 增加真实 React 终端与工作区订阅的混合负载，采样堆、监听器、未解析任务及卸载回收；
  浏览器合成结果与 Windows 原生十二小时现场分开记录。

## 当前范围

用户确认本地与 SSH 会话都有 CC 延迟；尚不确定无响应时是单终端还是整个应用。
本次 2.2.29 按用户要求包含监控与输出调度改进及诊断日志；这不代表 CC 延迟和长期无响应的根因已确定或确认解决。

## 已确认与修改

- 对比 2.2.17，当前 DOM 终端增加了符号、块字符、选区和前景适配。
  Chromium 的真实 xterm 密集 ANSI/CJK/符号重绘对照中，未安装适配器时主线程
  10ms 定时器的 P95 间隔约 17–18ms，全部适配器约 43ms。
  这是合成负载的事件循环延迟，不是 CC 按键到显示的端到端测量。
- 符号适配按批读取网格和样式，再统一修改 DOM；观察器忽略自己刚完成的修改记录。
  块/横线适配先批量读取行几何，再修改占位文本，复用逐行 canvas 并清除旧位图。
  不移除符号防遮挡、粗体继承、中文/组合字符列宽、选区、ANSI/反色或主题适配。
- 同一短对照下全部适配器的 P95 间隔约 36ms；仍高于原生 xterm，不能据此宣称
  用户报告的 CC 延迟已完全恢复。
- 新增 `renderer:performance` WARN：可见页面的 1 秒计时器相隔至少 4 秒时记录
  `event-loop-gap`，最多每 30 秒一次。隐藏/显示切换重置计时，不记录终端文本。
  只在主线程恢复后才能记录；系统睡眠也可能造成间隔，不能单凭该日志断言忙循环。

## 回归与负载证据

- `terminal-redraw-load.mjs` 使用真实 xterm 和四个适配器，连续 60 秒重绘 46 行，
  切换普通/备用缓冲区；断言全部输出排空、空闲后停止刷新、画布/DOM 有界及布局读取有界。
  Chromium、WebKit 通过；未复现永久无响应。
- 另一次 Chromium 90 秒负载排空 2581 次写入，停止后无持续刷新，结束时 10 个
  canvas、3046 个 DOM 节点。该负载不能替代数小时真实会话和原生 WebView 验证。
- 横线像素回归额外验证 canvas 在选区/主题刷新后复用，缩短一行后旧尾部像素清空。
  Chromium/WebKit 的横线、特殊符号及真实鼠标选区回归保留。
- Chromium/WebKit 的真实 React IPC fixture 人为阻塞主线程 4.2 秒，不抛出异常；
  恢复后实际诊断模块产生 WARN，页面没有 JS error。证明诊断不依赖 ERROR。
- 类型、lint、全范围格式、47 项 renderer 测试和 CSS contract 通过；CSS 只有原有债务。
- 从当前目录构建 macOS debug `FileTerm Dev.app`，身份为 `com.fileterm.desktop.dev`。
  原生运行真实 CC，验证本地文本输入、Unicode 粘贴、帮助菜单重绘、横线和块字符。
  10 秒 WebView 采样中主线程 805 个样本里 778 个处于 Mach 消息等待；短测未复现忙循环。
  另经实际本地 PTY 运行 1000 帧 ANSI/CJK/符号重绘，35.04 秒后显示完成标记并返回 Shell，
  完成后正常切页；没有捕获永久冻结。
  原生短测不能替代端到端延迟比较或长期运行。
- 原生连接截图对应的群晖会话，工具不再发送键鼠操作期间，侧栏实际提交摘要从
  revision=2 进到 revision=39（约 60 秒），样本年龄分别为 48/13ms；随后无输入观察
  也看到 CPU 数值变化。没有在该次短测复现侧栏停止提交。

## 未完成

- 原生 FileTerm 的真实 CC 输入延迟对照测量与数小时运行；Windows 原生回归。
- 无响应现场的主进程/WebView CPU、线程采样与内存增长证据；现有日志没有调用栈，
  无法认定唯一根因。下一次现场需同时确认冻结范围，不能把缺少 ERROR 当作没有阻塞。

## 2026-10-15：Windows 11 长时间交互延迟

用户反馈 2.2.31 beta1 运行约 1 小时后有明显输入/点击延迟，约 12 小时后可达 10 秒。
通常有 4 个 SSH 标签及若干拆分终端，总计约 6–10 个终端，包含持续重绘的交互程序、
空闲 SSH 和文件传输。现场日志主要是 SSH 发包 DEBUG；没有 ERROR 不能排除排队或主线程阻塞。

代码检查确认终端原生推送不等待 renderer 消费，renderer 未解析输出的字符串也没有上限。
这是可累积的路径，尚不能据此确认它是现场的唯一根因。本次修补：

- SSH/本地 PTY 原生输出按 UTF-8 边界拆成至多 16 KiB 的批次，每个窗口最多 4 批待确认。
  renderer 等待匹配终端的 xterm 解析完成后确认；单个监听器异常也会记录诊断并释放额度。
- 确认校验窗口、通道与批次身份，重复/过期/其他窗口的确认不增加发送额度。
  窗口关闭或重新订阅时释放旧通道的等待者。每窗口有独立的 8 项待发送队列与转发任务；
  一个慢窗口只消耗自身的额度，其他窗口、SSH 收包和文件通道不等待其解析。队列饱和会
  限频记录窗口告警，并在后续显示中提示跳过的字节数。
- SSH pump 保留现有的 128 项有界队列，并持续读取共享连接。SSH 的多个通道共用处理循环，
  暂停 shell 读取可能让库内部的通道发送等待阻塞输入和文件通道，因此沿用过载时跳过
  显示批次的策略；加入 30 秒限频告警和后续显示中的跳过字节提示。它不保证极端过载下
  终端显示或会话日志的完整性，TUI 可用 Ctrl+L 重绘。输入、命令和取消仍走原来的独立路径。
  空闲且无输出批次时不再每 16ms 唤醒。
- 原生额度等待不持有 workspace sessions 的锁。现有本地 PTY reader 的队列饱和提示策略保留。
- `renderer:performance` 每分钟记录 `renderer-health`：运行时长、可用时的 JS heap 和
  IPC callback 数量。JS heap 不是整个 WebView 的进程内存。
- 串口/Telnet 保留原有推送方式，避免其共用的收发 worker 被显示确认阻塞。
- `terminal-output-parse` WARN 记录慢解析、待解析批数/字符数与监听器数量，30 秒限频；
  原生 `terminal:output` 记录明显的额度等待或窗口队列饱和。以上记录均不包含终端正文。

自动回归涵盖 10 个监听器的 2 万批消息、解析完成确认、监听器异常、慢消费者/UTF-8 分片、慢窗口与健康窗口隔离、
跨窗口错误确认、重订阅释放等待及满 SSH 显示队列的跳过计数、限频和恢复提示。这些合成检查不能替代 Windows 原生长测。

Chromium 的真实 xterm 短测同时运行 10 个终端，启用现有符号、块字符、选区、前景和日志颜色
适配器，并执行 4,000 批 ANSI/CJK 重绘。待确认峰值为 4 批，结束后未解析字符和完成回调均为 0；
重绘期间输入框与按钮 10 次操作均成功，耗时样本为 120–622ms。该数字包含自动化操作开销，
不是 Windows WebView 的端到端延迟，也不代表 12 小时验证通过。

### Windows 长测与取证

- 使用本次修补的构建，保持实际 6–10 个终端和重绘/传输负载，至少运行 12 小时。
- 对比起始、1 小时、12 小时的文字输入、点击、终端 Ctrl+C 响应，并记录窗口数量与任务管理器
  中应用/WebView 进程的 CPU 和内存。不要把 JS heap 当成进程总内存。
- 保存同一现场的 `app.log` 和 `app.log.1`；日志会轮转，应在卡顿发生后及时取走。
  查找 `renderer-health`、`terminal-output-parse`、`renderer backpressure`、`window display queue saturated`、`event-loop-gap`。
- 若交互仍延迟且输出待确认保持有界，继续采集 WebView CPU/线程采样及原生内存增长，
  本计划保持 active，等待 Windows 长测确认。

## 2026-10-15：渲染侧累积开销与缓存约束

进一步检查确认了以下可复现路径；它们尚不能证明 Windows 现场的唯一根因：

- 搜索文字变化会重新设置主题及日志颜色，之前即使颜色没变也会重扫完整历史。
  现在比较实际主题值与归一化的颜色值，相同设置直接返回。日志着色与符号列映射复用
  xterm 的可复用 cell，减少逐列对象分配；真正切换颜色仍重新着色，卸载恢复原前景。
- 文件面板吸附观察器之前订阅整个 shell 子树，每次终端 span/canvas 变动都会查询侧栏
  吸附目标并安排布局。现在只观察 shell 本身及侧栏子树；侧栏/card 被替换、折叠和展开时
  重新绑定 ResizeObserver，销毁时解除绑定。
- DOM 渲染器的颜色对比、组合字符宽度缓存之前没有容量约束。现在每组颜色缓存最多
  8,192 个颜色对、字符宽度最多 16,384 项，达到容量仅清理可重算的结果。保留最低对比度、
  字体校准、ASCII 快表、终端缓冲与协议正文。最初的较小宽度容量在组合字符对照中导致
  反复测量，已调大并重新对照。该适配器依赖已固定的 xterm 6 私有结构，升级时需重跑回归。
- 每分钟 `renderer-health` 增加终端数、DOM 节点数和该分钟最大心跳间隔；支持 Long Tasks
  API 的 WebView 还记录长任务数量、总时长及最长时长。隐藏窗口的任务不计入可见窗口统计，
  不支持的 API 省略对应字段。记录仍不包含终端文本。

### 对照方法与证据

`apps/tauri/tests/browser/terminal-long-run.mjs` 使用真实 xterm 与生产适配器，创建 10 个终端，
每个先写入 6,500 行，填满配置的 6,000 行历史，再持续重绘高变化 RGB 与组合字符。
测试等待全部写入解析完成，不通过丢弃输出获得测量结果；结束后检查侧栏替换与 20 次终端
创建/销毁，确认空闲无重复布局及终端 DOM 释放。

同一主机的顺序 30 秒对照（不是 Windows 原生延迟测量）：

| 检查项                            | 对照提交 `13569a61` | 本次修改                |
| --------------------------------- | ------------------- | ----------------------- |
| 10 轮相同日志颜色设置扫描历史行数 | 601,000             | 0                       |
| 上述设置耗时                      | 约 1,752ms          | 约 0.3ms                |
| 终端重绘触发的文件面板观察回调    | 1,837               | 0                       |
| 单终端颜色缓存观测峰值            | 128,162             | 7,728，容量上限 8,192   |
| 组合字符宽度观测峰值              | 12,547              | 12,547，容量上限 16,384 |

该负载刻意覆盖高变化内容；宽度缓存的淘汰另有超容量单元回归。测试自身的采样也会消耗
主线程时间，不将定时器间隔或吞吐差异当成真实按键延迟提升比例。

Chromium 的 5 分钟持续负载完成每终端 2,224 次写入（总计 22,240 次），颜色缓存观测峰值
8,174、宽度缓存 12,547、DOM 峰值 8,280；结束后创建/销毁 20 个终端，DOM 回到 30 个节点，
空闲无持续布局。20 次输入/点击均成功，耗时样本为 27–720ms，包含自动化和取样开销。
这证明本次负载下缓存和 DOM 有界，不能推导整个进程内存无泄漏或 Windows 的 12 小时结果。

WebKit 的 30 秒同类检查也通过容量与清理断言，但初始高变化组合字符负载出现约 1.2 秒
主线程计时器间隔，前几次输入/点击耗时约 1.1–7.4 秒。该负载直接驱动十个 renderer，
未模拟原生共享的四批输出额度，且不含原生传输；保留这一渲染压力限制，不据此宣称
所有重绘负载都已流畅。原生额度链路另由 4,000 批真实 xterm fixture 验证。

运行方式：配置本机可用的 `PLAYWRIGHT_MODULE_PATH` 与可选的
`PLAYWRIGHT_EXECUTABLE_PATH` 后执行：

```bash
node apps/tauri/tests/browser/terminal-long-run.mjs
FILETERM_SOAK_MS=3600000 node apps/tauri/tests/browser/terminal-long-run.mjs
FILETERM_PERF_BASELINE=1 FILETERM_PERF_BASELINE_REF=13569a61 node apps/tauri/tests/browser/terminal-long-run.mjs
```

默认运行 60 秒，`FILETERM_SOAK_MS` 延长持续输出阶段；每轮穿插 20 次自动输入和点击，
输出操作耗时包含自动化开销。浏览器负载不包含原生 SSH、SFTP 或 WebView2 进程；
本计划仍等待 Windows 实际 6–10 终端至少 12 小时回归及现场 CPU/进程内存证据。

## 2026-10-15：分屏间重复字体测量

对十个终端的高变化组合字符重绘进行 CPU 采样，主要热点落在 DOM 字符宽度测量。
每个终端原本各自测量相同字体下的同一字符组合，字体缓存有容量上限仍会重复支付
冷测量的同步布局成本。布局隔离的对照在两个浏览器引擎均无收益，因此未采用该方案。

本次在已有缓存适配边界加入文档内共享测量：

- 只复用实际测得的正数有限宽度，保留原生字体测量与 ASCII 快表。按实际计算样式区分
  字体、字号、粗斜体、字体特性、语言、字距、书写方式与 DPI 等环境，跨文档不共享。
- 共享缓存总计最多 32,768 个测量值；不缓存超过 64 个 UTF-16 单元的字符组合。
  每个终端原来的字符宽度与颜色缓存容量约束继续生效。
- 字体加载完成/失败及校准清理时失效。共享状态只持有样式字符串与数值，不保留终端 DOM；
  最后一个终端销毁时清空状态并移除字体事件监听。无法识别的 renderer 结构保持原生行为。
- 该路径仍依赖 xterm 6 DOM renderer 的私有接口，升级需要重跑测量与像素回归。

`apps/tauri/tests/browser/terminal-shared-widths.mjs` 在同一引擎顺序比较十个终端、300 次
相同重绘写入，加载仓库离线字体与生产适配器。对照仅绕过共享层，两侧均保留原有容量限制：

| 检查项                                     | 独立测量     | 共享测量     |
| ------------------------------------------ | ------------ | ------------ |
| 实际 DOM 字体测量次数                      | 125,450      | 12,545       |
| Chromium 首次对照耗时                      | 约 10.05 秒  | 约 3.90 秒   |
| WebKit 首次对照耗时                        | 约 25.67 秒  | 约 11.19 秒  |
| 三组字体/字号、四种粗斜体的 120 个宽度样本 | 与共享侧一致 | 与独立侧一致 |

开启 CPU 采样且同时运行其他门禁的再次 Chromium 对照约为 11.99 / 5.09 秒，测量次数仍为
125,450 / 12,545。耗时受主机负载影响，不能当成真实按键延迟的固定提升比例。

单元回归覆盖跨字体/字号/粗斜体/文档隔离、字体事件与显式失效、容量上限、异常测量重试、
超长字符不缓存、方法恢复、最后一个消费者释放和未知缓存结构。持续重绘和图形/选区回归
继续覆盖该共享路径；Windows 原生 12 小时验证仍待完成。

当前代码的像素复测中，Chromium 对照约为 9.82 / 3.47 秒，WebKit 约为 27.01 / 12.51 秒；
两侧最终渲染的 PNG、120 个宽度样本与缓冲文本均一致。共享测量阶段的 5 分钟十终端负载
完成 23,030 次写入，缓存及 DOM 有界；后续图形筛选修改后追加的 60 秒负载完成 4,440 次
写入，20 次输入/点击均成功，耗时样本 314–861ms。以上仍是浏览器合成检查。

### 跳过不含目标字符的图形工作

进一步移除普通文本路径中的无关布局读取：符号适配器先判断目标行是否含需要处理的
非 ASCII 符号/标点，块字符适配器只处理含图形、旧画布或待清理占位的行，再批量读取几何。
块字符映射也复用 cell；行数缩小时移除越界画布，保留主题、反色、鼠标选区和像素校准。

`terminal-plain-output.mjs` 的 Chromium/WebKit 对照中，40 次交替英文、中文与组合字符
写入，原生与全部适配器均为 0 次 `getBoundingClientRect()` 调用；这不包含原生字体测量。
绘制八行图形后，行数缩小使画布从 8 降到 3，
再写普通文本后画布为 0。60 秒混合图形负载的布局读取从此前每次 render 约 49 次降到
约 13 次，结束后无持续刷新，画布与 DOM 保持有界；该变化不等于真实按键延迟按比例降低。

### 符号比例回退修正

用户补充防遮挡后 `⚠️`、`•` 变瘦。已将共享符号适配从 advance 与单轴缩放改为实际
笔画边界、必要平移和等比缩小，保留现有按行筛选、度量缓存与批量 DOM 更新。
回归增加彩色笔画、自然比例和字体留白场景，检查列位置与复制内容；细节见
[特殊符号比例](../../quality/terminal-regression-checklist.md#特殊符号比例2026-10-15)。
Chromium/WebKit 比例矩阵及防遮挡检查通过，Windows 原生字体回退仍待复核。

## 2026-10-15：运行中传输历史与回收检查

### 确认并修补的累积

- 原先只在写 journal 时选取最近 200 项，Rust 运行中的 `transfers` 和 renderer
  列表没有同步裁剪；完成目录任务仍持有全部 manifest。每次新任务持久化还会先克隆全部
  内存历史。现在运行内存、前端与 journal 使用同一保留规则，持久化前先原地裁剪，
  完成且无需恢复/清理的目录任务释放文件和目录数组的容量。
- 保留所有未结束、可恢复或待清理的任务；必要任务超过 200 时允许超限。不删除实际文件、
  断点和未完成 manifest。Rust 回归连续完成 5,000 个任务，并验证恢复数据与清理数据保留。
- 首次工作区快照等待期间，传输进度原先逐条进入数组；现在同一任务仅保留最新状态，
  已结束历史也遵守同一上限。50,000 次八任务进度更新只保留八条待应用状态。
- 失败通知去重表随历史清理，避免已被移除的失败任务 ID 长期驻留。

### 混合负载与销毁

`tests/browser/terminal-workspace-soak.mjs` 使用真实 React `TerminalView`、工作区同步 hook、
字体与传输 UI，采用 Windows renderer 平台分支。十个终端先填满 6,000 行滚动历史，
随后四个交互界面重绘、两个持续输出、四个空闲；同时推送监控与八个传输进度，
加速生成已结束任务。传输和监控是模拟事件，没有实际 SSH/SFTP IO 或原生 ConPTY。

| 检查项                           | Chromium 十分钟        | WebKit 三分钟                    |
| -------------------------------- | ---------------------- | -------------------------------- |
| 输出发送/确认                    | 46,050 / 46,050 批     | 13,466 / 13,466 批               |
| 最大待确认批数                   | 4                      | 4                                |
| 前端传输历史                     | 200 项                 | 200 项                           |
| 终端输出订阅                     | 始终 10 个，卸载后 0   | 始终 10 个，卸载后 0             |
| 强制 GC 后 JS 堆                 | 约 46.0–49.1 MiB       | 无该数据                         |
| 缓冲区存储                       | 约 116.169–116.171 MiB | 无该数据                         |
| 十轮替换后的存活实例             | 10 个                  | 未强制 GC，不据 WeakRef 数量判断 |
| 全部卸载后的实例/解析队列/marker | 0 / 0 / 0              | 队列与 marker 为 0               |

Chromium 每 30 秒采样；填满滚动历史后的堆与缓冲区存储未呈现随任务数持续增长的趋势。
一共替换 100 个终端后，DOM 事件监听器回到首个负载样本相同的 885 个，全部卸载后为
172 个（包含测试页本身的事件委托）。全部卸载后的 JS 堆约 7.0 MiB、缓冲区存储约
1.7 MiB。WebKit 未强制 GC，只确认订阅与 DOM 清理，不据此声称已证明堆回收。
旧同步 hook 的五秒对照积累到 508 个传输任务，违反历史上限断言；修补后维持 200 项。

运行方式与其他浏览器回归一致，通过 `PLAYWRIGHT_MODULE_PATH` 和
`PLAYWRIGHT_EXECUTABLE_PATH` 选择本机工具；`FILETERM_SOAK_MS` 设置时长。
`FILETERM_PERF_BASELINE=1` 从 `FILETERM_PERF_BASELINE_REF`（默认 HEAD）载入指定版本的
同步 hook；复现本次旧代码时指定 `FILETERM_PERF_BASELINE_REF=639117e4`。
对照仍保留上限断言，该旧版本预期失败。

### 原生任务与队列诊断

不新增周期任务或 IPC 请求：Rust 在既有每分钟 `renderer-health` 记录中添加 `native`：

- `tokioAliveTasks` / `tokioGlobalQueueDepth`：原生活跃任务与全局就绪队列深度。
- `sessions` / `workers` / `workerQueuedCommands` / `sessionLogWriters`：会话、worker、
  命令队列占用和会话日志 writer 数量。
- `transferTasks` / `transferRuns` / `transferProgressSamples`：传输历史、正在运行的任务
  与进度采样状态数量。
- `terminalOutput.windows` / `inFlight` / `queuedBatches`：订阅窗口、待确认输出和待发送批次。

采样只使用非等待锁；繁忙时记 `null`，不假报为零或等待业务。只写数量，不写会话内容。
全局就绪队列不包含所有 IO 等待任务，瞬时数值也不等同于泄漏；需要结合相同负载下的
连续样本、任务结束后的回落、JS heap 与回调数判断。Rust 回归覆盖繁忙锁、慢窗口和额度统计。
各诊断事件的触发条件、字段与解释限制见 [本地诊断日志](../../quality/logging.md#长时间运行诊断事件)。

Windows 11 原生十二小时、真实交互程序与实际 SFTP 并发仍待复现；本次没有确定现场
十秒延迟的唯一根因，也未发布版本。

本次全范围类型、lint、格式、92 项 renderer、775 项 Rust/contract 与 Clippy 通过。
CSS contract 通过，保留既有 11 处重要性声明提示。

## 2026-10-22：Refs #353，监控历史重复分配

反馈来自 macOS 27.0.1 / Apple Silicon / v2.2.31：2 个 SSH、4 个端口映射、15 秒资源监控，WebContent 长期约一个核心及 800MB。提供的 JSC 数组分配/GC 栈不能单独定位组件、证明泄漏，或等同本轮 Windows WebView2 问题。beta12 已含图表动画去 React state、workspace reducer 去重放；当前本地字体补丁尚未发布。

### 本轮复现及最小修补

1. 实际 NetworkPanel 在网络历史引用不变时，100 次仅 CPU 等无关指标变化仍执行 200 次 buildLinePath，重复分配两条曲线的点数组和字符串。生产 React 浏览器夹具先保留失败，再将曲线几何按 samples memo；空历史使用稳定空数组，避免每次创建填充窗口。保留组件正常重渲染，以便流量标签、语言、网卡和其他指标更新，不以整个组件 memo 阻断更新。
2. 全量 workspace 快照携带相同 generation/revision 时，原合并逻辑仍把反序列化后的历史对象替换到前端。现在仅对双方都已有数据的同版本历史复用引用；监控状态和其他 session 字段仍使用新快照，新版本/新 generation、首次加载及显式清空保留原行为。未改变 Rust 采样频率、历史容量、SSH/隧道配置或事件投递。
3. 新回归确认旧代码在相同历史下重建 200 次曲线，修补后非空/空历史均为 0 次；actual hook 的 100 次全量快照更新保留历史引用，目录字段仍更新。已有 360 次监控/输入、恢复断点、网卡选择、200 批解析/ACK 和卸载清理继续验证。

### 验证与边界

- 当前 Windows Chromium 的 DOM/WebGL 及 Windows Playwright WebKit 27.2 的 DOM 图表回归通过。WebKit 引擎测试不是 macOS Tauri/WKWebView 原生验收。
- monitoring-terminal-stream 的旧诊断断言依赖最后一条告警，可能被 LoAF 告警覆盖；大量输出还会先触发 event-loop-gap，导致随后主动阻塞受生产 30 秒限流。测试改为等待告警冷却并匹配新 event-loop-gap，保留诊断内容和 200 批解析断言，没有改生产阈值或限流。
- 两项源码改动只减少已复现的重复计算；不能由曲线调用次数推出 CPU 降幅或内存释放量，未证明 #353 的数小时持续高 CPU 已完整修复。
- 基线失败及修补后的输出归档在本地 `D:\CodeFiles\renderer-validation-archive\issue353-2026-10-22\`。未使用用户真实 SSH/隧道或更改持久化设置；现有字体候选原样保留。监控补丁随后按用户要求追加至 PR #352（`464b7456`）；PR 仍为 draft，未合并、发版或关闭 Issue。
- 待 macOS 同一原生版本、同 PID/启动时间下复测 2 SSH + 4 端口映射 + 15 秒监控：分别记录输出停止、监控关闭、侧栏收起、会话关闭后的 CPU/heap/native 内存与采样栈，继续区分 terminal/monitoring/IPC/布局及 GC 开销。

本轮门禁：完整 typecheck、198 项 renderer、806 项 Rust（1 项 ignored）、10 CLI、23 contract、3 scheduler、严格全目标/全特性 Clippy、改动文件 ESLint 通过。CSS contract 通过，保留既有 11 处 !important 与约 6 处旧颜色值提示。全量 lint 的 76,074 项均为 CRLF 格式冲突，全量 Prettier 标记 477 文件；未为此重排无关文件。workspace 重放回归仍为 300 个事件/300 次 reducer。

Chromium 与 Windows WebKit 的 monitoring-terminal-stream 均通过：100 次相同监控版本的全量快照保留历史引用，后台模拟无 RAF 时继续处理监控和排空输出，主动阻塞后的 event-loop-gap 告警内容与资源计数断言通过。

## 2026-10-22：WebGL addon 0.19 光标闪烁定时器保活终端

FileTerm 当前锁定 `@xterm/xterm@6.0.0` 和 `@xterm/addon-webgl@0.19.0`，运行时启用 `cursorBlink`，scrollback 上限为 6000 行。addon 0.19 创建 WebGL renderer 的 `CursorBlinkStateManager`，但没有把 manager 注册到 renderer 的 dispose store；聚焦后运行的 blink timer 会继续引用 renderer 和整棵 terminal/scrollback 对象，即使 tab 已关闭。xterm 上游 [PR #5817](https://github.com/xtermjs/xterm.js/pull/5817) 已修复此遗漏，但 npm 当前 `latest` 仍是 addon 0.19.0；Tabby 的 [问题复现与内存测量](https://github.com/Eugeny/tabby/issues/11718) 也使用了显式 dispose manager 的临时兼容措施。electerm 的默认 `cursorBlink` 为 false，并且在终端卸载时 dispose WebGL addon；这避开了常见的聚焦闪烁触发条件。

FileTerm 的 `releaseWebglAddon` 现在先 dispose `_cursorBlinkStateManager`，再 dispose addon 和丢弃 WebGL context，异常时仍继续完成后两步。浏览器生命周期回归在聚焦并确认 blink interval 已启动后拦截 manager.dispose：修改前因 manager 未销毁而失败，修改后 manager.dispose 被调用、context 丢失且 host 节点清空。结果在本地 `D:\CodeFiles\renderer-validation-archive\issue353-2026-10-22\webgl-dispose-before.log` 与 `webgl-dispose-after.json`。

Tabby 的同一问题还记录了另一条独立的活跃会话卡顿路径：其自定义 `OSCProcessor` 对未结束的 `ESC ]` 序列无限缓存，并在每个输出 chunk 上重复拼接和扫描；45 秒测试中 renderer RSS 从 615 MB 到 1,284 MB，主线程往返延迟从 0 增至 2,390 ms。这不是 xterm/WebGL 根因。检查 FileTerm 的终端路径未发现等价的 renderer OSC 中间件：OSC 52 由 xterm parser 处理，本地 OSC 7 sidecar 限 16 KiB，SSH CWD 扫描缓冲区约 8 KiB。该特定的无界缓存路径目前没有对应实现，但仍不能据此解释或排除 FileTerm 活跃 Claude 会话的其他 WebView2 内存增长原因。

这解释的是**关闭聚焦的 WebGL 终端后内存不回收**，尤其是多次开关终端、且缓冲区已有较多 scrollback 的场景；它不能单独解释一个始终打开的 Claude Code 终端从 120 MB 持续涨至 1 GB。FileTerm 另外已将输出限制为每窗口 4 个解析未确认批次、每批最多 16 KiB，native 队列有固定容量，超限记录并跳过显示字节；原生 transcript 限 200,000 字符，xterm scrollback 限 6000 行。因此仍须把活跃终端的图集/纹理存储增长与已关闭终端保活分别测量，并在 Tauri/WebView2 下复测 PID/启动时间对应的 private bytes、GPU/native memory 和 tab churn。

本次回归使用 Windows Playwright Chromium 156、RTX 3070 Laptop / driver 32.0.16.1742，不是原生 Tauri/WebView2；不能据此声称单个长会话的 1 GB 增长已解决。队列实际最多 8 批/标签、128 批/窗口；native transcript 限 200,000 字节，renderer transcript 回放上限为 200,000 个 JS 字符单元。

## 2026-10-22：降低活跃 WebGL 终端的图集峰值

Tabby [#11718](https://github.com/Eugeny/tabby/issues/11718) 的约 390 MB 是关闭 20 个标签后的 renderer RSS，约 405 MB 是单个异常 OSC 流修补后的 renderer RSS；不是十个活跃终端的浏览器进程组峰值。[#5667](https://github.com/Eugeny/tabby/discussions/5667) 的 400 MB+ 又是另一位用户创建三个 SSH 配置后的反馈。不得将这些数字当成 FileTerm 相同负载的上限保证。

参考 [VS Code xterm 生命周期](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/terminal/browser/xterm/xtermTerminal.ts)、[Tabby frontend](https://github.com/Eugeny/tabby/blob/master/tabby-terminal/src/frontends/xtermFrontend.ts) 和 [electerm 初始化](https://github.com/electerm/electerm/blob/master/src/client/components/terminal/mixins/term-init.js) 检查 WebGL、解析背压和销毁路径。FileTerm 已有这些机制，本轮修补的是其自己的图集保留策略：

- 小视口图集预算下限由 32 MiB 降至 4 MiB；仍按共享终端的最大物理像素视口保留四份 RGBA surface 空间，不降低滚动历史、颜色精度或输出完整性。
- 原清理逻辑清空字形但保留全部画布页，每个 context 仍持有这些空白页的纹理。现在复用最大普通页、保留 oversized glyph 专用页，清除冗余画布 backing pixels 并通知移除；既有纹理适配器将闲置槽缩回 1×1。所有共享终端的旧字形坐标同时失效。
- 长测夹具新增 WebGL 路径和去重图集字节采样，避免将 DOM-only 压测误称为默认 WebGL 路径的验证。

Windows Chromium 十个终端、每个 6000 行历史、相同动态 RGB/组合字形负载，各 60 秒，未强制 GC；每十秒采样进程 private bytes：

| 指标                             | 修改前     | 修改后    |
| -------------------------------- | ---------- | --------- |
| 图集画布采样峰值                 | 34 MiB     | 4 MiB     |
| GPU 进程私有内存采样峰值         | 784.9 MiB  | 293.9 MiB |
| 浏览器进程组私有内存合计采样峰值 | 1157.7 MiB | 612.4 MiB |
| 每终端完成写入批数               | 156        | 200       |
| P95 事件循环间隔                 | 403.1 ms   | 322.3 ms  |

仅降低预算而保留空白页的中间方案存在吞吐/延迟退步，未作为最终实现。以上是有限时间合成压测的采样峰值，不是精确瞬时峰值或真实 SSH/Claude 长任务结论；private bytes 也不等于独占显存。

回归覆盖小视口旧预算不回收、大视口共享预算、最大页复用、冗余页像素释放、移除通知、oversized glyph 支持，以及共享终端重绘。生产生命周期夹具追加 20 次清理，纹理尺寸/闲置槽/销毁归零检查通过。

原始数据与复现 runner 保存在本地 `D:\CodeFiles\renderer-validation-archive\peak-memory-2026-10-22\`。DOM fallback 峰值和真实远程长任务仍需单独测量。

### 原生 WebView2 同长对照

使用同一台 Windows 机器、同一 FileTerm dev 进程及 Vite 页面，旧版注入 HEAD 中的图集代码，新版使用工作区实现；均运行 300 秒、10 个活跃终端、每个约 6000 行 scrollback、相同 RGB/组合字形输出。每 5 秒采样一次以指定 `fileterm.exe` 为根的 WebView2 子进程树 private bytes。表内峰值是采样所得，进程组一栏是在每个采样点先求进程 private bytes 总和，再取峰值。

| 指标                                 |      旧策略 |    新策略 |
| ------------------------------------ | ----------: | --------: |
| 图集 canvas backing bytes            |     106 MiB |     4 MiB |
| GPU 子进程 private bytes             | 1,977.9 MiB | 276.1 MiB |
| renderer 子进程 private bytes        |   566.2 MiB | 461.0 MiB |
| WebView2 子进程组 private bytes 总和 | 2,532.3 MiB | 779.0 MiB |
| 每终端完成写入轮数                   |       1,811 |     3,291 |
| P95 event-loop 间隔                  |    165.0 ms |   99.5 ms |
| 最大 event-loop 间隔                 |  1,159.2 ms |  216.2 ms |
| 输入/点击延迟 P95                    |    2,408 ms |    848 ms |

修改版压测的诊断日志记录了两条 `renderer-progress-slow`（synthetic interaction duration 达 1,200 ms）；测试返回页面时另记录一条约 2,029 ms 的 `event-loop-gap` 和一条 `renderer-long-frame`。旧版同长压测记录一条 `renderer-long-frame`。两段压测窗口内没有 ERROR/FATAL。进程 private bytes 不是独占显存，5 秒采样也可能漏掉瞬时峰值；这些数值说明本机合成工作负载下的 WebView2 进程开销有明显下降，不能保证所有远程长任务都稳定在 Tabby issue 报告的 renderer RSS 范围内。
