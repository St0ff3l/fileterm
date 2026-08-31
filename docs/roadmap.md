# FileTerm 路线图

## 总体策略

路线图目标不是一次把所有协议做全，而是尽快形成一个可工作的桌面端 MVP，然后逐步打磨成可发布版本。

## 当前重构进度（2026-07-17）

仓库当前处于 **单运行时收口：Tauri 是唯一受维护、构建和发布的运行时，功能实现接近收口，真实协议与发行候选验收进行中**。历史 Electron 实现已从仓库彻底移除（删除提交 `2a2eb7ff`），不再保留兼容基线。

- 运行时核对：Tauri 位于 `apps/tauri`（`@fileterm/tauri`），是唯一可运行入口，拥有独立 renderer、构建和本地数据目录。仓库中不存在 `@fileterm/electron` 或 `@fileterm/desktop` workspace。
- Phase 0–2 的主体代码已落地：Tauri bridge/contract test、桌面壳、Rust JSON 存储、Workspace snapshot 与旧 Electron 用户数据的一次性可回滚迁移已经落地；Phase 0 在 renderer mount 前等待原生 metadata，并保留原生拖放路径直到目标成功消费，Phase 1 已补原生 File/View/Window 菜单与文件编辑器关闭取消状态；仍需实际 Tauri UI 手测，不能按“发行验收完成”表述。
- Phase 3 的 russh SSH 主链路已实现：受管 SSH 私钥库与口令交互、shell、SFTP、MFA、host verification、系统指标、CWD/远端用户跟随、重连水化、自动重连、远程编码、递归 chmod、单级 Jump Host、SOCKS5/HTTP CONNECT 出站代理及运行时 SSH `-L/-R/-D` 隧道均已接入 `apps/tauri/src-tauri`。
- Phase 3 已有本地 OpenSSH 的认证、exec、SFTP、HTTP/SOCKS5 代理、local 与 dynamic direct-tcpip 回归；MFA 使用真实 SSH 协议夹具验证。真实 sshd 的跳板、远程转发、sudo/root、CWD 事件和完整指标流仍是发行候选手测门禁。
- Phase 4 的 Transfer journal/断点、FTP/FTPS、Telnet、Serial、WebDAV、连接导入导出、偏好/窗口事件、CSP 和本地日志均已接入 Rust backend。显式/隐式 FTPS、WebDAV HEAD/PUT/GET + ETag/hash、Telnet HTTP CONNECT/SOCKS5 已有本地真实协议夹具；实体/虚拟串口、真实 Telnet 设备、真实 WebDAV 服务和三平台结果仍未全部取得。
- Phase 5 进行中：macOS Tauri 生产 DMG 已可打包并已有本机性能基线；自动 legacy 数据迁移与回滚代码已落地，签名 in-app updater、公证、Windows/Linux 包与 CI 结果、迁移演练和正式切换仍是发行前置。

更细的差距和里程碑以已归档的 [`docs/plans/completed/tauri-migration-progress.md`](plans/completed/tauri-migration-progress.md) 为准；Rust 后端的模块级拆分以已归档的 [`rust-backend-migration-plan.md`](plans/completed/rust-backend-migration-plan.md) 为准。

## Phase 0: 仓库初始化

目标：建立不会返工的工程基础。

交付：

- workspace 初始化
- Electron + React + TypeScript + Vite
- 基础 lint / format / tsconfig
- Monorepo 包结构
- 基础窗口与 preload 通信

验收标准：

- 桌面应用可启动
- Renderer 与 main 之间可进行类型安全通信
- 基础目录结构稳定

## Phase 1: 工作台骨架

目标：先做出像产品的桌面壳。

交付：

- 左侧连接列表
- 顶部标签栏
- 主工作区布局
- 底部传输任务面板占位
- 设置页基础框架
- 连接配置 CRUD 页面

验收标准：

- 可以创建和保存 SSH / FTP 配置
- 可以打开空白标签页工作区
- 布局接近目标产品形态

## Phase 2: SSH + SFTP MVP

目标：跑通最核心的工作流。

交付：

- SSH 连接
- xterm.js 终端渲染
- 终端输入输出
- resize
- SFTP 目录浏览
- SFTP 上传下载
- SSH/SFTP 联动布局

验收标准：

- 能打开 SSH 标签页并正常执行命令
- 能浏览远端目录
- 能上传下载文件
- 断开和错误状态可见

## Phase 3: FTP MVP

目标：加入第二种独立会话类型，但不污染 SSH 工作流。

交付：

- FTP 连接
- FTP 文件浏览
- FTP 上传下载
- FTP 新建目录、重命名、删除
- `file-only` 工作区布局

验收标准：

- FTP 会话可独立打开
- 不显示终端区
- 文件操作路径完整可用

## Phase 4: 传输中心与体验打磨

目标：从“能用”提升到“顺手”。

交付：

- 全局传输任务中心
- 任务进度、状态、取消、重试
- 最近连接
- 错误提示优化
- 重连能力
- 终端主题与字体设置
- 窗口与布局记忆
- 系统信息页本地化与指标稳定性修整
- 终端命令输入条雾透悬浮
- SSH 文件面板抽屉式收起/展开
- 工作区焦点模式
- 首页与工作区标签切换动效
- 侧栏收起态资源监控摘要
- 双栏文件编辑器体验

验收标准：

- 多任务传输稳定
- 常见错误能给出清晰提示
- 桌面体验接近可日常使用

当前已落地：

- 终端输入条已从占位式底栏调整为悬浮雾透条，终端输出可以延伸到工作区底部。
- SSH 文件面板支持抽屉式收起/展开，并保留上次用户拖拽高度。
- 顶部工具区加入工作区焦点模式，可一键收起侧栏和底部文件面板，再次点击恢复。
- 首页、SSH 终端、系统信息、新建标签切换时使用一致的方向动画，并尊重 `prefers-reduced-motion`。
- 收起侧栏下，系统监控以三条细柱展示 CPU、内存、交换空间，并保留名称与百分比。
- 文件编辑器已重做为左侧文件树、右侧 Monaco 编辑区的双栏布局。
- macOS 菜单栏托盘图标已改为独立 template 资源，不复用 Windows 应用图标。

## Phase 5: 发布准备

目标：形成可分发的桌面版本。

交付：

- macOS 安装包
- Windows 安装包
- 自动更新策略预留
- 崩溃日志和诊断信息
- 基础文档完善

验收标准：

- 双平台可以安装运行
- 关键功能回归通过
- 文档足够支持首次体验

## 后续阶段

以下内容不进入第一版，但可作为下一轮优先池：

- Linux 桌面专项适配
- SSH 隧道 / 端口转发
- 双远端面板
- 本地文件面板
- 命令片段与收藏
- 同步与备份
- 团队配置共享
- 移动端 companion

## 推荐开发顺序

最稳妥的实际落地顺序：

1. 先搭工程骨架
2. 先做布局和连接配置
3. 先打通 SSH 终端
4. 再接 SFTP 文件面板
5. 再补 FTP 独立会话
6. 最后统一传输中心和设置系统

## 近期待办

当前优先级已经从 Electron 结构拆分转为 Tauri/Rust 功能对齐：

1. 触发并确认 Windows/Linux Tauri socket lifecycle CI。
2. 配置 Tauri updater 的签名公钥、更新清单与 Windows/macOS 公证资产。
3. 在真实 SSH/代理与实体/虚拟串口设备上执行发行候选手测。
4. 完成三平台包签名、公证、数据迁移工具与回滚演练。
