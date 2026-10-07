# 单一 DEB 的 Debian 11 兼容

## 用户目标

Debian 11 与较新系统的 x86_64 用户下载同一个标准名称 `.deb` 后用 APT 安装，无需选择专用包、升级系统或混装另一发行版的系统库。

## 验证路线

- 只生成一个标准 Linux DEB，在包内同时提供未修改的 native 程序和私有运行库 fallback。
- 启动器用系统 ELF loader 的 `--list` 检查实际依赖及符号版本；检查通过才启动 native 程序，否则启动私有运行库版本。判断不依赖发行版名称或版本号。
- 在隔离的 Ubuntu 22.04 amd64 容器中准备运行库，将其放入 `/usr/lib/fileterm/runtime` 私有路径；包内保留原生二进制，启动器检查系统 loader 是否能解析全部依赖与版本符号，再自动选择原生或兼容运行时。
- 不替换宿主 `/lib`、`/usr/lib` 中的库；本地终端和外部浏览器必须继续使用宿主环境。
- 制作原型，在干净的 Debian 11 用户空间中安装，验证 CLI、WebKit 页面、托盘容错、链接打开和本地 shell。
- 若原型通过，再维护可重复构建脚本、依赖清单与许可证资源、兼容测试以及发布产物收集。

## 验收边界

仅改包元数据、安装成功或 CLI 能运行均不足以宣布桌面兼容。必须验证 WebKit 子进程与实际页面加载；WSL 内的 Debian 11 用户空间验证也不替代 Debian 11 实机的显卡、桌面、托盘与串口回归。

## beta8 实现与隔离验证（2026-10-07）

- 在 WSL 中用官方 debootstrap 建立 Ubuntu 22.04 与 Debian 11 独立用户空间。
- 以 beta.7 amd64 DEB 为原型输入；beta8 的正式流水线会将新构建的 amd64 DEB 替换为通用包。
- 私有运行库放入 `/usr/lib/fileterm/runtime`；fallback 应用与 helpers 的 ELF interpreter / RPATH 指向该目录，没有全局设置 `LD_LIBRARY_PATH`。未修改的 native 程序放入 `/usr/lib/fileterm/fileterm`。
- WebKit helpers、Mesa DRI 和 EGL vendor 目录需要同步重定位；只复制 `.so` 不足以加载页面。原型使用保持字节长度的路径替换，不关闭 WebKit sandbox。
- Debian 11（11.11，glibc 2.31）用户空间中 CLI、完整 WebKit 主页面、真实本地 PTY 均成功。终端里的 `getconf GNU_LIBC_VERSION` 返回 `glibc 2.31`，进程为宿主 `/usr/bin/bash`。
- 点击 GitHub 入口可调用宿主 `xdg-open` 选出的测试浏览器启动器；启动器同样返回 `glibc 2.31`，未继承 `LD_LIBRARY_PATH`。这不等于已经验证真实 Firefox 的完整网页加载。
- 可复用打包脚本：`apps/tauri/scripts/build-universal-deb.py`。保留标准 DEB 文件名、应用身份与 desktop entries，并加入运行库版本清单和版权文件。
- 同一个 DEB 经 APT 安装到 Debian 11 和 Ubuntu 22.04：前者自动执行 `/usr/lib/fileterm/runtime/fileterm`，后者自动执行 `/usr/lib/fileterm/fileterm`，两条 CLI 路径均正常。系统 glibc 分别仍为 2.31 和 2.35；Debian 11 系统 libc 文件哈希保持不变。

## 打包与发布验证

在**隔离 Ubuntu 22.04 amd64 环境**中执行；不要在用户日常系统中安装另一发行版的库：

```sh
apt-get update
apt-get install -y --no-install-recommends \
  libwebkit2gtk-4.1-0 libayatana-appindicator3-1 libssl3 \
  libgl1-mesa-dri patchelf python3
python3 build-universal-deb.py ./FileTerm-input.deb ./artifacts
```

GitHub Release workflow 在 x86_64 Linux 构建后，用独立 Ubuntu 22.04 容器生成通用包，并在 Debian 11 容器执行安装与 CLI 冒烟测试。桌面 WebKit 页面、本地 PTY 和外部链接另外在隔离 Debian 11 用户空间验证。桌面测试需要提供 X server、会话 DBus、`/proc`、`/sys`、`/dev`，并以非 root 用户运行。此前测试中缺少 `/sys` 会触发串口枚举异常，缺少 EGL vendor 数据会导致 WebKit 图形进程终止；这些属于测试环境和私有运行库布局问题，不能据此绕过运行库验收。

## 发布前仍需完成

- 对实际 GitHub Release 生成包核对体积和依赖文件集；私有 GTK/WebKit 运行库会随 FileTerm 的 `.deb` 更新，不单独从系统源更新。
- Debian 11 实机验证 GPU、桌面、托盘、串口及真实默认浏览器。
- 验证 beta8 发布流水线能把通用 x86_64 `.deb` 和移除私有运行库后的 Arch 包正确上传；arm64 `.deb` 继续使用原生系统依赖并以 Debian 12 为最低基线。
