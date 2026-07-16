# 可恢复传输回归矩阵

## 自动化命令

```bash
npm run test:transfers -w @fileterm/electron
npm run test:transfers:protocol -w @fileterm/electron
npm run test:tauri
npm run typecheck
npm run build
git diff --check
```

## 协议矩阵

| 场景             | SFTP     | root SFTP      | FTP      | 显式 FTPS | 隐式 FTPS |
| ---------------- | -------- | -------------- | -------- | --------- | --------- |
| 单文件上传/下载  | 必测     | 必测           | 必测     | 必测      | 必测      |
| 暂停后继续       | 必测     | 必测           | 必测     | 必测      | 必测      |
| 连接中断后重试   | 必测     | 必测           | 必测     | 必测      | 必测      |
| 应用重启后继续   | 必测     | 重新授权后必测 | 必测     | 必测      | 必测      |
| 目录 manifest    | 必测     | 必测           | 必测     | 必测      | 必测      |
| 断点大于源文件   | 必须拒绝 | 必须拒绝       | 必须拒绝 | 必须拒绝  | 必须拒绝  |
| 最终 rename 失败 | 保留断点 | 保留断点       | 保留断点 | 保留断点  | 保留断点  |
| 传输时目录浏览   | 不阻塞   | 不阻塞         | 不阻塞   | 不阻塞    | 不阻塞    |
| 跨文件系统提交   | 不适用   | 两阶段提交必测 | 不适用   | 不适用    | 不适用    |

Electron 真实 socket 协议夹具位于 `apps/electron/test/protocol/`，包含本地 OpenSSH SFTP、FTP、显式 FTPS 和隐式 FTPS。受限环境缺少 sshd/openssl 或禁止监听 localhost 时相应用例显示 skipped；普通 macOS/Linux 开发机或 CI 中必须执行为 pass。

Tauri 的 Rust 回归位于 `apps/tauri/src-tauri/src/sessions/` 与 `services/transfers.rs`：本地 OpenSSH 覆盖认证、exec、SFTP 和代理链路，本地 FTP/FTPS socket 夹具覆盖明文及两类 TLS 数据通道，并分别断言 `APPE -> SIZE` 与 `APPE 失败 -> REST -> STOR -> SIZE` 的续传命令顺序；transfer 单测覆盖 root 旧 journal 到“两阶段 staging + partial”的迁移。发行候选还需在真实服务器上验证传输期间并发浏览、root 目标目录与 `/tmp` 跨文件系统、断线后重新授权继续三项场景。
