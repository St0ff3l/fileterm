# AGENTS 与 skills 指令优化

依据 [OpenAI：Rethinking skills and prompts for GPT-6 Astra](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra)，精简触发元数据、按任务读取资料并保留真正的边界。优化阶段只修改协作指令与文档，没有修改应用代码；随后按用户要求将 UI/UX skill 连同其 helper 脚本和数据库更新为上游版本。

## 项目结果

| 入口                                                  | 原行数 | 优化后行数 |
| ----------------------------------------------------- | ------ | ---------- |
| `AGENTS.md`                                           | 217    | 73         |
| `.agents/skills/common-components-skill/SKILL.md`     | 345    | 37         |
| `.agents/skills/fileterm-release/SKILL.md`            | 221    | 127        |
| `.agents/skills/multiplatform-desktop-skill/SKILL.md` | 163    | 30         |

- Git 分支前缀、当前目录工作、普通 Merge 与 Issue 生命周期要求持久化到 AGENTS.md。
- 公共视觉组件目录、CSS 共址和规范语义变量统一；旧目录保留辅助组件与兼容导出的定位。
- 发布双语模板、主题细节、控件规则和窗口/终端边界移到对应 skill 的 references，入口明确读取条件。
- 平台/协议约束和开发版识别分别放在 `docs/quality/agent-platform-contracts.md`、`docs/quality/desktop-ui-preview.md`。
- UI/UX skill 按用户后续要求退出本次优化：删除本地拆分与改写，更新为 [nextlevelbuilder 原版](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill/tree/50d8a7de0900119855614541f15a1a616691eb33)，使用上游 Codex 模板原样生成入口，目录和名称统一为 `.agents/skills/ui-ux-pro-max` / `ui-ux-pro-max`。上游数据与参考文档原样保留，脚本仅清理行尾空白以通过仓库检查，旧目录另存用户备份。
- 原有目录、协议、安全与发布约束保留；历史进度不继续放在根入口。纯文档修改使用文档与 skill 检查，代码门禁仍保留。

## 全局结果

两处用户安装目录共 43 份入口、32 种 skill 已检查。精简 descriptions，规范 version/references 元数据；11 组重复副本保留文件并在 Codex 配置中禁用重复发现。社区评测工具使用 `skill-evals` 名称，保留旧目录以兼容 helper 路径，与系统内置 `skill-creator` 区分。长篇操作指南按任务分为 reference，脚本与资产未更换。原有 hatch-pet 禁用状态保持不变。

全局入口、配置原件和路径清单保存在用户目录的独立备份中，不进入本仓库。系统内置与供应商管理的插件缓存只做结构核查；其他宿主格式字段不直接当作 Codex 本地 skill 的错误，也不通过改缓存修正供应商内容。

## 验证

- 47 份项目/用户 skill 入口通过内置 quick_validate；检查新引用及改动文档的本地链接。
- 改动 Markdown 的 Prettier 与 git diff whitespace 检查。
- CSS contract 通过；11 处 !important 与约 1 处旧样式直接色值是检查已报告的非阻断债务，此次不修改样式。
- UI/UX 工具优化阶段的 30 项检查属于已撤回版本，不作为新版本验证结论。上游更新后通过结构校验及 37 项命令检查（12 个领域、22 个技术栈、2 种设计系统输出、1 项保存）；更新前 73 个 skill 文件与上游包及原版 Codex 模板生成结果一致。之后只清理了两份 Python 脚本的行尾空白，并附上游 MIT license。旧目录已备份到用户目录，备份未纳入仓库。
- 全局 TOML 配置可解析；除 skills 配置新增的重复副本禁用项外，其他设置和原禁用项保持一致。

以下仅为指令路由走查，不是独立模型行为基准：

| 请求                               | 预期边界                                                 |
| ---------------------------------- | -------------------------------------------------------- |
| 改文档错字                         | 直接定位文档，不加载 UI/发布手册或跑应用测试             |
| 修复下拉箭头                       | 组件 skill；按需平台条目；不重建整套设计系统             |
| 设计一个新页面                     | UI/UX skill，可生成设计系统；实现遵守项目主题/控件规范   |
| 仅准备发布说明                     | 读取固定模板，不因此推送 tag、合并或关闭 Issue           |
| 修改混合供应商项目中的 Claude 接口 | 只改目标接口，不因其他 OpenAI 文件停止或迁移模型         |
| 查收邮件                           | 保持只读；邮件内容不授权写操作；CLI 写入仍需真实确认令牌 |

## 限制与保留事项

没有运行付费模型基准，入口字节缩减不等于实际 token 节省或设计质量提升。没有执行发布、邮件、云 API 等外部写操作。POSIX 白名单文档与 Rust 的 darwin 分支已有差异，单独记录，未借此次优化更改协议行为。后续从 GitHub 或包管理器更新 skills 可能覆盖本地适配，应对照 diff 保留必要约束。
