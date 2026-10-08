# Bundled font audit

Checked on 2026-10-08 against `main` commit `f1101846` and the working-tree
font packaging changes. This audit covers bundled font assets and their notices;
it is not a license audit of every application dependency.

## Inventory and production output

| Family                       | Files | Font metadata version         | License                            |
| ---------------------------- | ----: | ----------------------------- | ---------------------------------- |
| Outfit                       |     5 | 1.100                         | OFL 1.1                            |
| JetBrains Mono               |     4 | 2.211                         | OFL 1.1                            |
| Geist                        |     9 | 1.800                         | OFL 1.1                            |
| Hanken Grotesk               |    18 | 3.013                         | OFL 1.1                            |
| Inter                        |    18 | 4.001                         | OFL 1.1                            |
| Noto Sans SC                 |     1 | 2.004-H2                      | OFL 1.1                            |
| Material Symbols Outlined    |     7 | 2.952                         | Apache 2.0                         |
| Codicons, from Monaco Editor |     1 | Installed Monaco 0.53.0 asset | CC BY 4.0 artwork; MIT editor code |

The 64 checked-in TTF files all have local CSS references and appear byte-for-byte
in the production renderer output. Monaco adds one additional `codicon-*.ttf`.
All license files under `apps/tauri/public/licenses/` are copied unchanged into
`apps/tauri/dist/licenses/`. No remote font request is required.

`npm run build:renderer -w @fileterm/tauri` now runs
`apps/tauri/scripts/check-bundled-fonts.mjs` after Vite. It rejects missing or
changed font bytes, undeclared output fonts, missing CSS references, missing
licenses, stale Monaco notices, and differences between root and packaged
third-party notices.

All desktop release configurations inherit `frontendDist: ../dist` and the
renderer build from `tauri.conf.json`, so the same check runs for macOS Intel,
macOS ARM, Windows, and Linux packaging. This audit rebuilt the shared production
renderer; it did not rebuild or inspect each platform's final installer or any
already published release. Font files in Tauri's frontend assets are embedded
in the application; they need not appear as loose files beside the executable.

## Licensing findings

- The seven OFL families permit software bundling, with copyright and license
  retention. Each has a corresponding packaged `OFL.txt`; font metadata copyright
  entries agree with the declared authors. FileTerm's MIT license explicitly
  excludes these third-party fonts. Noto's Reserved Font Name notice is retained.
- Material Symbols permits redistribution under Apache 2.0. Static weight
  instances are identified in the notices, and the Apache license is packaged.
- Codicons was present in the build but absent from FileTerm's font notices.
  Added Microsoft/contributor attribution, the upstream CC BY 4.0 text, source
  and license links, and an explicit statement that FileTerm does not alter the
  Monaco-supplied font bytes. Also included Monaco's installed MIT license and
  `ThirdPartyNotices.txt` unchanged.
- Cascadia Code previously appeared as a preset without bundled files. Its official
  regular and italic variable TTFs are now bundled unchanged, with the upstream
  OFL and Reserved Font Name notice. The source archive and SHA-256 checksums are
  recorded beside the assets.
- SF Mono and SF Pro Text remain system-only fonts. Apple font files are not
  copied into FileTerm. Settings access them on
  macOS via generic system families: `ui-monospace` for SF Mono and `system-ui`
  for SF Pro Text. Apple presets remain selectable on macOS and are disabled on
  other platforms. Neither `local("SF Mono")` nor `document.fonts.check()` is a
  valid availability test for Apple's protected UI fonts: the former may fail
  even though system access works, and the latter accepts nonexistent names.
- Selected UI/code fonts always retain bundled Inter/JetBrains Mono and Noto Sans
  SC fallbacks. Imported family names are safely quoted, including commas and
  quotes. Legacy SF Mono selections use the macOS system generic entry too.
  Existing terminals remeasure and resize when the selected font changes.
- User-imported fonts are outside this bundled-asset inventory. Settings exposes one shared import action; importing adds a font to both selectors without changing the current UI or code font. Users apply it explicitly from the desired selector.

The checked-in static fonts do not have a complete reproducible generation
manifest. The hash check proves that FileTerm's packaging preserves those files,
not that every static instance is byte-identical to an upstream variable font.
The identified licenses allow redistribution of static instances under the
retained licenses; no additional font purchase or copyleft requirement was
identified for this inventory.

## Browser verification

`apps/tauri/tests/browser/bundled-fonts.mjs` loads every production CSS font face
in WebKit and Chrome. All 65 faces decode successfully without external network
requests. A nonexistent selected code font resolves to exactly the bundled
JetBrains Mono metrics. macOS SF Mono and SF Pro selections resolve to the same
metrics as `ui-monospace` and `system-ui`; they remain selectable even when direct
family lookup fails. Windows/Linux/browser disable these Apple presets. Imported
names containing commas/quotes remain valid CSS.

## Primary sources checked

- [Cascadia Code OFL](https://github.com/microsoft/cascadia-code/blob/v2407.24/LICENSE)
- [Apple font license](https://developer.apple.com/fonts/)
- [Outfit OFL](https://github.com/google/fonts/blob/main/ofl/outfit/OFL.txt)
- [JetBrains Mono OFL](https://github.com/JetBrains/JetBrainsMono/blob/master/OFL.txt)
- [Geist OFL](https://github.com/vercel/geist-font/blob/main/OFL.txt)
- [Hanken Grotesk OFL](https://github.com/google/fonts/blob/main/ofl/hankengrotesk/OFL.txt)
- [Inter OFL](https://github.com/rsms/inter/blob/master/LICENSE.txt)
- [Noto Sans SC OFL](https://github.com/google/fonts/blob/main/ofl/notosanssc/OFL.txt)
- [Material Symbols license](https://github.com/google/material-design-icons#license)
- [Codicons legal notices](https://github.com/microsoft/vscode-codicons#legal-notices)
- [Codicons CC BY 4.0 license](https://github.com/microsoft/vscode-codicons/blob/main/LICENSE)

Apple system generic access: https://webkit.org/blog/10247/new-webkit-features-in-safari-13-1/
