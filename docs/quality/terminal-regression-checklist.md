# Terminal Regression Checklist

本文记录终端层最近一次“高流量输出 + 全屏 TUI”回归后，后续修改 `TerminalView`、SSH 会话流、终端尺寸同步时必须复测的最小清单。

## 1. 目的

终端层目前已经覆盖了几类彼此容易冲突的能力：

- 普通 shell 输出
- 大量连续流式输出
- `\r` 单行重绘式进度条
- `nano` / `vim` 这类全屏 TUI
- 连接启动 transcript 的一次性回放

这些能力共用一套 `TerminalView` 写入链路，后续只要调整：

- `terminal:data` 写入方式
- transcript hydration / bootText 回放
- `fitAddon` 行列计算
- PTY resize / shell ready 时机

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

### 2.5 连接启动信息

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
- transcript 只做低频、一次性、启动期补偿
- 不要为了修欢迎信息去恢复高频 reset / 全量回放

## 4. 修改注意事项

后续如果改这些点，必须跑完整清单：

- `TerminalView.tsx` 内的 `terminal.write` / flush / transcript hydration
- `workspace-session-runtime.ts` 内的 `terminalTranscript` / `terminal:state`
- `ssh-session-controller.ts` 内的 shell transcript 维护
- `fitAddon` 尺寸同步和 PTY resize

尤其不要轻易把下面两类逻辑重新混在一起：

- “全屏 TUI 稳定性”
- “启动 transcript 回放”

这两者一旦耦合，最容易出现：

- `nano` 黑屏
- `vim` 退出后 shell 被污染
- `apt` 进度条残影

