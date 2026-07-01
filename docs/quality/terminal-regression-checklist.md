# Terminal Regression Checklist

本文记录终端层最近一次“高流量输出 + 全屏 TUI”回归后，后续修改 `TerminalView`、SSH 会话流、终端尺寸同步时必须复测的最小清单。

## 1. 目的

终端层目前已经覆盖了几类彼此容易冲突的能力：

- 普通 shell 输出
- 大量连续流式输出
- `\r` 单行重绘式进度条
- `nano` / `vim` 这类全屏 TUI
- 连接启动 transcript 的一次性回放
- 终端内搜索、Web 链接识别、Unicode 11 字符宽度

这些能力共用一套 `TerminalView` 写入链路，后续只要调整：

- `terminal:data` 写入方式
- transcript hydration / bootText 回放
- `fitAddon` 行列计算
- PTY resize / shell ready 时机
- xterm addon 加载顺序或搜索 UI

都可能引入回归。

## 2. 必测命令

### 2.1 nano

```bash
sudo nano /opt/docker/frpc/frpc.toml
```

通过标准：

- `nano` 界面能正常出现
- 底部快捷键栏完整可见
- 输入、保存、退出正常
- 不出现整屏黑底只剩光标块的情况

### 2.2 vim

```bash
sudo vim /opt/docker/frpc/frpc.toml
```

通过标准：

- `vim` 能正常进入和退出
- 退出后 shell 能正常回到 prompt
- 不出现终端响应串泄漏到 shell，例如：

```txt
2RR0;276;0c10;rgb:...
-bash: 2RR0: 未找到命令
```

### 2.3 单行覆盖式进度条

```bash
for i in $(seq 1 200); do printf "\rInstalling package %03d/200 [%-50s]" "$i" "$(printf '%*s' $((i/4)) '' | tr ' ' '=')"; sleep 0.03; done; printf "\nDone\n"
```

通过标准：

- 屏幕上只保留一条进度行不断更新
- 最终只留下完整的 `Installing package 200/200 ...`
- 下一行输出 `Done`
- 不残留白条、阶梯条、控制序列、乱码

### 2.4 真实高输出安装

```bash
sudo apt install --reinstall libreoffice clang llvm gimp inkscape ffmpeg python3-dev -y
```

通过标准：

- 不黑屏
- 不自动 reload
- `apt/dpkg` 进度显示不残影
- 命令结束后 prompt 正常

### 2.5 多行进度条和窗口拖拽

用下面命令模拟 3 行进度条同时刷新，然后在执行过程中拖拽窗口大小：

```bash
bash -c 'printf "\033[?7l\n\n\n"; trap "printf \"\033[?7h\n\"" EXIT; for i in $(seq 0 100); do cols=$(tput cols); width=$((cols - 18)); [ "$width" -lt 10 ] && width=10; printf "\033[3F"; for n in 1 2 3; do filled=$((width * i / 100)); empty=$((width - filled)); printf "\r\033[2KTask %d [%-*s] %3d%%\n" "$n" "$width" "$(printf "%${filled}s" | tr " " "#")" "$i"; done; sleep 0.04; done; printf "\033[?7h\nDone\n"'
```

通过标准：

- 三条进度最终都到 `100%`
- 进度条不会因为窗口变宽而贴到真实右边界
- 拖拽窗口时不出现大片阶梯残影
- 命令结束后，上下键翻 shell 历史不会把历史内容画到 prompt 上方

### 2.6 连接启动信息

重连一个 SSH tab，确认 prompt 上方还能看到类似：

```txt
连接主机...
连接主机成功
Linux ...
Last login: ...
```

通过标准：

- 启动 transcript 会显示
- 但不会因为 transcript 回放破坏 `nano/vim`

### 2.7 终端搜索、链接和宽字符

在终端里输出一些可搜索内容、URL、宽字符和 Emoji：

```bash
printf 'FileTerm search Search\nhttps://example.com\nPowerline 字符 Emoji 😀\n'
```

通过标准：

- `⌘F` / `Ctrl+F` 打开终端内搜索框，不触发文件编辑器搜索。
- 搜索支持上一条/下一条，`Aa` 能切换大小写，`.*` 能切换正则。
- HTTP/HTTPS 链接悬停可识别，点击能打开链接。
- 中文、Powerline 字符和 Emoji 不明显挤压或造成光标错位。
- 粘贴长命令并回车后，终端内不残留白色选区；普通 selection 应保持半透明灰色。

## 3. 当前脆弱点

当前实现里最容易回归的是 transcript hydration，也就是：

- main 进程会维护一份 `terminalTranscript`
- renderer 挂载终端时，可能用这份 transcript 把“连接主机... / Linux ... / Last login ...”补回屏幕

这套机制的价值是保住启动阶段的欢迎信息，但它也有风险：

- 如果回放时机过晚，可能把已经在运行的 `nano/vim` 终端重置掉
- 如果回放内容里混入终端控制序列，可能污染 TUI 或 shell
- 如果回放过于频繁，会重新引入大输出时的卡顿、闪烁或黑屏

一句话理解：

```txt
hydration 是“启动体验补偿”，不是实时终端同步机制
```

所以后续改动必须坚持：

- 实时流优先走 `terminal:data`
- 实时 PTY 数据保持原始控制流，交给 xterm 解析；不要在 renderer 里统一改写 `\r` / `\n`。
- transcript 只做低频、一次性、启动期补偿
- 不要为了修欢迎信息去恢复高频 reset / 全量回放

## 4. 2026-06 nano / 进度条修复结论

这次回归反复出现了三类症状：

- `nano` 打开后没有底部菜单，输入内容回车后消失。
- bash 上下键翻历史记录时，历史内容被画到 prompt 上方或吃掉旧输出。
- 多行进度条贴近右边界时，拖拽窗口会出现阶梯残影和历史污染。

最终确认的修法如下。

### 4.1 不要在 renderer 改写实时 PTY 控制流

曾经为了修进度条，尝试在 `TerminalView` 里识别 `\r` 并手动转换为清行、上移、多行重绘序列。这个做法会破坏真实终端协议：

- `nano/vim` 依赖原始光标移动和 alternate screen 控制序列。
- bash/readline 依赖原始 `\r`、清行和终端列数来重绘当前输入行。
- renderer 统一改写 `\r` / `\n` 会让全屏 TUI 和 shell 历史重绘互相污染。

正确原则：

```txt
实时 PTY 数据保持原样 -> xterm 解析
renderer 只做 FileTerm 固定文案本地化，不做控制序列修补
```

### 4.2 xterm cols 必须和后端 PTY cols 完全一致

这次有一个非常容易误判的坑：

```txt
本地 xterm 用真实窗口宽度
后端 PTY 上报一个更小的保守 cols
```

这个看起来能让进度条离右边界远一点，但会直接破坏 bash/readline：

- 后端 shell 按较小 `cols` 计算换行和清行。
- 前端 xterm 按较大 `cols` 渲染。
- 上下键翻历史时，readline 发出的清行和光标移动会落到错误位置，表现为“吃上去”。

正确修法是：本地 `terminal.resize(cols, rows)` 和后端 `pty.resize(cols, rows)` 必须使用同一个 `cols`。

当前策略：

- 平稳状态下，用 `fitAddon.proposeDimensions()` 的真实宽度计算 `cols`，本地和后端保持完全一致。
- 保留少量 guard cols，避免输出紧贴最右边界。
- 用户横向拖拽窗口时，临时冻结上一帧 `cols`，避免拖拽过程里连续改列数。
- 横向拖拽停止后，再把真实宽度对应的 `cols` 一次性同步给本地 xterm 和后端 PTY。

这样可以同时保证：

- `nano/vim` 的菜单和状态栏不会错位。
- bash/readline 上下键历史重绘不会吃旧内容。
- 多行进度条不会在拖拽过程中连续重排出阶梯残影。
- 拖拽结束后，普通表格命令能恢复使用真实可见宽度。

### 4.3 行数保持动态，但保留 1 行安全余量

列数可以按最小窗口稳定固定；行数不要硬编码。原因：

- 文件面板高度可拖拽。
- `nano/vim` 需要知道当前真实可用高度。
- 行数固定会让全屏 TUI 的底部菜单和文件 dock 更容易互相挤压。

当前策略是继续使用 `fitAddon.proposeDimensions()` 的行数，但减去 1 行安全余量，再同步给 xterm 和后端 PTY。

### 4.4 不要重新启用 WebGL 作为默认渲染器

WebGL 原本是为了高频输出性能引入的，但这次验证中它会放大渲染层问题，尤其是搜索高亮、selection、TUI 重绘和窗口 resize 混在一起时更难排查。

当前策略：

- 不默认加载 `@xterm/addon-webgl`。
- 保持 xterm 默认渲染路径。
- 先确保 PTY 控制流和尺寸同步正确，再考虑未来单独评估硬件加速。

### 4.5 搜索高亮和普通 selection 必须分开

终端搜索可以使用 SearchAddon decoration 做浅白高亮，但普通 selection 不能复用同一套浅白底色，否则粘贴长命令或搜索关闭后容易残留白块。

当前策略：

- SearchAddon decoration：浅白底、黑字，用于当前搜索命中。
- xterm 普通 selection：半透明灰色。
- 粘贴、键盘输入、远端输出时主动清理临时 selection / decoration。

## 5. 修改注意事项

后续如果改这些点，必须跑完整清单：

- `TerminalView.tsx` 内的 `terminal.write` / flush / transcript hydration
- `workspace-session-runtime.ts` 内的 `terminalTranscript` / `terminal:state`
- `ssh-session-controller.ts` 内的 shell transcript 维护
- `fitAddon` 尺寸同步和 PTY resize
- `TerminalView` 的 selection/search decoration 清理逻辑

尤其不要轻易把下面两类逻辑重新混在一起：

- “全屏 TUI 稳定性”
- “启动 transcript 回放”

这两者一旦耦合，最容易出现：

- `nano` 黑屏
- `vim` 退出后 shell 被污染
- `apt` 进度条残影
- bash/readline 上下键历史记录“吃上去”
