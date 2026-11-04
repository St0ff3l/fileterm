/* global window, document, location, requestAnimationFrame */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { mkdirSync, writeFileSync } from 'node:fs'
import { basename, dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const cdpUrl = process.env.FILETERM_WEBVIEW2_CDP_URL || 'http://127.0.0.1:9223'
const expectedRenderer = process.env.FILETERM_EXPECTED_RENDERER || 'webgl'
assert.ok(['dom', 'webgl'].includes(expectedRenderer), 'FILETERM_EXPECTED_RENDERER must be dom or webgl')

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const archiveRoot = resolve(
  process.env.FILETERM_VALIDATION_ARCHIVE ||
    resolve(projectRoot, '..', 'renderer-validation-archive', 'windows-2026-10-20')
)
const resultsDirectory = resolve(process.env.FILETERM_VALIDATION_RESULTS || join(archiveRoot, 'results'))
const outputPath = resolve(
  process.env.FILETERM_WEBVIEW2_OUTPUT || join(resultsDirectory, `tauri-webview2-${expectedRenderer}.json`)
)
const screenshotDirectory = resolve(process.env.FILETERM_RENDERER_SCREENSHOT_DIR || join(archiveRoot, 'screenshots'))
const outputName = basename(outputPath).replace(/\.json$/i, '')
const screenshotPath = join(screenshotDirectory, `${outputName}-render.png`)
const mixedScreenshotPath = (suffix) => join(screenshotDirectory, `${outputName}${suffix}.png`)
mkdirSync(dirname(outputPath), { recursive: true })
mkdirSync(screenshotDirectory, { recursive: true })

const browser = await chromium.connectOverCDP(cdpUrl, { noDefaults: true })
let page
const errors = []
try {
  const pages = browser.contexts().flatMap((context) => context.pages())
  page = pages.find((candidate) => candidate.url().startsWith('http://localhost:5188/'))
  assert.ok(page, `No Tauri dev page at http://localhost:5188/; found ${pages.map((p) => p.url()).join(', ')}`)
  await page.waitForFunction(() => window.fileterm?.platform === 'win32', undefined, { timeout: 10000 })
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })

  const cdp = await page.context().newCDPSession(page)
  await cdp.send('Performance.enable')
  const beforeMetrics = await cdp.send('Performance.getMetrics')

  await page.evaluate(async () => {
    const stylesheet = document.createElement('link')
    stylesheet.rel = 'stylesheet'
    stylesheet.href = '/node_modules/@xterm/xterm/css/xterm.css'
    await new Promise((resolve, reject) => {
      stylesheet.onload = resolve
      stylesheet.onerror = reject
      document.head.append(stylesheet)
    })
    await document.fonts.ready

    const { createTerminalLifecycleRuntime } = await import(
      `/src/renderer/components/terminal-lifecycle-core.ts?smoke=${Date.now()}`
    )
    const host = document.createElement('main')
    host.id = 'fileterm-webview2-smoke-host'
    host.className = 'terminal-host'
    host.style.cssText =
      'position:fixed;left:12px;top:72px;width:calc(100vw - 24px);height:calc(100vh - 84px);z-index:2147483647;background:#111;'
    document.body.append(host)

    const createRuntime = (element) => {
      const inner = document.createElement('div')
      inner.className = 'terminal-inner'
      element.append(inner)
      return createTerminalLifecycleRuntime({
        hostRef: { current: inner },
        profileIdRef: { current: 'renderer-test' },
        tabIdRef: { current: `renderer-test-${crypto.randomUUID()}` },
        terminalRef: { current: null },
        fitAddonRef: { current: null },
        searchAddonRef: { current: null },
        terminalLogColorizerRef: { current: null },
        pendingWriteRef: { current: '' },
        renderedTranscriptRef: { current: '' },
        isActiveRef: { current: true },
        setTerminalScrollableElement: () => {},
        isMac: false,
        isWindowsPty: false,
        buildTerminalTheme: () => ({ foreground: '#dddddd', background: '#111111', selectionBackground: '#444444' })
      })
    }

    window.__filetermWebView2Smoke = { host, createRuntime, runtime: createRuntime(host) }
  })

  await page.waitForFunction(
    (renderer) => {
      const terminal = window.__filetermWebView2Smoke?.runtime?.terminal
      const activeRenderer = terminal?._core?._renderService?._renderer?.value
      return renderer === 'webgl'
        ? Boolean(activeRenderer?._gl && !activeRenderer._gl.isContextLost())
        : Boolean(terminal?.element?.querySelector('.xterm-rows'))
    },
    expectedRenderer,
    { timeout: 15000 }
  )

  const active = await page.evaluate(async (renderer) => {
    const { runtime, host } = window.__filetermWebView2Smoke
    const terminal = runtime.terminal
    const startedAt = performance.now()
    const rows = Array.from({ length: 1500 }, (_, index) => {
      const needle = index === 250 ? ' Needle' : ''
      return `row ${String(index).padStart(4, '0')} 中文🙂${needle} 0123456789`
    })
    const payload = `Header 中文🙂\r\n${rows.join('\r\n')}\r\n`
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error('xterm write callback timed out')), 15000)
      terminal.write(payload, () => {
        clearTimeout(timeout)
        resolve()
      })
    })
    const parseAndWriteCallbackMs = performance.now() - startedAt
    await new Promise((resolve) => requestAnimationFrame(() => resolve()))
    const callbackToNextFrameMs = performance.now() - startedAt - parseAndWriteCallbackMs

    runtime.fitAddon.fit()
    const originalSize = { cols: terminal.cols, rows: terminal.rows }
    host.style.width = '640px'
    runtime.fitAddon.fit()
    const narrowSize = { cols: terminal.cols, rows: terminal.rows }
    host.style.width = `${Math.max(640, window.innerWidth - 24)}px`
    runtime.fitAddon.fit()
    const restoredSize = { cols: terminal.cols, rows: terminal.rows }
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))

    terminal.select(0, 0, 6)
    const selection = terminal.getSelection()
    const searchFound = runtime.searchAddon.findNext('Needle')
    const firstLine = terminal.buffer.active.getLine(0)?.translateToString(true) ?? ''
    const rendererName = terminal._core._renderService._renderer.value.constructor.name
    const canvas = [...terminal.element.querySelectorAll('canvas')].find((item) => item.getContext('webgl2'))
    const gl = canvas?.getContext('webgl2') ?? null
    const activeRenderer = terminal._core._renderService._renderer.value
    const measureGrid = (target = terminal) => {
      const renderer = target._core._renderService._renderer.value
      const { css, device } = renderer.dimensions
      const targetGl = renderer._gl
      return {
        dpr: target._core._coreBrowserService.dpr,
        measuredWidth: target._core._charSizeService.width,
        cellWidth: css.cell.width,
        cellHeight: css.cell.height,
        deviceCellWidth: device.cell.width,
        deviceCellHeight: device.cell.height,
        canvasWidth: css.canvas.width,
        canvasHeight: css.canvas.height,
        hostWidth: target.element.parentElement.getBoundingClientRect().width,
        hostHeight: target.element.parentElement.getBoundingClientRect().height,
        cols: target.cols,
        rows: target.rows,
        gpuSurface: targetGl
          ? {
              width: targetGl.drawingBufferWidth,
              height: targetGl.drawingBufferHeight,
              viewport: Array.from(targetGl.getParameter(targetGl.VIEWPORT)),
              nonIntegerGlyphs: renderer._charAtlas.pages
                .flatMap((page) => page.glyphs)
                .filter(
                  (glyph) =>
                    !Number.isInteger(glyph.size.x) ||
                    !Number.isInteger(glyph.size.y) ||
                    !Number.isInteger(glyph.texturePosition.x) ||
                    !Number.isInteger(glyph.texturePosition.y)
                ).length
            }
          : null
      }
    }
    let domReference = null
    const measureDomReference = async () => {
      if (!domReference) {
        const referenceHost = document.createElement('main')
        referenceHost.className = 'terminal-host'
        referenceHost.style.cssText = `position:fixed;left:-10000px;top:0;width:${host.clientWidth}px;height:${host.clientHeight}px;`
        document.body.append(referenceHost)
        const referenceInner = document.createElement('div')
        referenceInner.className = 'terminal-inner'
        referenceHost.append(referenceInner)
        const reference = new terminal.constructor({
          fontFamily: terminal.options.fontFamily,
          fontSize: terminal.options.fontSize,
          lineHeight: terminal.options.lineHeight,
          letterSpacing: terminal.options.letterSpacing,
          overviewRuler: { width: 0 },
          scrollback: terminal.options.scrollback
        })
        const fit = new runtime.fitAddon.constructor()
        reference.loadAddon(fit)
        reference.open(referenceInner)
        domReference = { terminal: reference, fit, host: referenceHost }
        window.__filetermWebView2Smoke.domReference = domReference
      }
      domReference.terminal.options.fontFamily = terminal.options.fontFamily
      domReference.terminal.options.fontSize = terminal.options.fontSize
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      domReference.fit.fit()
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      return measureGrid(domReference.terminal)
    }
    const gridMetrics = measureGrid()
    const domReferenceGrid = await measureDomReference()
    const screen = terminal.element.querySelector('.xterm-screen')
    const bounds = screen.getBoundingClientRect()
    const hostBounds = terminal.element.parentElement.getBoundingClientRect()
    const surfaceFitsHost = bounds.right <= hostBounds.right + 1 && bounds.bottom <= hostBounds.bottom + 1
    const hitColumns = [0, Math.floor(terminal.cols / 2), terminal.cols - 1].map(
      (column) =>
        terminal._core._mouseService.getCoords(
          {
            clientX: bounds.left + ((column + 0.25) * bounds.width) / terminal.cols,
            clientY: bounds.top + (0.25 * bounds.height) / terminal.rows
          },
          screen,
          terminal.cols,
          terminal.rows,
          false
        )?.[0]
    )
    const configuredFont = terminal.options.fontFamily
    const configuredFontSize = terminal.options.fontSize
    const loadedReferenceFont = await document.fonts.load('400 14px "JetBrains Mono"')
    terminal.options.fontFamily = 'JetBrains Mono'
    terminal.options.fontSize = 14
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    runtime.fitAddon.fit()
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    const referenceFontMetrics = {
      fontAvailable: loadedReferenceFont.length > 0 && document.fonts.check('14px "JetBrains Mono"'),
      fontFamily: terminal.options.fontFamily,
      fontSize: terminal.options.fontSize,
      ...measureGrid()
    }
    const referenceFontDomGrid = await measureDomReference()
    terminal.options.fontFamily = configuredFont
    terminal.options.fontSize = configuredFontSize
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    runtime.fitAddon.fit()
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    const restoredFontGrid = measureGrid()
    const restoredFontDomGrid = await measureDomReference()
    domReference.terminal.dispose()
    domReference.host.remove()
    window.__filetermWebView2Smoke.domReference = null
    const debug = gl?.getExtension('WEBGL_debug_renderer_info') ?? null
    const gpuRenderer = debug ? String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL)) : null
    const gpuVendor = debug ? String(gl.getParameter(debug.UNMASKED_VENDOR_WEBGL)) : null
    const software = /swiftshader|llvmpipe|software|warp|basic render driver/i.test(gpuRenderer ?? '')
    window.__filetermWebView2Smoke.webglRenderer = activeRenderer

    return {
      requestedRenderer: renderer,
      rendererName,
      rendererKind: activeRenderer._gl && !activeRenderer._gl.isContextLost() ? 'webgl' : 'dom',
      rowCount: terminal.buffer.active.length,
      parseAndWriteCallbackMs,
      callbackToNextFrameMs,
      firstLine,
      selection,
      searchFound,
      originalSize,
      narrowSize,
      restoredSize,
      gridMetrics,
      domReferenceGrid,
      hitColumns,
      surfaceFitsHost,
      referenceFontMetrics,
      referenceFontDomGrid,
      restoredFontGrid,
      restoredFontDomGrid,
      fontFamily: terminal.options.fontFamily,
      fontSize: terminal.options.fontSize,
      devicePixelRatio: window.devicePixelRatio,
      gpuVendor,
      gpuRenderer,
      hardwareAcceleration: gpuRenderer ? !software : null,
      webglCanvas: Boolean(canvas),
      contextLossSupported: Boolean(gl?.getExtension('WEBGL_lose_context'))
    }
  }, expectedRenderer)

  assert.equal(active.requestedRenderer, expectedRenderer)
  assert.equal(active.rendererKind, expectedRenderer)
  assert.match(active.firstLine, /Header 中文/)
  assert.equal(active.selection, 'Header')
  assert.equal(active.searchFound, true)
  assert.notDeepEqual(active.originalSize, active.narrowSize)
  assert.equal(active.surfaceFitsHost, true, 'terminal surface must fit inside the padded production host')
  for (const [actual, reference] of [
    [active.gridMetrics, active.domReferenceGrid],
    [active.referenceFontMetrics, active.referenceFontDomGrid],
    [active.restoredFontGrid, active.restoredFontDomGrid]
  ]) {
    if (actual.gpuSurface) {
      assert.ok(Number.isInteger(actual.deviceCellWidth), 'WebGL atlas cell width must be integer')
      assert.equal(actual.gpuSurface.nonIntegerGlyphs, 0, 'WebGL glyph atlas pixels must be integer')
      assert.deepEqual(actual.gpuSurface.viewport, [0, 0, actual.gpuSurface.width, actual.gpuSurface.height])
    }
    for (const key of expectedRenderer === 'webgl'
      ? ['measuredWidth']
      : ['cols', 'rows', 'cellWidth', 'cellHeight', 'canvasWidth', 'canvasHeight', 'measuredWidth'])
      assert.ok(Math.abs(actual[key] - reference[key]) < 1e-6, `${key}: ${actual[key]} != DOM ${reference[key]}`)
  }
  assert.deepEqual(active.hitColumns, [1, Math.floor(active.gridMetrics.cols / 2) + 1, active.gridMetrics.cols])
  if (expectedRenderer === 'webgl') {
    assert.equal(active.webglCanvas, true)
    assert.equal(active.contextLossSupported, true)
    assert.equal(active.hardwareAcceleration, true)
  } else {
    assert.equal(active.webglCanvas, false)
  }

  await page.locator('#fileterm-webview2-smoke-host').screenshot({ path: screenshotPath })

  const mixedText = []
  for (const fontSize of [12, 14, 16]) {
    const sample = await page.evaluate(async (fontSize) => {
      const { runtime } = window.__filetermWebView2Smoke
      const { terminal, fitAddon } = runtime
      terminal.options.fontSize = fontSize
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      fitAddon.fit()
      terminal.reset()
      const lines = [
        '中文 + Symbols + English + 0123456789 / WebGL 字形验证',
        '连接主机... ABCabc 0123456789 !@#$%^&*()_+-=[]{};:,.<>/?',
        '连接主机成功 中文测试：你好，世界！ FileTerm Windows WebView2',
        '中文夹英文 English夹中文 abc中文XYZ 123中文456 中文123ABC',
        '数字 0123456789 12312312312333333333123123123123123 0O1Il',
        '半角 [] {} () <> | \\ / ~ ` ^ _ + - = : ; , . ! ? @ # $ % & *',
        '全角 【】「」《》（）［］｛｝：；，。！？＋－＝１２３ＡＢＣ',
        '符号 ← ↑ → ↓ ↔ ✓ ✗ ★ ☆ ● ○ ◆ ◇ ■ □ ✻ ▢',
        '线框 ┌────────┬────────┐  ┏━━━━┳━━━━┓  ▀▄█░▒▓',
        '表格 │ 中文   │ ABC123 │  ┃ 中 ┃ 01 ┃',
        '线框 └────────┴────────┘  ┗━━━━┻━━━━┛',
        'Emoji 中文🙂 English🚀 123🧪 ❤️ 👨‍👩‍👧‍👦 🇨🇳',
        '\u001b[31m红色中文 Red123!\u001b[0m \u001b[32m绿色中文 Green456?\u001b[0m \u001b[35m123123中文ABC\u001b[0m',
        '\u001b[1m粗体中文 Bold ABC 0123456789 []{}\u001b[0m / 普通 Regular 0123456789',
        '\u001b[31mA\u001b[32mB\u001b[33mC\u001b[34m1\u001b[35m2\u001b[36m3\u001b[37m中\u001b[31m文\u001b[0m / 相邻彩色字形与数字',
        'Warning: Data should only be stored in shared folders. 中文提示 12345',
        'PS C:\\Test> 中文ABC123 + 12312312312333333333123123123123123'
      ]
      await new Promise((resolve) => terminal.write(lines.join('\r\n'), resolve))
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      const sgr = new RegExp(`${String.fromCharCode(27)}\\[[\\d;]*m`, 'g')
      const expected = lines.map((line) => line.replace(sgr, ''))
      const actual = expected.map((_, row) => terminal.buffer.active.getLine(row).translateToString(true))
      terminal.select(0, 3, 6)
      const selection = terminal.getSelection()
      terminal.clearSelection()
      const renderer = terminal._core._renderService._renderer.value
      return {
        fontSize,
        fontFamily: terminal.options.fontFamily,
        dpr: window.devicePixelRatio,
        cols: terminal.cols,
        rows: terminal.rows,
        expected,
        actual,
        selection,
        webgl: Boolean(renderer._gl),
        nonIntegerGlyphs:
          renderer._charAtlas?.pages
            .flatMap((page) => page.glyphs)
            .filter(
              (glyph) =>
                !Number.isInteger(glyph.size.x) ||
                !Number.isInteger(glyph.size.y) ||
                !Number.isInteger(glyph.texturePosition.x) ||
                !Number.isInteger(glyph.texturePosition.y)
            ).length ?? null
      }
    }, fontSize)
    assert.deepEqual(sample.actual, sample.expected, `mixed Chinese/English/symbols/digits at ${fontSize}px`)
    assert.equal(sample.selection, '中文夹')
    assert.equal(sample.webgl, expectedRenderer === 'webgl')
    if (sample.webgl) assert.equal(sample.nonIntegerGlyphs, 0)
    const mixedScreenshot = mixedScreenshotPath(`-mixed-${fontSize}px`)
    await page.locator('#fileterm-webview2-smoke-host').screenshot({ path: mixedScreenshot })
    mixedText.push({
      ...sample,
      screenshot: relative(dirname(outputPath), mixedScreenshot).replaceAll('\\', '/')
    })
  }
  await page.evaluate(async (fontSize) => {
    const { runtime } = window.__filetermWebView2Smoke
    runtime.terminal.options.fontSize = fontSize
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    runtime.fitAddon.fit()
  }, active.fontSize)

  let contextLossFallback = null
  if (expectedRenderer === 'webgl') {
    await page.evaluate(() => {
      const terminal = window.__filetermWebView2Smoke.runtime.terminal
      const canvas = [...terminal.element.querySelectorAll('canvas')].find((item) => item.getContext('webgl2'))
      canvas.getContext('webgl2').getExtension('WEBGL_lose_context').loseContext()
    })
    await page.waitForFunction(
      () => {
        const test = window.__filetermWebView2Smoke
        const renderer = test.runtime.terminal._core._renderService._renderer.value
        return renderer !== test.webglRenderer && Boolean(test.runtime.terminal.element.querySelector('.xterm-rows'))
      },
      undefined,
      { timeout: 10000 }
    )
    contextLossFallback = await page.evaluate(() => ({
      rendererName:
        window.__filetermWebView2Smoke.runtime.terminal._core._renderService._renderer.value.constructor.name,
      domRows: Boolean(window.__filetermWebView2Smoke.runtime.terminal.element.querySelector('.xterm-rows')),
      cols: window.__filetermWebView2Smoke.runtime.terminal.cols,
      rows: window.__filetermWebView2Smoke.runtime.terminal.rows
    }))
    assert.equal(contextLossFallback.domRows, true)
    assert.ok(
      Math.abs(contextLossFallback.cols - active.restoredFontDomGrid.cols) <= 1,
      'native FitAddon rounding may differ by one column after renderer switch'
    )
    assert.equal(contextLossFallback.rows, active.restoredFontDomGrid.rows)
    await page.locator('#fileterm-webview2-smoke-host').screenshot({
      path: mixedScreenshotPath('-mixed-dom-fallback')
    })
  }

  await page.evaluate(() => {
    window.__filetermWebView2Smoke.runtime.disposeCore()
    window.__filetermWebView2Smoke.host.remove()
  })

  const normalDisposals = []
  for (let index = 0; index < 3; index += 1) {
    await page.evaluate((cycle) => {
      const host = document.createElement('main')
      host.className = 'terminal-host'
      host.style.cssText = 'position:fixed;left:-10000px;top:0;width:900px;height:600px;'
      document.body.append(host)
      const runtime = window.__filetermWebView2Smoke.createRuntime(host)
      window.__filetermWebView2Smoke.cycle = { host, runtime, cycle }
    }, index)
    await page.waitForFunction(
      (renderer) => {
        const terminal = window.__filetermWebView2Smoke.cycle.runtime.terminal
        const activeRenderer = terminal._core._renderService._renderer.value
        return renderer === 'webgl'
          ? Boolean(activeRenderer?._gl && !activeRenderer._gl.isContextLost())
          : Boolean(terminal.element.querySelector('.xterm-rows'))
      },
      expectedRenderer,
      { timeout: 10000 }
    )
    normalDisposals.push(
      await page.evaluate(() => {
        const { host, runtime } = window.__filetermWebView2Smoke.cycle
        const activeRenderer = runtime.terminal._core._renderService._renderer.value
        const canvas = [...runtime.terminal.element.querySelectorAll('canvas')].find((item) =>
          item.getContext('webgl2')
        )
        const gl = canvas?.getContext('webgl2') ?? null
        const rendererKind = activeRenderer._gl && !activeRenderer._gl.isContextLost() ? 'webgl' : 'dom'
        runtime.disposeCore()
        host.remove()
        return {
          rendererKind,
          contextLostAfterDispose: gl ? gl.isContextLost() : null,
          hostChildrenAfterDispose: host.querySelector('.terminal-inner').childElementCount
        }
      })
    )
  }
  if (expectedRenderer === 'webgl') {
    assert.ok(normalDisposals.every((item) => item.contextLostAfterDispose && item.hostChildrenAfterDispose === 0))
  } else {
    assert.ok(
      normalDisposals.every(
        (item) =>
          item.rendererKind === 'dom' && item.contextLostAfterDispose === null && item.hostChildrenAfterDispose === 0
      )
    )
  }

  const afterMetrics = await cdp.send('Performance.getMetrics')
  const metricMap = (metrics) => Object.fromEntries(metrics.metrics.map(({ name, value }) => [name, value]))
  const before = metricMap(beforeMetrics)
  const after = metricMap(afterMetrics)
  const cdpDelta = Object.fromEntries(
    ['ScriptDuration', 'LayoutDuration', 'RecalcStyleDuration', 'TaskDuration', 'JSHeapUsedSize'].map((name) => [
      name,
      name === 'JSHeapUsedSize' ? after[name] : after[name] - before[name]
    ])
  )

  await page.waitForTimeout(250)
  assert.deepEqual(errors, [])
  const result = {
    environment: await page.evaluate(() => ({
      url: location.href,
      userAgent: navigator.userAgent,
      platform: navigator.platform,
      devicePixelRatio: window.devicePixelRatio,
      viewport: { width: window.innerWidth, height: window.innerHeight },
      hardwareConcurrency: navigator.hardwareConcurrency,
      appPlatform: window.fileterm?.platform
    })),
    expectedRenderer,
    active,
    mixedText,
    contextLossFallback,
    normalDisposals,
    cdpDelta,
    unavailable: ['native terminal input latency', 'PTY parse/ACK backlog', 'native WebView2 IME composition'],
    pageErrors: errors,
    screenshotPath: relative(dirname(outputPath), screenshotPath).replaceAll('\\', '/'),
    appLog: 'FileTerm app.log; filter event=terminal-renderer'
  }
  writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`)
  console.log(
    JSON.stringify(
      {
        outputPath: relative(archiveRoot, outputPath).replaceAll('\\', '/'),
        screenshotPath: relative(dirname(outputPath), screenshotPath).replaceAll('\\', '/'),
        result
      },
      null,
      2
    )
  )
  await cdp.detach()
} finally {
  if (page) {
    await page
      .evaluate(() => {
        const test = window.__filetermWebView2Smoke
        test?.domReference?.terminal.dispose()
        test?.domReference?.host.remove()
        try {
          test?.runtime?.disposeCore()
        } catch {
          // Cleanup is best effort after the test has already failed.
        }
        try {
          test?.host?.remove()
          test?.cycle?.runtime?.disposeCore()
          test?.cycle?.host?.remove()
        } catch {
          // Cleanup is best effort after the test has already failed.
        }
      })
      .catch(() => undefined)
  }
  await browser.close()
}
