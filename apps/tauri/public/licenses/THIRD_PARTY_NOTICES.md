# Third-party notices

FileTerm is open-sourced under the MIT License in the repository root
(`LICENSE`). That MIT License applies to FileTerm's own source code only.
The bundled fonts and icon fonts below remain under their own licenses.

## Bundled fonts

### Outfit

- Files: `apps/tauri/src/renderer/assets/fonts/text/outfit/*`
- Copyright: 2021 The Outfit Project Authors
- License: SIL Open Font License 1.1
- Source: https://github.com/Outfitio/Outfit-Fonts
- Packaged license: `licenses/outfit/OFL.txt`

### JetBrains Mono

- Files: `apps/tauri/src/renderer/assets/fonts/text/jetbrains-mono/*`
- Copyright: 2020 The JetBrains Mono Project Authors
- License: SIL Open Font License 1.1
- Source: https://github.com/JetBrains/JetBrainsMono
- Packaged license: `licenses/jetbrains-mono/OFL.txt`

### Geist

- Files: `apps/tauri/src/renderer/assets/fonts/text/geist/*`
- Copyright: 2024 The Geist Project Authors
- License: SIL Open Font License 1.1
- Source: https://github.com/vercel/geist-font
- Packaged license: `licenses/geist/OFL.txt`

### Hanken Grotesk

- Files: `apps/tauri/src/renderer/assets/fonts/text/hanken-grotesk/*`
- Copyright: 2021 The Hanken Grotesk Project Authors
- License: SIL Open Font License 1.1
- Source: https://github.com/marcologous/hanken-grotesk
- Packaged license: `licenses/hanken-grotesk/OFL.txt`

### Inter

- Files: `apps/tauri/src/renderer/assets/fonts/text/inter/*`
- Copyright: 2016 The Inter Project Authors
- License: SIL Open Font License 1.1
- Source: https://github.com/rsms/inter
- Packaged license: `licenses/inter/OFL.txt`

### Noto Sans SC

- Files: `apps/tauri/src/renderer/assets/fonts/text/noto-sans-sc/*`
- Copyright: 2014-2021 Adobe, with Reserved Font Name "Source"
- License: SIL Open Font License 1.1
- Source: https://github.com/google/fonts/tree/main/ofl/notosanssc
- Packaged license: `licenses/noto-sans-sc/OFL.txt`

## Bundled icon font

### Material Symbols Outlined

- Files: `apps/tauri/src/renderer/assets/fonts/material-symbols/*`
- Copyright: Google LLC
- License: Apache License 2.0
- Source: https://github.com/google/material-design-icons
- Packaged license: `licenses/material-symbols/LICENSE.txt`

System fallback fonts such as SF Pro Text, PingFang SC, Microsoft YaHei,
Segoe UI, SF Mono, Menlo, and Consolas are not bundled by FileTerm and remain
subject to their platform owners' terms.

The same notice is copied to the packaged application's
`licenses/THIRD_PARTY_NOTICES.md` file.

## Vendored russh compatibility fork

- Files: `vendor/russh/*`
- Upstream: [https://github.com/warp-tech/russh](https://github.com/warp-tech/russh)
- License: Apache License 2.0
- Modification: adds an explicit opt-in client GEX constructor that permits
  the 1024-bit minimum needed by older Comware SSH peers, and selects the
  `1024/1024/8192` request only for matching Comware banners when FileTerm's
  legacy SSH compatibility is enabled; secure/default client validation and
  behavior remain unchanged.
