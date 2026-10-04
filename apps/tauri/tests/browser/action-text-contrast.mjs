/* global window, document, getComputedStyle, requestAnimationFrame */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { build } = require(process.env.ESBUILD_MODULE_PATH || 'esbuild')
const { chromium, webkit } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const directory = mkdtempSync(join(tmpdir(), 'fileterm-action-contrast-'))
await build({
  stdin: {
    resolveDir: fileURLToPath(new URL('../../../../', import.meta.url)),
    loader: 'tsx',
    contents: `
import { createRoot } from 'react-dom/client'
import { Button } from './apps/tauri/src/renderer/components/common/button/button'
import { createCodexThemeConfig } from '@fileterm/core'
import { applyThemeVariables } from './apps/tauri/src/renderer/app/theme-config'
import './apps/tauri/src/renderer/styles/index.css'
window.setTheme = (variant, action) => {
  const config = createCodexThemeConfig(variant)
  if (action) {
    config.theme.semanticColors.primaryAction = action
    config.theme.semanticColors.dangerAction = action
  }
  applyThemeVariables('codex-' + variant, config)
}
window.setTheme('light')
createRoot(document.getElementById('root')).render(<div style={{background:'var(--surface-panel)',padding:40}}>
  <Button variant="primary">Continue</Button><Button variant="danger">Delete</Button>
</div>)
`
  },
  bundle: true,
  outfile: join(directory, 'bundle.js'),
  jsx: 'automatic',
  loader: { '.svg': 'dataurl', '.ttf': 'dataurl', '.woff2': 'dataurl', '.png': 'dataurl' },
  logLevel: 'silent'
})

let browser
try {
  const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
  browser = await engine.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL })
  const page = await browser.newPage({ viewport: { width: 900, height: 400 } })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.setContent('<html data-platform="win32"><body><main id="root"></main></body></html>')
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addStyleTag({ content: '.ft-btn { transition: none; }' })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
  await page.locator('.ft-btn--primary').waitFor()
  const read = (variant) =>
    page.evaluate((variant) => {
      const button = document.querySelector('.ft-btn--' + variant)
      const style = getComputedStyle(button)
      const canvas = document.createElement('canvas')
      canvas.width = canvas.height = 1
      const context = canvas.getContext('2d', { willReadFrequently: true })
      const rgba = (css) => {
        context.clearRect(0, 0, 1, 1)
        context.fillStyle = css
        context.fillRect(0, 0, 1, 1)
        return [...context.getImageData(0, 0, 1, 1).data]
      }
      const foreground = rgba(style.color)
      const background = rgba(style.backgroundColor)
      const parent = rgba(getComputedStyle(button.parentElement).backgroundColor)
      const composite = (front, back) =>
        front.slice(0, 3).map((v, i) => (v * front[3] + back[i] * (255 - front[3])) / 255)
      const effectiveBackground = composite(background, parent)
      const luminance = (rgb) =>
        rgb
          .map((v) => v / 255)
          .map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
          .reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0)
      const a = luminance(composite(foreground, [...effectiveBackground, 255]))
      const b = luminance(effectiveBackground)
      return {
        color: style.color,
        background: style.backgroundColor,
        ratio: (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
      }
    }, variant)
  const paint = () =>
    page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))))
  let checks = 0
  for (const [theme, action] of [
    ['light', null],
    ['dark', null],
    ['light', '#fff'],
    ['dark', '#fff'],
    ['light', '#ffff00'],
    ['dark', '#ffff00'],
    ['light', '#ffffff22'],
    ['dark', '#ffffff22'],
    ['light', '#fff2'],
    ['dark', '#000'],
    ['light', '#000000ff']
  ]) {
    await page.evaluate(([theme, action]) => window.setTheme(theme, action), [theme, action])
    for (const variant of ['primary', 'danger']) {
      const button = page.locator('.ft-btn--' + variant)
      await page.mouse.move(800, 350)
      await paint()
      const normal = await read(variant)
      // These active controls must never have the same/pale-on-pale label.
      assert.ok(normal.ratio >= 4.48, `${theme}/${action}/${variant}: ${JSON.stringify(normal)}`)
      for (const state of ['hover', 'active']) {
        await button.hover()
        if (state === 'active') await page.mouse.down()
        await paint()
        const current = await read(variant)
        // Keep the label stable while the button's existing background changes.
        assert.equal(current.color, normal.color)
        assert.ok(current.ratio >= 3, `${theme}/${action}/${variant}/${state}: ${JSON.stringify(current)}`)
        if (state === 'active') await page.mouse.up()
        checks++
      }
      if (theme === 'light' && !action && variant === 'primary') assert.equal(normal.color, 'rgb(26, 28, 31)')
      if (theme === 'dark' && !action && variant === 'primary') assert.ok(normal.ratio >= 4.48)
      checks++
    }
  }
  assert.deepEqual(errors, [])
  console.log(
    `PASS: ${checks} actual Button checks, Codex dark/light and custom white/yellow/transparent/black backgrounds, normal/hover/active, stable readable labels`
  )
} finally {
  await browser?.close()
  rmSync(directory, { recursive: true, force: true })
}
