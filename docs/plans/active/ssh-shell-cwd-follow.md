# SSH 终端目录跟随计划

## 目标

让 SSH shell 主动上报真实工作目录，并由工作区运行时按策略驱动远程文件面板，避免在 renderer 解析 `cd` 文本或推测目录变化。

## 分层

```txt
remote shell integration
  -> OSC 7 cwd extraction
    -> controller cwd changed callback
      -> workspace runtime follow policy
        -> remote file snapshot refresh
          -> renderer display and toggle
```

- `packages/core`：区分 `shellCwd`、`remotePath`、`followShellCwd`。
- `ssh-session-controller`：探测远端登录 shell，安装可降级的 cwd integration，并解析 OSC 7。
- `workspace-session-runtime`：按 tab 去重 cwd，串行执行目录读取，决定是否跟随。
- `renderer`：只展示状态和切换跟随，不解析终端输出。

## 第一阶段范围

- 支持 bash、zsh、fish 和 POSIX 风格 shell 的会话内注入策略。
- 注入或 shell 探测失败时静默降级，不影响终端、SFTP 与已有文件操作。
- SSH tab 默认开启跟随。
- 手动浏览文件目录后保持跟随开关开启，但相同 cwd 的重复 prompt 不抢回文件面板；下一次真实 cwd 变化重新跟随。
- cwd 相同则不重复刷新远程目录。

## 暂不处理

- 不持久化每个 profile 的跟随开关。
- 不尝试解析用户输入的 `cd` 命令。
- 不要求 FTP 会话具备 cwd 跟随能力。
- 不把远端 shell 类型暴露到 renderer。

## 验收

- bash/zsh 中执行成功与失败的 `cd`，只在真实 cwd 改变时刷新。
- `pushd/popd`、alias/function 内切目录可被识别。
- 手动浏览文件区后，同 cwd prompt 不回抢；再次切换 shell cwd 后恢复跟随。
- 关闭跟随后 shell cwd 仍更新，但文件区不跳转；重新开启时同步到最新 cwd。
- `npm run typecheck -w @fileterm/desktop` 通过。

## 进度记录

- 2026-06-21：完成 core 状态、OSC 7 解析、shell 策略注入、runtime 跟随、IPC 开关和 renderer 入口；构建与本地 shell 语法检查通过，待连接真实远端主机回归。
- 2026-06-21：cwd 上报改用 `pwd -P` 物理路径并限制异常长的 OSC 7 payload，避免 `/bin/X11 -> .` 这类循环符号链接让逻辑 `$PWD` 无限增长并触发重复目录读取。
- 2026-06-30：文件面板 root 视角与交互 shell 提权解耦；注入探测 1.5 秒未完成时释放暂存输出，确保探测失败不会阻塞终端输入输出。
