# 网络端口与连接详情（Refs #393）

## 目标与交互

点击系统侧栏的网络速率或曲线打开“网络连接 · 主机名”独立标签，重复点击复用该会话的已有标签；网卡下拉框仍独立切换网卡。标签复用 system host-tool 生命周期，参与主机分组、UI 状态恢复及来源会话关闭清理。详情显示远端整机的 TCP/UDP 监听端口，选择端口查看连接；提供全部 socket、筛选及排序。共享现有 Input、DropdownSelect 与 VerticalScrollbar，长列表虚拟化。

## 数据边界

- `app_get_network_connections` 固定只读命令 → 现有 SSH worker 独立 exec → Rust parser → core 类型 → Tauri bridge → renderer。
- Linux 每次采集优先查找 PATH 中的 `ss`，再检查 `/opt/bin/ss`、`/opt/sbin/ss` 的可执行文件，安装后无需修改 PATH 或重连，刷新即可重新检测。使用兼容旧版 Entware 的 `ss -n -a -t -u -p -i`，解析表头及缩进的 TCP 统计续行；缺失时回退 `netstat -antup`（精简版不支持进程参数时再用 `netstat -antu`）；8 秒超时、既有 256 KiB 输出上限、最多 2000 个 socket，并显示截断状态。只在网络标签激活时每 3 秒串行刷新；离开、关闭或 SSH 断线停止后续采集，失败等待用户刷新。
- 不提权、不安装远端代理、不解析交互终端；当前 SSH 用户看不到的进程信息显示不可用。
- 上传/下载是当前活动 TCP socket 内核累计计数，不是网卡总量、历史流量或抓包流量；UDP 与内核未提供的字段显示 —。
- 此次支持 Linux 远端；其他远端系统明确显示不支持。数据为整机视图，不把网卡选择器当成进程 socket 过滤器。

## 开源参考

- [psutil Linux network connections](https://github.com/giampaolo/psutil/blob/master/psutil/_pslinux.py)：区分 socket 地址、状态、PID，并保留权限受限进程的未知身份。借鉴数据模型，未复制源码或新增依赖。
- [Sniffnet](https://github.com/GyulyVGC/sniffnet/wiki/Required-dependencies)：抓包需要额外依赖/权限，本功能采用系统 socket 统计，不植入抓包代理。

## 验证

- `npm run typecheck`、`npm run lint`、全量 Prettier 检查、CSS 契约、`cargo clippy --locked --all-targets --all-features -- -D warnings` 通过。
- Renderer 286 项通过；Rust 1001 个单元测试、10 个 CLI 测试、24 个 runtime contract 测试通过，1 个既有交互 OTP fixture 忽略。
- 新增 Rust parser 测试覆盖 IPv4/IPv6、UDP、权限受限 owner、空结果、截断及 BusyBox/netstat 输出；renderer 测试覆盖端口关联、未知计数、不同远端连接身份、网络标签创建/复用、恢复及主机分组。
- Playwright 基于当前 Vite 源码与模拟 socket 快照检查卡片入口、非模态工作区、端口明细、流量计数、600+ 行虚拟化、筛选、卸载停止采集、四个主题、缺少采集工具与失败不自动重试。
- 未验证真实群晖/远端 Linux `ss` 和 `netstat` 命令；未验证 Tauri 开发窗口、Windows/Linux 桌面 UI。浏览器模拟数据不替代真实 SSH 验收。

## 刷新与视觉修正

- 自动轮询静默更新，初次加载和手动刷新才显示 busy，复用公共 360ms 最短反馈；刷新按钮最小宽度 82px、预留 loading 空间，深色普通/hover/忙碌态保持 RGB(48,48,48)，工具栏四个控件统一 36px 高和 4px 圆角。
- 表格默认列宽等分、无首列特殊加宽，使用 `table-layout: fixed`，基于 socket 身份生成稳定 key，避免刷新时表头跳动或行 DOM 重建；与文件区域一致采用 24px 行高、11px 等宽正文、10px/500 表头和细分隔线。表格外框圆角统一为 4px。
- IPv4/IPv6 通配监听地址显示为“所有 IPv4/IPv6 地址”；详情标题将地址、端口、协议分开显示。`netstat` 快照带来源标记，明确提示安装 `ss`（iproute2）以获取内核支持的连接流量。
- Playwright 检查连续两轮自动刷新无按钮 loading 切换、动态进程名不改变列宽、相同行 DOM 保持、手动刷新按钮宽度固定、24px 行高、安装提示、通配地址说明，以及明暗/紧凑布局；分别测量自绘与 macOS 原生下拉框的工具栏高度、上边缘与 4px 圆角。

## 主机信息页面一致性与列宽调整

- 网络和系统信息标题共用局部样式，标题说明到内容的实际间距统一为 18px。系统信息样式从旧全局皮肤迁移到 feature 共址文件，保留明暗、自定义主题变量链路。
- 两页表头/内容底色分层，正文统一主文本色；概览左右两列有中央 1px 竖线、配对行斑马纹、固定 24px 行高和末行占位，单列模式自动隐藏占位与竖线。
- 表头、数据行、外框只各承担一条分隔边；虚拟占位行不绘边框，最后数据行不叠加底部外框。网络行点击后持续高亮并更新详情，端口内容左对齐。上下两张表所有列支持表头升降序排序和方向箭头，排序不改变已选中的 socket；空状态与虚拟占位行不触发 hover。
- 长文字复用侧栏 `MetricHoverDetail`，只在溢出时显示同款全文提示，不再用原生 title。
- 新公共 `TableColumnResizer` 供文件、网络、系统信息表格复用：跨分隔线两侧各 6px（共 12px）的命中区，与 10px 单元格文字内边距保留至少 4px 空隙、1px 可视指示、pointer capture、最小列宽 50px、键盘方向键 10px（Shift 为 20px）、结束/取消/卸载恢复鼠标状态。默认等分的新表格仅在用户拖动后固定像素列宽，刷新不重置；文件表格保留原初始宽度和重置入口，拖动不触发排序。
- 浏览器在四个内置主题与窄窗口验证相同标题间距、表头/内容色区分、默认等宽列、单层边框、概览中央线/配对斑马纹/末行补齐、全文提示和正文颜色一致。另在 `.fs-shell` 环境验证 82×36px 按钮、360ms 最短反馈、端口左对齐；文件/网络/系统列从边界两侧 5px 处均可启动拖动，文字起点不会触发拖宽，+60px 拖动与 +10px 键盘调整保持其他列宽不变，刷新不重置、文件排序不误触发。

网络采集通过固定用途 worker 请求，复用当前已启用的 root 文件访问模式；旧版 ss 的上传字段回退和权限边界见 [权限与旧版流量兼容](network-process-permissions-and-legacy-traffic.md)。
