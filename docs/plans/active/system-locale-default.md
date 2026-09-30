# System locale as the initial UI language

## Context

AppImage catalog PR #9146 asks FileTerm to show English by default outside Chinese locales. The renderer has locale detection, but Tauri returns `zhCN` for new installations and the HTML bootstrap also selects Chinese before preferences load.

## Changes

- Resolve the first UI preference from the operating system locale: Chinese for `zh` locales, English otherwise.
- Keep saved language choices authoritative and align renderer bootstrap, translation state, and the HTML `lang` attribute.
- Show the English README on the repository home page and retain a linked Chinese translation.
- Correct the Linux AppImage launcher permissions after bundling. Catalog PR #9146 fails because `AppRun.wrapped` has mode `770` in the published v2.2.16 image.

## Acceptance

- New installations use Chinese in a Chinese locale and English elsewhere.
- Existing saved language choices remain in effect, including native menus and the tray.
- GitHub shows English by default and keeps Chinese one click away.
- Linux CI and release packaging require `AppRun` and `AppRun.wrapped` to be executable by all users.
- FileTerm quality gates and the CSS contract check pass.
