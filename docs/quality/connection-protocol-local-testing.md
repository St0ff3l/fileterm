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

FileTerm 当前已经有串口连接，串口会话是终端型连接，不提供远程文件面板。连接配置会通过 Rust bridge 扫描系统串口并显示设备名/USB 信息，但扫描结果不代表设备一定能打开；仍然保留手动填写路径的能力。最容易验证的方法是准备一个 USB 转串口模块，并把两个串口的 `TX` 与 `RX` 交叉连接、`GND` 对接；不要把电源脚直接短接。两端都使用 `115200 / 8 数据位 / 1 停止位 / 无校验 / 无流控`。

串口的「终端」页还支持以下设备侧常用选项：

- 发送换行：不转换、`LF`、`CR` 或 `CRLF`。它只转换输入中的行结束符，不会凭空给普通文本追加换行。
- 输入模式：`Text` 按当前字符编码发送；`Hex` 以一行作为发送单位，按回车发送，支持退格编辑，以及空格、冒号、逗号、下划线和 `0x` 前缀，例如 `0x41 42 43`。
- 输出模式：`Text` 按当前编码显示；`Hex` 以大写字节显示，便于查看协议帧和不可见控制字节。
- 本地回显：发送后立即在当前终端显示一份；设备自身也回显时会出现两份，这是预期行为。
- 断线行为：不自动重连、按回车重连、断线后每 2 秒后台自动重连。主动点击断开或关闭标签不会触发自动重连。

- macOS：插入设备后运行 `ls /dev/cu.*`，在连接配置中优先填写 `/dev/cu.usbserial-*` 或 `/dev/cu.SLAB_USBtoUART`，不要优先使用 `/dev/tty.*`。打开两个串口标签，一个发送文字，另一个应收到文字。
- Linux：使用 `/dev/ttyUSB*` 或 `/dev/ttyACM*`；权限不足时将当前用户加入 `dialout` 组并重新登录。没有实体设备时，可以用 Linux 虚拟串口对：

  ```bash
  socat -d -d pty,raw,echo=0,link=/tmp/fileterm-ttyA pty,raw,echo=0,link=/tmp/fileterm-ttyB
  ```

  然后在 FileTerm 连接 `/tmp/fileterm-ttyA`，用另一个终端或串口工具打开 `/tmp/fileterm-ttyB` 回显数据。仓库中的 `virtual_pty_round_trip_exercises_the_real_serial_stack` 也会在 Linux CI 中验证这一链路。

- Windows：设备管理器中查看端口号，填写 `COM3`；两位数端口直接填写 `COM10`，不需要 `\\.\` 前缀。没有实体设备时，需要安装虚拟 COM 对软件，再把两个成对端口分别填入两个 FileTerm 标签。

拔出设备或关闭虚拟串口后，会话应显示“串口设备已断开”，并且重新插入后可以重新连接，不应残留占用句柄。没有任何设备时，开发机仍可运行 `serial_port_contract_tests::maps_usb_metadata_without_accessing_hardware`，验证扫描结果的 bridge 数据结构；Linux CI 还会用 PTY 验证真实 `tokio-serial` 读写链路。macOS 的系统 PTY 不是实体串口，不能用它代替 `/dev/cu.*` 做最终验证；Windows/macOS 的最终打开、收发和拔插行为仍需要虚拟串口或实体设备做一次人工验收。

没有设备时仍可验证大部分行为：Rust 单元测试覆盖 `Text/Hex` 编码、换行转换、Hex 分隔符和 Hex 输出；Linux CI 的 PTY 测试覆盖真实异步串口读写。最终发行前仍要在 macOS、Windows 各使用一次虚拟串口对或实体设备，确认系统枚举、设备占用、拔插和驱动权限提示。

## JSON 导入、隧道与 WebDAV

- JSON 导入先确认预览中重复项的处理方式；预览绝不显示连接或代理密码。兼容格式导出会要求选择目录，并为每条连接生成单独的 JSON 文件。
- SSH 工作区底部切换到“隧道”后，分别验证运行时 `-L`、`-R`、`-D` 的新增、停止和删除；停止或关闭标签后端口必须能立即重新绑定。
- WebDAV 同步默认只接受 HTTPS。仅在隔离测试环境中启用 HTTP；先上传，再从另一份本地 profile 数据下载，确认 ETag 冲突会阻止未确认的上传，远端 JSON 包含密码/私钥口令/代理密码，下载到已存在的同端点连接时会更新凭据而不是按重复项跳过。测试文件用完必须删除。
