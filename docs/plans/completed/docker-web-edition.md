# FileTerm Docker 浏览器访问计划

发起人希望把 FileTerm 装在服务器上，从其他设备用浏览器访问。用户确认 Docker 版须完整复用当前前后端。[Issue #162 的需求确认](https://github.com/St0ff3l/fileterm/issues/162#issuecomment-7221700292)

## 交付方案（已升级为原生 Web 架构）

- **初版（Xvfb + noVNC，已移除）**：早期方案在容器内安装官方 Linux 桌面包并通过 Xvfb/Openbox/x11vnc/websockify 串流画面。
- **现行原生 Web 版**：直接编译 Rust 后端独立二进制 `fileterm-web`（`--no-default-features --features web`）与现有 Vite React 构建产物 `apps/tauri/dist`，由 Caddy 作为网关负责 Basic Auth 与可选 HTTPS；容器内不再安装 X11、Xvfb、noVNC、GTK 或 WebKit2GTK。
- 前后端完整复用现有界面与 Rust SSH/SFTP/FTP/Telnet/Serial/存储逻辑：前端通过 `apps/tauri/src/bridge/` 运行时抽象切换至 `/api/bootstrap`、`POST /api/rpc`、`/api/ws` 及 `/api/web/*` 文件传输接口，原桌面独立子窗口由 `WebWindowOverlayHost` 以模态浮层承载。
- 认证与安全：未设置 `FILETERM_WEB_PASSWORD` / `FILETERM_WEB_PASSWORD_HASH` 时容器拒绝启动（除非显式设置 `FILETERM_WEB_DISABLE_AUTH=true`）；`/api/bootstrap` 下发 `HttpOnly` `fileterm_session` Cookie，Caddy 鉴权覆盖所有应用入口。Rust 通过私有 Unix socket 接受网关请求，WebSocket 额外校验会话 Cookie，写请求和 WebSocket 检查 Origin。Linux Compose 默认使用 host 网络，Caddy 仅监听 `127.0.0.1:34538`。
- 数据卷 `/data` 持久化存储配置文件、`/data/.machine-id` 与 `/data/.session-secret`，确保重建容器后凭据仍可解密。

## 验收与发布

- Rust 三种特性组合（`desktop`、`web --bin fileterm-web`、`--all-features --all-targets`）与单元/桥接测试（`web::tests`、`web-bridge.test.mjs`）均需通过。
- 通过 `apps/tauri/web/tests/run.sh` 构建 Docker 镜像，校验镜像不含 `Xvfb` / `x11vnc` / `fluxbox` / `websockify`，并使用 Playwright 验证 Basic Auth、会话 Cookie、同源拦截、新建连接、本地终端、容器重启持久化及 `FILETERM_WEB_TLS=internal`。

部署和升级见 [Docker 使用说明](../../../apps/tauri/web/README.md)。
