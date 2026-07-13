# FileTerm 架构规划

## 1. 目标

FileTerm 第一版要解决的是“桌面端远程工作台”的核心闭环，而不是协议大而全。

核心体验：

- 一个连接列表
- 一个多标签工作区
- SSH 会话中终端与 SFTP 文件联动
- Telnet 与 Serial 终端会话（不附带 SSH 文件或系统能力）
- FTP 会话中只呈现文件管理
- 上传下载任务全局可见

## 2. 当前实现状态

当前仓库已经具备 MVP 雏形，主要能力包括：

- Monorepo workspace 基础结构
- Electron 主窗口启动
- 独立连接管理器窗口
- React 工作台主界面
- 标签页工作区模型
- SSH / FTP 连接配置的新增、编辑、删除、持久化
- 基于文件的 profile 存储（`profiles.json`）
- 本地目录浏览
- 本地文件读写
- SSH 会话连接
- SSH shell 输出接入
- SSH keyboard-interactive/MFA、SOCKS5/HTTP outbound proxy、单级 Jump Host 与运行时 `-L/-R/-D` tunnel service；隧道运行状态归属具体 workspace tab，断线自动回收。
- 终端写入与 resize IPC
- SFTP 远程目录浏览
- 远程文件读取与写回
- FTP 会话连接与远程文件能力
- 上传下载任务队列与进度状态
- 工作区快照广播到多个窗口
- 预览态数据与桌面运行态双模式
- SSH 主机系统信息页与轮询刷新
- 首页、终端、系统信息、新建标签之间的工作区切换动效
- SSH 工作区底部文件面板的抽屉式收起/展开
- 终端命令输入条的雾透悬浮布局
- 侧栏收起态的资源监控摘要
- 文件编辑器的左侧文件树与右侧 Monaco 编辑区
- 工作区焦点模式，可同时收起侧栏和底部文件面板
- macOS 菜单栏 template 托盘图标资源
- 单一连接导入入口同时支持 `~/.ssh/config`、SSH 配置文本和外部 JSON；导入统一采用 main-process preview plan，renderer 不接收导入 secret。
- Telnet（RFC 854 基础协商）和 Serial（main-process device handle）终端会话；两者不进入 SFTP、CWD、sudo 或系统指标链路。
- 设置页的 WebDAV 手动配置同步：portable package 不包含连接 secrets，上传使用 ETag 检测冲突。

当前主题系统也已经开始成形，主要落点包括：

- `apps/desktop/src/renderer/styles/themes/tokens.css`
  - 基础视觉 token，供半径、阴影、间距等全局样式复用。
- `apps/desktop/src/renderer/styles/themes/default-dark.css`
- `apps/desktop/src/renderer/styles/themes/default-light.css`
  - 明暗主题变量与组件覆盖层。
- `apps/desktop/src/renderer/hooks/useThemeMode.ts`
  - 通过 `document.documentElement.dataset.theme` 切换主题。
- `apps/desktop/src/renderer/components/TerminalView.tsx`
  - 从 CSS 变量读取终端主题色，确保终端外观和全局主题联动。
- `apps/desktop/src/renderer/features/files/FileEditorModal.tsx`
  - Monaco 主题从 CSS 变量读取，跟随深色/浅色主题切换。

## 3. 技术栈

当前已安装、已接入的第三方项目和维护边界见 [integration-inventory.md](./integration-inventory.md)。本节包含部分阶段规划项，精确现状以该集成总表和 `package.json` 为准。

### 当前已采用

- Electron
- React
- TypeScript
- Vite
- xterm.js
- Monaco Editor
- OpenCC
- `ssh2`
- `basic-ftp`
- `electron-builder`
- 文件型 profile 存储

### 已放弃或暂缓

- Tailwind CSS：已放弃迁移。FileTerm 需要大量覆盖 xterm、Monaco、Electron 桌面壳和表格/文件面板细节，继续使用 `token -> theme vars -> component skins` 的 CSS 分层更可控。
- Radix UI / shadcn/ui：暂不引入。当前自有组件和桌面式密集布局更贴近产品形态，避免再叠一层组件体系。
- react-resizable-panels：暂不引入。现阶段文件面板和侧栏 resize 已由本地组件处理。
- Zustand：已完成评估，当前不引入。`App.tsx` 已拆为 7 个领域 hooks 与布局组件，尚未出现需要跨多层组件共享可变状态的稳定场景；继续使用 React state、领域 hooks、workspace snapshot 与 IPC 广播。
- zod：暂不引入。类型先收敛到 `packages/core`，运行时校验如果变复杂再单独评估。
- SQLite / Drizzle ORM：暂缓。当前连接配置使用文件型 profile 存储；等 profile、设置、传输历史的查询需求明确后再迁移。
- 系统钥匙串：暂缓。敏感信息存储策略需要单独做安全设计，不混在 UI 或协议改动里。

## 4. 高层分层

```txt
Renderer UI
  -> Application Stores
    -> IPC Bridge
      -> Desktop Services
        -> Session Controllers
          -> Protocol Adapters
            -> Remote Servers
```

分层职责：

- `Renderer UI`
  - 界面渲染
  - 用户交互
  - 布局与组件状态
- `Application Stores`
  - 标签状态
  - 当前工作区状态
  - 连接列表
  - 传输任务
- `IPC Bridge`
  - 暴露类型安全的前后端调用接口
- `Desktop Services`
  - 配置存储
  - 密钥管理
  - 会话创建与销毁
  - 文件传输调度
  - 系统信息采集调度与快照广播
- `Session Controllers`
  - 会话生命周期管理
  - 状态机
  - 通道复用
  - 面向协议与平台的原始系统指标采集
- `Protocol Adapters`
  - SSH
  - SFTP
  - FTP
  - Telnet (RFC 854 基础协商)
  - Serial (main process device handles)

## 4.1 系统信息能力边界

当前系统信息页先服务于 SSH 会话，但边界需要按“可扩展到多平台”来建立：

- `packages/core`
  - 只定义原始指标结构与 renderer 需要的通用字段，不承载中文或英文展示字符串。
- `main/services/sessions/*`
  - 负责按远端平台探测并采集原始指标，例如 Linux `/proc`、BusyBox 兼容命令、未来的 Windows PowerShell。
- `workspace-session-runtime`
  - 负责轮询、节流、网络历史合并、快照广播。
- `renderer/features/system/*`
  - 只做展示、本地化、布局与表格格式化，不承担平台分支判断。

现阶段的实现仍然偏 `Linux over SSH`，但后续扩展应保持：

```txt
platform probe
  -> raw metrics collection
    -> normalized core shape
      -> snapshot/runtime merge
        -> localized renderer presentation
```

## 4.2 Renderer 桌面壳与布局边界

桌面壳相关状态目前属于 renderer 本地 UI 状态，不进入 main service：

- 顶部标签栏、工作区焦点模式与标签切换动效由 `features/layout/TabBar.tsx`、`features/workspace/WorkspaceStage.tsx` 和 `App.tsx` 协作。
- SSH 工作区的文件面板高度、抽屉收起状态、终端悬浮输入条由 `features/workspace/SessionWorkspace.tsx` 与 `features/terminal/TerminalDock.tsx` 处理。
- 首页侧栏展开/收起、macOS 红绿灯避让和收起态左边界由 `features/workspace/HomeWorkspace.tsx` 与 `styles/features/home.css` 处理。
- 系统监控的展开态详情和收起态资源摘要都在 `features/system/SystemSidebar.tsx` 展示，不把平台判断或采集逻辑放进 renderer。
- 文件编辑器窗口继续通过 `main -> preload -> renderer` 打开，但编辑器内部的文件树、工具栏、状态栏和 Monaco 主题属于 renderer 组件状态。

平台差异的原则：

- renderer 从 preload 暴露的平台信息同步到 `document.documentElement.dataset.platform`。
- CSS 使用 `data-platform` 和稳定 class 做 macOS/Windows/Linux 差异化布局。
- macOS 菜单栏托盘图标使用 `apps/desktop/build/trayTemplate*.png` template 资源，由 main process 设置为 `setTemplateImage(true)`。
- 应用更新通过 `AppUpdateService` 统一管理，renderer 仅经 `main -> preload -> renderer` IPC 查询状态、触发检查、下载和重启安装；已打包的 Windows/macOS 客户端从 GitHub Release 读取 electron-builder 更新元数据。
- 主题和语言属于主进程持久化的 UI preferences；资源监控是 SSH 连接配置，关闭后该连接不采集资源数据，工作区仅保留窄侧栏。
- 产品更名后，main process 首次启动只迁移旧用户目录中的应用自有 JSON 数据；Chromium session、缓存与日志不迁移。

## 4.3 传输暂停与恢复边界

- `packages/core` 的 `TransferTask.tabId` 记录任务创建时所属的连接标签，renderer 据此隔离同一 profile 下的并行标签任务。
- 标签关闭后，旧任务仍按 profile 作为可重新打开的历史断点；仍然存在的其他连接标签任务不会串入当前标签。
- 主动暂停、标签关闭、连接断开、应用退出和重启恢复都只保留断点并进入 `paused`，不会自动续传；继续传输只能由用户显式触发。
- 单次传输取消通过 controller 调用参数中的 `AbortSignal` 传播。该信号只属于运行时，不进入 `TransferTask` 或传输日志。
- 真正退出应用时，main process 必须先等待活动任务停止并刷新 transfer journal，再放行窗口关闭和 `app.quit()`；macOS 隐藏窗口不触发 workspace shutdown。

## 4.4 SSH 终端与文件身份联动

- 远端 shell integration 在 prompt 上报真实 `cwd` 和 `id -un`，renderer 不解析命令文本、提示符或 `sudo` 输出。
- 每个 SSH controller 的首次用户上报是登录身份；后续终端用户变化会单向驱动文件访问身份，并在切换成功后按最新 cwd 重新跟随。
- 文件区手动切换 user/root 只改变独立的 SFTP/exec 文件通道，不向交互终端写命令；相同 shell 用户的重复 prompt 不会覆盖手动选择。
- 终端与文件通道不是同一远端进程。文件区进入特权身份仍需通过独立 exec channel 校验 sudo，优先复用终端输入期间已捕获的授权或远端免密 sudo。

## 5. 当前仓库结构

```txt
fileterm/
  AGENTS.md
  README.md
  package.json
  tsconfig.base.json
  docs/
    architecture.md
    roadmap.md
    plans/
      active/
      completed/
    decisions/
    quality/
  .agents/
    README.md
    extensions/
      README.md
  apps/
    desktop/
      index.html
      package.json
      tsconfig.json
      tsconfig.node.json
      vite.config.ts
      src/
        main/
          main.ts
          ipc.ts
          services/
            file-profile-repository.ts
            local-files-service.ts
            session-controllers.ts
            workspace-service.ts
        preload/
          preload.cts
        renderer/
          App.tsx
          i18n.ts
          main.tsx
          vite-env.d.ts
          components/
            TerminalView.tsx
          styles/
            app.css
            themes/
  packages/
    core/
      src/
        index.ts
    shared/
      src/
        index.ts
    storage/
      src/
        index.ts
```

## 6. 仓库结构

当前仓库采用 `npm workspaces` 的 monorepo 结构。协议 controller 仍放在 desktop main service 内，暂不拆成独立 `protocol-*` package：

```txt
fileterm/
  apps/
    desktop/
      src/
        main/
        preload/
        renderer/
  packages/
    core/
    storage/
    shared/
```

### 目录职责

- `apps/desktop`
  - Electron 主工程
  - 桌面窗口
  - preload
  - React 渲染层入口
- `packages/core`
  - 领域模型
  - Session interface
  - 文件操作统一抽象
  - 状态定义
- `apps/desktop/src/main/services/sessions`
  - SSH 连接
  - shell channel
  - SFTP client adapter
  - FTP 连接与文件操作 adapter
- `packages/storage`
  - profile repository
  - settings repository 预留
- `apps/desktop/src/main/services`
  - 文件型 profile 存储
  - profile repository
- `packages/shared`
  - types
  - constants
  - 轻量共享工具
- `apps/desktop/src/renderer`
  - 可复用 UI 组件
  - 终端、文件管理、连接管理、系统信息等 feature UI

## 7. 会话模型

### 7.1 Session Type

```ts
type SessionType = 'ssh' | 'ftp'
```

### 7.2 Profile

```ts
interface BaseProfile {
  id: string
  name: string
  host: string
  port: number
}

interface SshProfile extends BaseProfile {
  type: 'ssh'
  username: string
  authType: 'password' | 'privateKey'
  password?: string
  privateKeyPath?: string
  passphrase?: string
  sftpEnabled: boolean
}

interface FtpProfile extends BaseProfile {
  type: 'ftp'
  username: string
  password?: string
  secure: boolean
}

type ConnectionProfile = SshProfile | FtpProfile
```

### 7.3 Runtime Session

```ts
interface SshSession {
  id: string
  type: 'ssh'
  shell: ShellChannel
  sftp: SftpClient
}

interface FtpSession {
  id: string
  type: 'ftp'
  ftpClient: FtpClient
}
```

### 7.4 SSH shell cwd 跟随

SSH 终端与远程文件区通过真实 shell cwd 联动，不解析用户输入的 `cd` 文本：

```txt
shell integration
  -> OSC 7 cwd
    -> SSH controller cwd changed
      -> workspace runtime follow policy
        -> SessionSnapshot
          -> renderer
```

`SessionSnapshot` 中三个字段语义保持独立：

- `shellCwd`：远端交互式 shell 当前工作目录。
- `remotePath`：远程文件面板当前展示目录。
- `followShellCwd`：该 tab 是否允许 cwd 变化驱动文件面板。

shell 注入按 bash、zsh、fish、POSIX 风格策略选择；探测或注入失败时只降级目录跟随，不影响 SSH、SFTP 和终端输入输出。

### 7.5 Workspace Tab

```ts
interface WorkspaceTab {
  id: string
  sessionType: 'ssh' | 'ftp'
  profileId: string
  title: string
  layout: 'terminal-file' | 'file-only'
  status: 'idle' | 'connecting' | 'connected' | 'error' | 'closed'
}
```

## 8. 为什么 SSH/SFTP 与 FTP 必须拆开

这是第一版最重要的建模约束。

### SSH/SFTP

- 同一个认证上下文
- 同一个目标主机
- 终端与文件面板天然联动
- 未来可扩展端口转发、远端命令、路径跟随

### FTP

- 独立协议
- 无 shell
- 文件操作是完整主路径
- 未来即使支持 FTPS，也仍属于 FTP 家族，不应嫁接到 SSH 模型中

如果一开始为了“统一文件协议”硬揉成一个大 `RemoteSession`，后面会很快出现：

- 一堆条件分支
- 一堆无意义空能力
- 布局逻辑混乱
- 状态类型失真

所以这里必须拆。

## 9. IPC 与服务边界

前端不要直接接触协议客户端，所有能力都走服务边界。

### 建议 IPC 模块

- `profile`
  - 创建连接配置
  - 更新连接配置
  - 删除连接配置
  - 查询连接配置
- `session`
  - 打开会话
  - 关闭会话
  - 重连
  - 查询会话状态
- `terminal`
  - 输入
  - resize
  - 粘贴
  - 订阅输出
- `remoteFile`
  - list
  - stat
  - mkdir
  - rename
  - delete
  - upload
  - download
- `transfer`
  - 查询任务
  - 暂停任务
  - 继续任务
  - 丢弃任务与断点
  - 清理历史

### 9.1 高频事件边界

完整 `WorkspaceSnapshot` 只用于标签、会话、文件列表等低频结构状态，不承载高频流式更新：

- 传输层逐数据块累计真实字节数，main 最多每 200ms 发送一次轻量 `transfer:update` 任务事件；完成、失败和取消立即发送。
- Renderer 的传输订阅与列表状态收敛在独立 `TransferCenter`，进度变化不更新顶层 workspace state。
- 终端输入使用 renderer 到 main 的单向 IPC，避免交互等待请求响应队列。
- 终端 resize 同样使用单向 IPC；终端输出在 main 按 16ms 合并，再交给 renderer 逐帧写入 xterm。
- SSH transcript 由 controller 的有界分块缓冲统一维护，runtime 不重复拼接第二份历史。
- 系统指标首次绑定时发送完整历史，稳定轮询只发送最新样本，由 renderer 追加到固定长度历史。
- 同一 Renderer 的并发完整快照采用单飞和尾随合并，确保最终状态可达，同时避免重复磁盘读取与大对象序列化。

文件编辑器的 Monaco 资源通过动态 import 独立分块，只在文件编辑器窗口需要时加载，主工作区不静态携带编辑器与语言服务代码。

### 9.2 可恢复传输边界

- 单文件和目录 manifest 任务由 main process 持久化到 Electron `userData/transfer-journal.json`；renderer 不直接读写 journal。
- 上传和下载都先写入 `.fileterm-part` 临时文件，校验大小后再替换正式目标。
- SFTP 与 FTP/FTPS 分别在 controller 内实现 offset 读写和远端收尾，不把协议命令伪统一到 renderer 或 transfer UI。
- 传输调度使用 `profileId` 作为跨重启身份，不依赖生命周期短暂的 `tabId` 恢复连接；所有中断任务都等待用户手动继续，root 任务继续前还需要恢复 root 授权。
- 高频字节进度仍只走 `transfer:update`，journal 只在任务创建、状态切换和收尾时更新；恢复 offset 以实际临时文件大小为准。
- 普通断线和暂停保留临时文件；只有显式丢弃才清理断点。
- 本地最终替换采用可回滚的备份重命名。Windows 文件占用导致替换失败时保留 `.fileterm-part`，避免丢失已传数据。
- 目录任务持久化逐文件 manifest：已完成文件经过目标大小复核后跳过，当前文件按真实 `.fileterm-part` 长度继续。
- SFTP root 上传为每个任务持久化一个不可预测的 `/tmp` staging 路径；staging 始终保存源文件从 0 开始的连续前缀，暂停和重启后按实际长度续写，再由 sudo 只提交目标断点尚缺的后缀。普通 SFTP/FTP/FTPS 直接写目标同目录断点。
- FTP 上传优先使用 `APPE`，服务器不支持时回退 `REST + STOR`；回退结果不安全时删除断点并从零重传，避免拼接出等长但错误的文件。
- FTP 安全模式明确区分未加密 FTP、显式 FTPS 和隐式 FTPS。
- SFTP 可恢复路径保持有序流式写入。并行绝对 offset 会让文件长度无法证明前缀连续，因此在没有持久化范围位图前不用于断点判断。

## 10. UI 结构

### 10.1 桌面主布局

- 左侧：连接列表 / 收藏 / 分组
- 顶部：标签栏
- 主区：工作区视图
- 底部：传输任务面板

### 10.2 SSH/SFTP 工作区

- 上部：终端
- 下部：远程文件面板
- 支持拖动调整比例
- 支持切换终端全屏或文件全屏

### 10.3 FTP 工作区

- 仅文件面板
- 主区域全高使用

## 11. 主题系统

主题系统需要保持一条清晰链路：

```txt
tokens -> theme vars -> component skins -> terminal colors
```

约束：

- 新视觉能力优先进入 `styles/themes/` 的 token 和主题层。
- 终端、标签页、按钮、表格、面板这类共用外观，先判断是否需要补 theme vars。
- 不要把颜色、阴影、圆角散落在业务组件里。
- 终端配色从 CSS 变量读取，避免和全局主题脱节。

## 12. 存储设计

### 12.1 本地持久化对象

- 连接配置
- 分组
- 最近连接
- UI 设置
- 终端设置
- 传输历史

### 12.2 敏感信息

当前策略：

- 连接配置、密码、私钥均使用文件型 profile 存储，以 `profiles.json` 明文保存在应用用户数据目录下。
- 产品定位偏向个人/小团队的本地桌面工具，依赖操作系统对应用用户数据目录的权限隔离；不额外引入 Electron `safeStorage`、系统钥匙串或密文存储层，避免跨平台弹窗和钥匙串依赖破坏独立运行体验。
- 该策略是有意为之，不作为后续安全专项规划。如果未来产品定位转向多用户/企业场景，再重新评估存储安全模型，届时单独出 ADR 决策。

## 13. 状态管理

当前状态由 React state、7 个领域 hooks、workspace service snapshot 与 IPC 广播驱动。`App.tsx` 和 workspace service 的领域边界拆分完成后已重新评估 Zustand，结论是现有 hooks 编排足以满足当前共享范围，不引入额外 store。详见 [ADR-0004](./decisions/0004-retain-domain-hooks-without-zustand.md)。

只有在出现多个非父子 feature 需要直接订阅同一份高频可变状态、且现有 props/IPC snapshot 明显造成重复状态或性能问题时，才重新评估 store。届时按领域拆 store，而不是一个全局超级 store：

- `useWorkspaceStore`
- `useProfilesStore`
- `useSessionStore`
- `useTransferStore`
- `useSettingsStore`

避免重走旧桌面项目常见的“大 store 全塞”路线。

## 14. 当前重构热点

上一轮重构热点（`workspace-service.ts` 臃肿、`session-controllers.ts` 混合、App.tsx 上帝组件）已结束。拆分结果：

- `apps/desktop/src/main/services/workspace-service.ts`：已收敛为 façade，薄委托给 `WorkspaceTabsState`、`WorkspaceSessionRuntime`、`TransferService`、`WorkspaceTabLifecycleService` 与 `ProfileRepository`。
- `apps/desktop/src/main/services/session-controllers.ts`：已拆分为 `ssh-session-controller.ts` 与 `ftp-session-controller.ts`，SSH/SFTP 与 FTP 物理隔离。
- `apps/desktop/src/renderer/App.tsx`：已从 3898 行收敛至 1337 行，工作区逻辑拆出 8 个领域 hooks，由 `ModalPortalManager` 统一收口全局模态框。

当前阶段的关注重点已转移：

- **存储策略已明确**：连接配置、密码、私钥继续采用 `profiles.json` 明文文件型存储，不引入 `safeStorage`、系统钥匙串或密文存储层。详见第 12 节。该策略是有意为之，非待办债务。
- **系统指标多平台覆盖**：`apps/desktop/src/main/services/sessions/system-metrics/` 是新增活跃区域，重点在 POSIX/BusyBox/Windows 的 CRLF 归一化、平台探测与遥测注入门控。
- **controller 测试工程效率**：`test/controllers/` 当前依赖 `dist-electron/main` 产物，需先 build main 才能运行。后续应逐步支持 TypeScript 源码级测试，缩短反馈循环。
- **renderer 组件测试**：当前测试集中在 main/services 领域逻辑，UI 组件与交互暂无自动化覆盖，可作为下一步补充。

建议进入小步迭代阶段：不再做大规模结构拆分，而是围绕安全专项、平台扩展和测试厚度持续巩固。

## 15. 第一版不做的内容

这些能力在设计时可以预留接口，但不进入首轮交付：

- Telnet
- Serial
- RDP
- VNC
- 自动化脚本
- 团队同步
- 云备份
- AI 助手
- 手机端正式布局适配

## 16. 实施原则

- 所有新功能先落抽象，再接 UI
- IPC 必须有类型定义
- 不允许 renderer 直接操作协议客户端
- 文件传输必须走统一任务中心
- 错误处理必须以用户可读提示为目标
- 优先把 macOS 与 Windows 体验做顺
- 视觉样式优先遵循主题系统链路
