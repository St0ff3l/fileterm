# Windows renderer validation

最新日志修补和诊断 schema 3 说明见 [现场日志分析与修补](../terminal-log-analysis-repair-2026-10-21.md)。旧 #185 尚未在真实现场复现确认唯一原因，不标记为彻底解决。

> 2026-10-21 字形回归更新：用户真实会话发现先前小数网格候选存在断笔/拖影。已删除 WebGL 小数宽度覆盖，恢复上游整数像素图集，仅保留 backing resize 同步。52 组字体/DPI 与实际 WebView2 独立临时终端验证通过；初始已有会话回退 DOM，后续门禁触发开发版自动重启，详见记录。此前候选的网格一致和短测性能数据不能作为当前候选视觉或性能验收依据。详见 [字形回归修复](font-regression-fix-2026-10-21.md)。

> 2026-10-21 renderer 设置：Windows、macOS 和 Linux 均可在“设置 → 界面 → 终端显示”选择 DOM 或 WebGL。新安装及旧 `state.json` 缺少 renderer 字段时默认 WebGL；已保存选择会保留。新建终端采用所选项；加载失败、激活失败或 context loss 会回退 DOM。本文中的原生 WebGL 测量针对 Windows WebView2；真实 Claude Code 长任务和中文 IME 仍待交互验证。

This report covers two different test layers on PR #348 branch
`feat/renderer-performance-diagnostics`, source commit `e8c5932b49f7f937de17c70a559c4828e40489ba`:

- Independent Windows headless Chromium fixtures: 12 paired performance runs, 52 font/DPI cases and lifecycle fallback checks.
- A real FileTerm Tauri development window running Microsoft Edge WebView2 155.0.4283.62, tested on 2026-10-21 with a temporary in-memory xterm host.

The host was Windows 11 Pro 10.0.26300 x64, 20 logical CPUs and 39.8 GiB RAM. Windows reported NVIDIA GeForce RTX 3070 Laptop GPU, driver 32.0.16.1742 dated 2026-09-30. WebView2 reported NVIDIA ANGLE Direct3D11 and hardware acceleration enabled. Display scale was 125% (DPR 1.25), viewport 1280×820. The initial in-memory smoke used 2.3.1-beta.10; the current real-PTY soak uses a local beta11 debug build. The checkout includes uncommitted changes.

## Grid repair on 2026-10-21

The development WebGL renderer now uses the existing DOM logical grid without altering raw font measurements or applying the experimental DOM calibration. It also resynchronizes GL viewport and glyph uniforms when ResizeObserver changes canvas backing pixels; the previous viewport could crop the top rows after font/DPI changes.

The production adapter passed 52 font/DPI cases, late font loading, three resize widths, selection, search, replay and first/middle/last column hit checks. Each WebGL viewport is checked against its backing buffer. The 125% screenshot contains all five test lines after the repair.

Real WebView2 now matches a native DOM reference running the same font sequence inside the actual terminal-host → terminal-inner structure. Its 1228×716 CSS-pixel content area fits 183×49 initially and 144×37 at JetBrains Mono 14. After restoring the original font, both renderers fit 184×49; context-loss fallback retains that grid. This one-column round-trip change also occurs with native DOM/FitAddon and is retained. The terminal surface stays inside the padded host; all three disposal cycles release GL contexts. Results and source hashes are in the grid-fixed artifacts. Native IME, PTY/ACK and real Claude Code remain interactive checks.

The new production-grid Chromium comparison completed 12 paired runs. Normal 4,000-batch medians: DOM/WebGL 28.66/11.10 seconds, tree CPU 38.41/8.66 seconds, input-to-next-rAF p95 43.8/34.6ms. The 6× 200-batch medians were 18.29/5.80 seconds and 380.6/123.8ms. All batches were parsed and mock-ACKed, final backlog and context loss were zero. These are headless Chromium results, not WebView2 end-to-end measurements or a leak verdict. Raw data is terminal-renderer-production-comparison.json.

Earlier timing and memory tables below are retained as pre-repair data. The new WebView2 smoke includes the actual terminal-host / terminal-inner structure, immediate fit and native DOM reference hosts, so its timing is not a before/after benchmark.

## Artifacts

Generated `.json` results and `.log` captures are stored outside the checkout in the sibling
`CodeFiles` archive directory `../renderer-validation-archive/windows-2026-10-20/results/`.
Screenshots are in its `screenshots/` folder. The filenames below identify the archived files;
result JSON points to screenshots relative to each JSON file. Re-run artifacts use the same sibling
archive by default.

- `terminal-renderer-comparison.json`: headless paired measurements and per-second process, CPU, memory, parser/ACK and keyboard samples.
- `terminal-renderer-font-grid.json`: 52 headless font, DPI, resize, selection, search and replay cases.
- `terminal-webgl-lifecycle.json`: production lifecycle source bundled into Chromium; covers module-load failure, activation failure, context loss, fallback and disposal.
- `tauri-webview2-dom.json` and `tauri-webview2-webgl.json`: same smoke procedure attached to the real Tauri WebView2, each writes 1,500 synthetic rows with Chinese and emoji, checks selection/search/resize, measures the configured and JetBrains Mono 14 font grids, then creates and disposes three renderer instances.
- `tauri-webview2-dom-process.json` and `tauri-webview2-webgl-process.json`: 35 Windows process-tree samples per mode, including FileTerm, WebView2 browser, renderer, GPU and utility processes; working set, private bytes and cumulative CPU are recorded separately.
- `tauri-webview2-renderer-events.log`: filtered `terminal-renderer` app.log lines only. No full application log was copied.
- Matching screenshots in the same archive's `screenshots/` folder. Result JSON records each
  screenshot path relative to that JSON file.
- `beta11-react-workspace-soak.json` and its stdout/stderr: two-hour synthetic React workspace soak with DOM; `beta11-webgl-react-workspace-soak.json` and its stdout/stderr: the sequential two-hour WebGL run, including Chromium GPU and process-group samples. Summary and limitations are in the [DOM / WebGL comparison report](../terminal-dom-webgl-comparison-2026-10-20.md#windows-react-workspace-两小时-renderer-soak2026-10-21).
- `local-log-followup-summary.json`, `local-log-followup-filtered.log`, `log-followup-environment.json` and `log-followup-monitoring-{dom,webgl}.stdout.log` / `.stderr.log`: beta10/beta11 follow-up input/source hashes, gate results, local diagnostic excerpts and the DOM/WebGL query-response/React regressions. These are separate from the earlier overnight workload.

Repair artifacts: terminal-renderer-production-comparison.json, terminal-renderer-production-grid.json, terminal-renderer-production-grid-125pct.png, tauri-webview2-dom-grid-fixed.json, tauri-webview2-webgl-grid-fixed.json, both matching grid-fixed-process.json files, tauri-webview2-grid-fixed-environment.json, the filtered excerpts `tauri-webview2-dom-grid-fixed-renderer-events.log`, `tauri-webview2-grid-fixed-renderer-events.log`, and `tauri-webview2-font-fix-renderer-events.log`, plus terminal-webgl-lifecycle-grid-fixed.json.

The headless fixtures use Playwright 1.64.1, esbuild 0.28.2 and Chromium 156.0.8078.4. They are local test tools, not FileTerm dependencies. The font-grid calibration is confined to `tests/browser/helpers/terminal-grid-calibration.mjs`; it has not been applied to production.

## Real WebView2 smoke results before grid repair

The same smoke script passed in DOM and WebGL modes with no page or console errors. It confirmed 1,502 parsed rows, Chinese/emoji text, selected text `Header`, a successful search, resize from 188/194 columns to 94/97 and back, and three clean create/dispose cycles per renderer. WebGL context loss fell back to DOM; disposing WebGL instances released their contexts and left no host child nodes.

| Measurement                               |                         DOM |                         WebGL |
| ----------------------------------------- | --------------------------: | ----------------------------: |
| xterm write callback after 1,500 rows     |                     22.7 ms |                       34.9 ms |
| callback to next rAF                      |                       10 ms |                         28 ms |
| CDP script / layout / style / task delta  | 54 / 42.6 / 38.3 / 263.5 ms | 184.9 / 9.8 / 22.2 / 722.6 ms |
| JS heap at sample                         |                    32.9 MiB |                      26.4 MiB |
| 35-sample process-tree working set peak   |                   637.6 MiB |                     631.9 MiB |
| 35-sample process-tree private bytes peak |                   679.6 MiB |                     536.2 MiB |
| process-tree CPU delta during sample      |                      6.59 s |                        2.05 s |

These are single short synthetic runs, not a performance verdict. The write callback combines xterm parsing and callback scheduling; rAF is only a browser frame opportunity. CDP durations and process samples are not isolated GPU completion or stable memory measurements.

At the same 1,256 CSS-pixel host width and 125% display scale, WebGL fitted more columns:

| Font                          | DOM grid | WebGL grid | physical cell width, DOM / WebGL |
| ----------------------------- | -------: | ---------: | -------------------------------: |
| Current configured font, 12px |   188×51 |     194×51 |                     8.247 / 8 px |
| JetBrains Mono, 14px          |   147×38 |     155×38 |                     10.5 / 10 px |

This pre-repair result reproduced xterm WebGL 0.19's physical-pixel cell-width rounding. This earlier smoke did not use the actual inner host/padding, so its absolute column counts are historical. The production WebGL adapter now preserves the DOM grid, as verified above. The experimental DOM calibration helper remains separate.

The Tauri smoke uses an in-memory host created inside the actual WebView2. It does not create a FileTerm terminal session or exercise a native PTY, Rust IPC, real Windows IME, or a real Claude Code process. Native keystroke latency, PTY parse/ACK backlog, and IME composition are explicitly unavailable in both JSON results. Headless fixture ACKs are mocked and cannot fill those gaps.

## Re-run

From the repository root in PowerShell:

```powershell
$env:FILETERM_VALIDATION_ARCHIVE = Join-Path (Split-Path -Parent $PWD) 'renderer-validation-archive\windows-2026-10-20'
$env:FILETERM_VALIDATION_RESULTS = Join-Path $env:FILETERM_VALIDATION_ARCHIVE 'results'
$env:FILETERM_RENDERER_SCREENSHOT_DIR = Join-Path $env:FILETERM_VALIDATION_ARCHIVE 'screenshots'
New-Item -ItemType Directory -Force -Path $env:FILETERM_VALIDATION_RESULTS, $env:FILETERM_RENDERER_SCREENSHOT_DIR | Out-Null
$env:FILETERM_GRID_MODE = 'production'
$env:FILETERM_COMPARISON_OUTPUT = Join-Path $env:FILETERM_VALIDATION_RESULTS 'terminal-renderer-production-comparison.json'
Remove-Item Env:FILETERM_COMPARISON_REPEATS -ErrorAction SilentlyContinue
Remove-Item Env:FILETERM_COMPARISON_SLOWDOWNS -ErrorAction SilentlyContinue
Remove-Item Env:FILETERM_THROTTLED_BATCH_COUNT -ErrorAction SilentlyContinue
node apps/tauri/tests/browser/terminal-renderer-comparison.mjs

$env:FILETERM_GRID_OUTPUT = Join-Path $env:FILETERM_VALIDATION_RESULTS 'terminal-renderer-production-grid.json'
$env:FILETERM_GRID_SCREENSHOT = Join-Path $env:FILETERM_RENDERER_SCREENSHOT_DIR 'terminal-renderer-production-grid.png'
node apps/tauri/tests/browser/terminal-renderer-font-grid.mjs

$env:FILETERM_WEBGL_LIFECYCLE_OUTPUT = Join-Path $env:FILETERM_VALIDATION_RESULTS 'terminal-webgl-lifecycle-grid-fixed.json'
node apps/tauri/tests/browser/terminal-webgl-lifecycle.mjs
```

FILETERM_GRID_MODE=comparison selects the historical experiment-only DOM normalization. Production mode keeps native DOM and loads the new WebGL adapter. Do not combine the two modes' performance values.

For Tauri smoke, start the dev app with WebView2 remote debugging enabled, then attach the smoke script:

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9223'
npm run dev -w apps/tauri

# In FileTerm, select the renderer under Settings → Interface → Terminal Display, then open a new terminal.
# In another PowerShell window after the app opens, match the expected value to that selection:
$env:FILETERM_VALIDATION_ARCHIVE = Join-Path (Split-Path -Parent $PWD) 'renderer-validation-archive\windows-2026-10-20'
$env:FILETERM_VALIDATION_RESULTS = Join-Path $env:FILETERM_VALIDATION_ARCHIVE 'results'
$env:FILETERM_RENDERER_SCREENSHOT_DIR = Join-Path $env:FILETERM_VALIDATION_ARCHIVE 'screenshots'
New-Item -ItemType Directory -Force -Path $env:FILETERM_VALIDATION_RESULTS, $env:FILETERM_RENDERER_SCREENSHOT_DIR | Out-Null
$env:FILETERM_EXPECTED_RENDERER = 'dom' # or 'webgl'
$env:FILETERM_WEBVIEW2_OUTPUT = Join-Path $env:FILETERM_VALIDATION_RESULTS "tauri-webview2-$($env:FILETERM_EXPECTED_RENDERER).json"
node tests/local/run-tauri-webview2-smoke.mjs
```

The smoke script attaches to `127.0.0.1:9223`; the selected renderer applies to newly opened terminals.

## Development window snapshot (2026-10-21 02:06 SGT)

This is a historical capture, not a live-process claim. A later process check found no `fileterm.exe` or
Vite process from this checkout. The last renderer sequence in FileTerm's app.log at
12:08 SGT shows WebGL activated on the same local terminal twice, then falling back to DOM with
`block-glyph-compatibility`; it does not establish the renderer of any currently open window.

At capture time, the WebGL Tauri development window was PID 45964 from
`apps/tauri/src-tauri/target/debug/fileterm.exe`, version 2.3.1-beta.11. It started at 2026-10-21 02:06
from this checkout at e8c5932b49f7f937de17c70a559c4828e40489ba plus uncommitted changes. Vite is on
port 5188; the WebView2 debugging listener remains on localhost:9223. The window has two real local
PowerShell PTYs: `local-b086bb78-bf0e-4b27-9054-dc548d9a07ec` is DOM after context loss, and
`local-435a92ad-5840-45d2-b7af-c309742caa95` is active WebGL. Both have passed mixed Chinese, punctuation,
English, digits, accented Latin, Han glyph and emoji output plus terminal search. Context loss briefly
showed a white frame before the same session's DOM rows recovered; transition duration was not measured.

A six-hour no-output idle window was scheduled on WebGL PTY `local-435a92ad-5840-45d2-b7af-c309742caa95`
from 2026-10-21 02:21:37 to 08:21:37 +08:00. The target remained WebGL in all 356 renderer-health
samples; the second terminal also became WebGL during the run. There were 359 complete native samples
covering FileTerm and all six registered WebView2 processes. No parse/ACK failures, renderer errors,
React #185 / maximum-update-depth signatures, context loss or DOM fallback were found in the app.log
segment. Parse backlog stayed at zero; pending ACKs peaked at one and ACK elapsed time at 202 ms.

The process-group private bytes ranged from 397.0 to 766.1 MiB and ended the scheduled window at
495.1 MiB; working set ranged from 606.7 to 910.5 MiB and ended at 682.2 MiB. CPU averaged 1.60% of
one logical core (p95 1.80%, peak 25.05%). JS heap ranged from 53.0 to 89.2 MiB and was 73.5 MiB at
the end; the canvas backing estimate rose from 10.1 to 20.2 MiB as both terminals used WebGL. These
samples show a transient peak followed by a lower plateau, not proof that a leak is fixed. One frame
warning reported 3.879 s (health maximum 4.407 s); it had no script attribution or xterm work and no
parse/ACK backlog. Its cause is unknown. Only two qualifying input samples were collected, so this
idle run does not establish interactive latency.

The expected six-hour time elapsed, but the shell prompt/return itself was not observable: both
PowerShell child processes remained alive, and app.log does not record terminal prompts. The current
WebGL canvas has no readable xterm row text in the DOM. Therefore the scheduled observation window is
complete, while `Start-Sleep` completion is not independently verified. The running debug app remains
PID 45964 at beta11; the latest post-window samples at 08:40 showed two WebGL terminals, 72.8 MiB JS
heap, 20.2 MiB canvas estimate and 495.6/683.6 MiB private bytes/working set. No Claude Code task ran.

The filtered records and derived metrics are saved in `beta11-overnight-filtered.log` and
`beta11-overnight-summary.json`; source app.log paths and byte-offset handling are recorded in
`beta11-overnight-environment.json`. WebView2 was 155.0.4283.62 on Chromium 155, NVIDIA RTX 3070 Laptop
GPU / driver 32.0.16.1742, ANGLE D3D11 with hardware acceleration. Dedicated GPU memory was not
available; the GPU process is included in the WebView2 process-group total. ETW scheduling remained
unavailable with privilege error 1300.
The Claude Code 2.1.296 attempt did not meet the low-token one-command goal: the selected Custom Haiku
mapped to `deepseek-flash`, used about 2.4k tokens in roughly 22 seconds and launched 12 shell commands.
That session and its child processes were stopped. No Claude Code long-run pass is claimed; the current
idle soak is a direct local PowerShell sleep with no model calls.

## Remaining interactive validation

The six-hour app.log segment is captured from byte offset 1934114 in
`beta11-overnight-filtered.log`; its metrics and coverage caveats are in `beta11-overnight-summary.json`.
Do not call a stable RSS/private-byte sample proof that a leak is fixed.
Before recommending WebGL, repeat in the same window with matched local terminal output on both the DOM
fallback and active WebGL PTY. Keep font, scale (125%), theme, viewport and output fixed. Check font increase,
decrease and reset; maximize/restore and resize; CJK, emoji and box drawing; Chinese IME composition commit
and cancel; selection/copy; `Ctrl+Shift+F`; and tab-away/tab-back replay. Physical IME and selection checks
remain outstanding. A renderer-mode app restart disconnects local PTYs, so recreate paired local sessions
and replay the identical output instead of claiming one local shell survived that restart.

Do not retry Claude Code with automatic multi-tool access for this soak. The local CLI attempt used about
2.4k tokens and launched 12 shell commands before being stopped. A real Claude Code task still needs a
separately controlled, user-visible interactive run with observed token usage. Production default remains DOM.

The 6× headless CPU throttle is not Windows efficiency mode. A browser rAF is not a GPU completion timestamp. A short post-dispose memory sample cannot establish whether a long-run leak is fixed. Production default remains DOM pending the real input, grid and long-run evidence.
