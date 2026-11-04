# xterm 适配与扩展

这里集中维护 FileTerm 对 xterm 6 的渲染、输入和缓冲区适配。使用私有字段或依赖 xterm DOM 的实现应放在这里，升级 xterm 时一起复核。

`terminal-diagnostics.ts` 在 open 后计时同步解析/绘制并注册数字资源摘要；关闭时恢复自身挂接的方法、移除引用。升级 xterm 时核对私有方法覆盖状态与队列计量单位，未知值不能当作零。字段解释见 [日志文档](../../../../../docs/quality/logging.md#解析绘制与内存占用的补充采集)。

| 文件                                                          | 职责                                                                           |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `terminal-emoji-widths.ts`                                    | emoji、VS16、键帽、旗帜、肤色和 ZWJ 组合列宽                                   |
| `terminal-symbol-glyph-renderer.ts`                           | 按 buffer 列宽固定符号显示槽；越界笔画等比收进单元格，保持跨格选中时的笔画完整 |
| `terminal-block-glyph-renderer.ts`、`box-drawing.ts`          | 按网格自绘块字符和框线                                                         |
| `terminal-selection-renderer.ts`                              | 防止选中装饰背景遮挡相邻字形                                                   |
| `terminal-foreground-adapter.ts`                              | 普通白色文字随默认背景采用主题前景色                                           |
| `terminal-render-cache.ts`、`terminal-shared-glyph-widths.ts` | 缓存上限、相同字体环境间共享测量和失效处理                                     |
| `terminal-log-colorizer.ts`                                   | 日志着色规则、xterm buffer 单元颜色修改及还原                                  |
| `terminal-input-compat.ts`                                    | IME 组合事务、Shift 数字键、原生 input/keydown 与 xterm onData 去重            |
| `terminal-transcript-replay.ts`                               | 避免历史截断后从半段控制序列开始回放                                           |
| `terminal-responses.ts`                                       | 终端查询回包识别及历史回放的合成回包过滤                                       |
| `terminal-vim-selection.ts`                                   | Vim 接管鼠标后，从 buffer 恢复 Visual 选区用于复制                             |
| `terminal-webgl-grid.ts`                                      | WebGL backing resize 同步与共享字形图集清理后的跨终端失效                      |
| `terminal-webgl-display-grid.ts`                              | 小数显示投影与整数图集栅格隔离；文字矩形按设备像素对齐                         |
| `terminal-render-recovery.ts`                                 | 字体/DPI 缓存校准、普通/备用屏幕切换重绘和标签激活渲染恢复                     |
| `terminal-disposal.ts`                                        | WebGL 光标/恢复定时器、原生 context 与终端隐藏 resize 队列清理                 |
| `terminal-wheel-compat.ts`                                    | 将 WebView 重定向到 document 的滚轮事件交还 xterm                              |
| `terminal-options.ts`                                         | TUI、ConPTY、本地选择等兼容选项                                                |
| `xterm-compat.css`                                            | 禁用影响 DOM renderer 单元格测量的中文标点间距                                 |

`components/terminal-lifecycle-core.ts` 负责接入和销毁渲染适配：Unicode11 和 emoji 列宽在 `terminal.open()` 前安装，DOM 适配在其后安装；选区、符号、块字形的注册顺序保持不变。

输入适配由 `terminal-lifecycle-input.ts` 接入，兼容处理先于应用快捷键。应用层继续决定连接/活动状态、串口独占、快捷键、会话输入队列和 PTY resize；不要绕过这些守卫直接向后端发送输入。滚轮目标判定和缩放归属也留在应用层，适配只转发已经确认属于终端的事件。

跨组件的字体加载/DPI 观察仍在 `app/font-metrics.ts`，Monaco 与 xterm 共用；仅 xterm 需要的字体栈脉冲和 atlas 重建在本目录。标签激活重试和隐藏窗口尺寸恢复的调度、主题/字号后的 PTY 尺寸同步，以及 transcript 回放时机仍由组件编排。

`styles/features/session.css` 和各主题 skin 继续维护终端布局、选区颜色与自定义滚动条的样式契约；与字形测量有关的排版修正在本目录的 `xterm-compat.css`。

单元测试在 `apps/tauri/tests/renderer/terminal-*.test.mjs`；真实 xterm 的字体、字宽、符号、框线和鼠标选区回归在 `apps/tauri/tests/browser/terminal-*.mjs`。这些 fixture 直接加载生产模块，移动文件时须同步路径。

`node apps/tauri/tests/browser/terminal-webgl-glyph-layouts.mjs` 扩展检查 10 档倍率、7 种单窗/分屏布局、4 种字体和 8–28px 全部整数档位，另含超范围字号、主题/行高切换、共享图集重建与隐藏恢复。`FILETERM_WEBGL_DPR=1.2` 可定位单档倍率。覆盖层仍按显示网格定位，但其 `_refreshCharAtlas` 必须使用整数单元尺寸；上游共享图集的 key 没有比较单元尺寸，不能让覆盖层先创建小数图集。升级 xterm 时一起复核此边界。

`terminal-webgl-atlas-cache.ts` 修正 addon-webgl 0.19 的清理引用与合并页复用，由 grid 适配器在绘制前按共享终端最大视口回收历史颜色变体；小视口预算下限为 4 MiB，大视口保留四份可见 RGBA surface 的空间。清理时复用最大普通页、保留 oversized glyph 专用页，释放其余空白页的 backing pixels；各 context 的闲置纹理槽由 texture 适配器缩为 1×1。预算是缓存回收触发线，不是进程总内存上限。升级 xterm 时复核私有结构与 RGB/重复清理回归。

`terminal-webgl-textures.ts` 随 grid 适配器管理每个 renderer 的纹理槽；页面尺寸变化时替换纹理对象，未使用槽保留 1×1 占位，避免合并或字体替换后留下旧 storage/mip levels。实际显存释放由驱动决定，测试 API 请求统计不代表显存；见 [Windows 复核](../../../../../docs/quality/terminal-windows-texture-storage-2026-10-21.md)。
