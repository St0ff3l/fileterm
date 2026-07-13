# 连接协议本机验证

本清单不依赖公网服务；所有监听地址使用 `127.0.0.1`，测试结束后停止对应命令。

## SSH keyboard-interactive

FileTerm 的「Keyboard-interactive / MFA」会在服务端实际发出 challenge 后弹出逐项输入框。它不会把保存的密码重复填入 OTP/MFA 提示。

可用带 PAM keyboard-interactive 的 SSH 测试机验证；仅开启 `PasswordAuthentication` 的服务器不会发 challenge，应选择「密码」认证而非本模式。登录失败时先确认服务端 `sshd_config` 中启用了 `KbdInteractiveAuthentication yes`。

## SOCKS5 / HTTP 代理

启动临时 HTTP CONNECT 代理或 SOCKS5 代理后，在连接的「代理服务器」页填写本机端口。验证标准：代理停止时 FileTerm 明确显示代理错误；代理启动时 SSH/Telnet 可连接目标。

## 隧道

建立 SSH 连接后使用本机服务验证：

```bash
python3 -m http.server 8080 --bind 127.0.0.1
```

- 本地转发：`127.0.0.1:18080 -> 远端 127.0.0.1:8080`，随后访问 `http://127.0.0.1:18080`。
- 远程转发：远端监听端口指向本机 `127.0.0.1:8080`，从远端访问该监听端口。
- 动态转发：监听 `127.0.0.1:1080`，用支持 SOCKS5 的客户端指定此代理。

关闭连接标签后，监听端口应立刻可重新绑定。

## Telnet

仅在隔离网络使用 Telnet。可用 `telnetd` 或网络设备测试 RFC 854 协商；检查终端中不出现 IAC 控制字节，窗口 resize 不污染输出。

## Serial

- macOS：优先选择 `/dev/cu.*`，而不是 `/dev/tty.*`。
- Linux：使用 `/dev/ttyUSB*` 或 `/dev/ttyACM*`；权限不足时将用户加入 `dialout` 后重新登录。
- Windows：使用 `COM3`；两位数端口也直接填 `COM10`，不需要 `\\.\` 前缀。

使用 USB 串口设备或虚拟串口对验证 115200/8N1；拔出设备后会话应明确断开而不会残留句柄。

## JSON 导入、隧道与 WebDAV

- JSON 导入先确认预览中重复项的处理方式；预览绝不显示连接或代理密码。兼容格式导出会要求选择目录，并为每条连接生成单独的 JSON 文件。
- SSH 工作区底部切换到“隧道”后，分别验证运行时 `-L`、`-R`、`-D` 的新增、停止和删除；停止或关闭标签后端口必须能立即重新绑定。
- WebDAV 同步默认只接受 HTTPS。仅在隔离测试环境中启用 HTTP；先上传，再从另一份本地 profile 数据下载，确认 ETag 冲突会阻止未确认的上传，且远端 JSON 不包含密码、私钥或代理密码。
