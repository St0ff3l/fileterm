# DSH 插件图标

DSH（DeepSeek Harness）的插件列表与详情页读取 bundle `package.json` 中导出的 `./icon` 作为图标。这里存放由 FileTerm 官方应用图标导出的插件图标，新增 FileTerm 相关 DSH 插件时直接复制，不要另画一版。

## 文件

| 文件               | 规格                                 | 来源                                                                                                      |
| ------------------ | ------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| `fileterm-512.png` | 512×512，8 位 sRGB RGBA，53,825 字节 | macOS Dock 主图 `apps/tauri/src/renderer/assets/app-icons/dock-classic.png`（`classic` 默认变体）等比缩放 |

图标保留 Dock 主图的 100px 透明安全边距（缩放到 512 后约 47–50px），自带圆角与四周留白；插件侧不要再补内边距，也不要把全出血的 `classic-1024.png` 当插件图标用，那会在圆角处露出金属外圈。

## 刷新

Dock 主图变更后按同样方式重新导出（Pillow，与 `apps/tauri/scripts/generate-platform-app-icons.py` 使用同一缩放与压缩参数）：

```sh
python3 - <<'PY'
from pathlib import Path
from PIL import Image

src = Path("apps/tauri/src/renderer/assets/app-icons/dock-classic.png")
out = Path("apps/tauri/assets/dsh-plugin-icons/fileterm-512.png")
icon = Image.open(src).convert("RGBA").resize((512, 512), Image.Resampling.LANCZOS)
icon.save(out, format="PNG", optimize=True, compress_level=9)
PY
```

## 用到插件里

把图标复制为 bundle 内的 `icon.png`，并在 `package.json` 声明导出与打包文件：

```json
{
  "exports": { "./icon": "./icon.png" },
  "files": ["cordis.patch.yml", "locale/*.json", "icon.png"]
}
```

bundle 还带详情文档或其他资源时，按实际文件把它们加进 `files`（例如 `docs`）。

```sh
cp apps/tauri/assets/dsh-plugin-icons/fileterm-512.png <bundle>/icon.png
```

DSH 对图标的要求：SVG/PNG/JPEG/WebP，不超过 256 KiB，必须位于包目录内；绝对路径、URL 和指向包外的软链都会被拒绝。本地目录（link）安装的 bundle 直接读取包内副本，主图更新后要重新复制一次。
