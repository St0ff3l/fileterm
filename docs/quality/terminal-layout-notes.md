# Terminal Layout Notes

本文记录 FileTerm 终端区域最近一次布局回归的原因和正确修法，避免后续继续通过“加 padding 试试”这类方式误判问题。

## 1. 现象

症状主要有两类：

- terminal 最后一行 prompt 看起来紧贴底部文件 dock，没有之前预期的两行左右留白。
- 给 `.terminal-host` 增加 bottom padding 之后，视觉上仍然几乎没有改善。

## 2. 根因

关键问题不只是样式数值，而是 `xterm` 的挂载层级：

- `fitAddon.fit()` 会根据实际挂载节点的可用尺寸计算终端列数和行数。
- 如果 `xterm` 直接挂在带内边距的 `.terminal-host` 上，padding 会混进可用高度计算。
- 这样看起来虽然外层写了留白，但终端最后一行仍然会被挤到靠近底部边界的位置。

一句话总结：

```txt
terminal spacing issue != pure CSS spacing issue
terminal spacing issue = xterm mount box and fit box are the same node
```

## 3. 正确修法

正确做法是把“视觉留白容器”和“xterm 实际挂载容器”拆开：

- 外层 `terminal-host` 只负责 padding、背景和阴影边界。
- 内层 `terminal-inner` 负责给 `xterm` 挂载和执行 `fitAddon.fit()`。
- `terminal-host` 需要 `box-sizing: border-box`，确保 padding 不把外层高度撑爆。

在最近一次回归里，我们又补了一个更稳的保护层：

- `fitAddon.proposeDimensions()` 计算出来的行数会减去 1 行安全余量，再上报给主进程。
- 这个余量专门留给 nano / vim 这类全屏 TUI 的底部状态行和菜单行，避免它们和文件 dock 的边界抢高度。
- `xterm` 内部的 `.xterm-screen` / `.xterm-helpers` 不再被额外强制拉成 `height: 100%`，让 xterm 自己按渲染尺寸管理内部画布。

### 3.1 当前悬浮命令输入条模型

2026-06 的工作区样式调整后，终端底部不再通过 `.terminal-host` 预留一块固定命令输入区域。当前模型是：

- `TerminalView` / xterm 仍然占满 shell 可用区域，输出可以延伸到文件面板上边界。
- `TerminalDock` 作为绝对定位控件悬浮在 shell 区域底部，使用半透明背景和 backdrop blur 做雾透效果。
- 文件面板抽屉按钮与 `TerminalDock` 同一水平线，收起和展开状态下都保持平行。
- 不要为了避开 `TerminalDock` 再给 terminal host 加固定 bottom padding；这会重新造成 xterm fit 尺寸和视觉容器尺寸不一致。
- 如果需要看被输入条覆盖的输出，优先通过终端滚动查看，而不是改变 PTY 行数或额外插入空行。

2026-06 的 nano / 进度条回归又确认了一条边界：终端列数不能出现“前端 xterm 一个 cols、后端 PTY 另一个 cols”的分裂状态。当前做法是按主窗口最小宽度、左侧栏宽度和终端 padding 算出稳定列数，并让本地 `terminal.resize(cols, rows)` 与后端 `pty.resize(cols, rows)` 使用同一个 `cols`。详细排查结论见 [terminal-regression-checklist.md](./terminal-regression-checklist.md)。

当前实现位置：

- 结构调整在 [apps/desktop/src/renderer/components/TerminalView.tsx](../../apps/desktop/src/renderer/components/TerminalView.tsx#L700)
- 通用尺寸约束在 [apps/desktop/src/renderer/styles/features/session.css](../../apps/desktop/src/renderer/styles/features/session.css#L43)
- 视觉留白在 [apps/desktop/src/renderer/styles/features/workstation-skin.css](../../apps/desktop/src/renderer/styles/features/workstation-skin.css#L499)

## 4. 历史背景

之前样式里真正影响 terminal 四边留白的是：

```css
.terminal-host {
  padding: 16px 18px 16px 15px;
}
```

后续一次样式调整把它改成了：

```css
.terminal-host {
  padding: 15px 0 0 15px;
}
```

这会让右侧和底部留白一起消失，所以如果要恢复旧体验，不能只盯着 terminal 输出或文件面板高度。

## 5. 排查顺序

以后遇到 terminal 底部留白异常，建议按这个顺序排查：

1. 先确认 `xterm` 挂载节点是不是直接等于带 padding 的容器。
2. 再确认 `fitAddon.fit()` 使用的是不是内层纯内容区尺寸。
3. 如果全屏 TUI 的底部状态行仍然挤掉，先看是否需要保留 1 行安全余量，而不是继续改 padding。
4. 如果问题来自悬浮命令输入条，先检查 `TerminalDock` 的绝对定位、层级和透明背景，不要直接给 terminal host 加 bottom padding。
5. 最后才调 `.terminal-host` 的 padding 数值。

## 6. 不推荐的修法

下面这些办法容易造成“看起来改了，但实际没解决根因”：

- 只增加 `.terminal-host` 的 bottom padding。
- 为悬浮命令输入条额外保留固定终端高度。
- 只调整 `.session-workspace` 或文件面板高度。
- 只在 terminal transcript 里插入额外空行。

这些方式可能短期改变视觉结果，但无法稳定保证 `xterm` 最后一行和文件 dock 的真实间距。
