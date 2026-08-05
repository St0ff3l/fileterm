## FileTerm 2.1.6

FileTerm 2.1.6 聚焦 SSH/SFTP 权限切换后的文件管理稳定性、文件面板筛选体验，以及跨平台工作区和设置界面细节优化。

### 2.1.6 更新重点

- SSH/SFTP root 文件访问同步：终端执行 `su -` 或 `sudo -i` 后，自动同步 root 身份与当前目录，浏览、读取、写入、上传、下载、移动、删除和权限修改使用匹配的 root 执行链路，避免文件传输完成但目标文件未落地。
- root shell 与错误反馈：修复权限切换后的重复终端提示符，完善 CWD/用户状态同步，并将 root 文件操作失败原因显示在顶部横幅和传输详情中。
- 文件面板筛选：支持普通文本、glob 通配符和正则表达式筛选，统一两个文件面板的输入、焦点和空状态表现。
- 设置与工作区布局：限制设置窗口高度并拆分 WebDAV/S3 子选项卡，继续优化文件表格滚动条、主题边框、终端边框和工作区分栏对齐。
- 质量门禁：继续执行 Tauri/Rust 格式、类型、协议夹具、跨平台 socket 生命周期和依赖安全检查。

### 本版本包含的主要 PR

- [PR #171](https://github.com/St0ff3l/fileterm/pull/171)：修复 `su -`/`sudo -i` 后 root 文件访问同步，并加入文件面板筛选能力。
- [PR #169](https://github.com/St0ff3l/fileterm/pull/169)：限制设置模态框高度并拆分 WebDAV/S3 子选项卡。
- [PR #165](https://github.com/St0ff3l/fileterm/pull/165)：优化跨平台文件表格、主题边框、滚动条和工作区布局。

> 遇到问题请前往 [GitHub Issues](https://github.com/St0ff3l/fileterm/issues) 提交反馈，并附上操作系统、FileTerm 版本、连接类型、复现步骤和脱敏日志；不要提交密码、私钥或 token。

> 也可以加入微信群交流：请打开仓库 [README 的“社区交流”部分](https://github.com/St0ff3l/fileterm#%E7%A4%BE%E5%8C%BA%E4%BA%A4%E6%B5%81) 扫描二维码进微信群，也可加入 QQ 群 534418986。
