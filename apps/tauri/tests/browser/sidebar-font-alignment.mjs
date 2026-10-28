/* global document, window, createImageBitmap, requestAnimationFrame */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { chromium } from 'playwright'

const root = fileURLToPath(new URL('../../../../', import.meta.url))
const font = (path) =>
  readFileSync(new URL(`../../src/renderer/assets/fonts/text/${path}`, import.meta.url)).toString('base64')
const icon = (path) => readFileSync(new URL(`../../src/renderer/assets/icons/${path}`, import.meta.url), 'utf8')
const dashboard = icon('navigation/material-dashboard.svg')
const shield = icon('security/app-shield.svg')
const bundle = await build({
  stdin: {
    resolveDir: root,
    loader: 'tsx',
    contents: `
      import React from 'react'
      import {createRoot} from 'react-dom/client'
      import {SidebarAlignedLabel} from './apps/tauri/src/renderer/components/common/icon-label/icon-label'
      createRoot(document.getElementById('root')).render(<>
        <div className="row home"><span className="icon" dangerouslySetInnerHTML={{__html:${JSON.stringify(shield)}}}/><SidebarAlignedLabel>代理与隧道</SidebarAlignedLabel></div>
        <div className="row settings"><span className="icon" dangerouslySetInnerHTML={{__html:${JSON.stringify(dashboard)}}}/><SidebarAlignedLabel className="label">界面与概览</SidebarAlignedLabel></div>
        <div className="row control"><span className="icon" dangerouslySetInnerHTML={{__html:${JSON.stringify(shield)}}}/><span>代理与隧道</span></div>
      </>)
    `
  },
  bundle: true,
  write: false,
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': '"production"' },
  logLevel: 'silent'
})
const browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_CHANNEL || 'chrome', headless: true })
try {
  for (const scale of [1, 1.25, 2]) {
    const page = await browser.newPage({ viewport: { width: 300, height: 160 }, deviceScaleFactor: scale })
    await page.setContent(`<style>
      @font-face { font-family: Inter; src: url(data:font/ttf;base64,${font('inter/Inter-normal-500.ttf')}); font-weight: 500 }
      @font-face { font-family: 'Noto Sans SC'; src: url(data:font/ttf;base64,${font('noto-sans-sc/NotoSansSC-Variable.ttf')}); font-weight: 100 900 }
      @font-face { font-family: ImportedTest; src: url(data:font/ttf;base64,${font('noto-sans-sc/NotoSansSC-Variable.ttf')}); font-weight: 500; ascent-override: 145%; descent-override: 15% }
      body { margin: 0; background: white }
      .row { display: flex; align-items: center; gap: 12px; height: 50px; font: 500 13.5px/1.4 var(--font-ui); }
      .icon { color: red; width: 18px; height: 18px; flex: none; display: flex; align-items: center }
      .icon svg { display: block; width: 18px; height: 18px }
      .row span:not(.icon) { color: blue; white-space: nowrap }
      .label { padding: 2px 0; overflow: hidden }
    </style><div id="root"></div>`)
    await page.evaluate(() => {
      window.fileterm = { platform: 'win32' }
    })
    await page.addScriptTag({ content: bundle.outputFiles[0].text })
    for (const family of ['"Microsoft YaHei"', 'Inter, "Noto Sans SC"', '"Noto Sans SC"', 'ImportedTest']) {
      await page.evaluate(async (family) => {
        document.documentElement.style.setProperty('--font-ui', family)
        await document.fonts.load(`500 13.5px ${family}`, '代理与隧道界面概览')
        await document.fonts.ready
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      }, family)
      const screenshot = await page.screenshot()
      const offsets = await page.evaluate(
        async ({ encoded, scale }) => {
          const bitmap = await createImageBitmap(await (await fetch(`data:image/png;base64,${encoded}`)).blob())
          const canvas = document.createElement('canvas')
          canvas.width = bitmap.width
          canvas.height = bitmap.height
          const context = canvas.getContext('2d')
          context.drawImage(bitmap, 0, 0)
          const pixels = context.getImageData(0, 0, bitmap.width, bitmap.height).data
          return [...document.querySelectorAll('.row')].map((row) => {
            const rect = row.getBoundingClientRect()
            let min = bitmap.height
            let max = -1
            let iconMin = bitmap.height
            let iconMax = -1
            for (let y = Math.floor(rect.top * scale); y < Math.ceil(rect.bottom * scale); y++) {
              for (let x = 0; x < bitmap.width; x++) {
                const i = (y * bitmap.width + x) * 4
                if (pixels[i + 2] > pixels[i] + 40 && pixels[i + 2] > pixels[i + 1] + 40) {
                  min = Math.min(min, y)
                  max = Math.max(max, y)
                }
                if (pixels[i] > pixels[i + 1] + 40 && pixels[i] > pixels[i + 2] + 40) {
                  iconMin = Math.min(iconMin, y)
                  iconMax = Math.max(iconMax, y)
                }
              }
            }
            if (max < 0 || iconMax < 0) throw new Error('Text or icon was not rendered')
            return Math.abs((min + max - iconMin - iconMax) / 2 / scale)
          })
        },
        { encoded: screenshot.toString('base64'), scale }
      )
      assert.ok(
        offsets.slice(0, 2).every((offset) => offset <= (family.includes('Noto Sans SC') ? 2 : 1.5)),
        `${family} at ${scale}: ${offsets}`
      )
      if (family === 'ImportedTest') assert.ok(offsets[2] > 1.5, `Control did not expose the issue: ${offsets}`)
    }
    await page.close()
  }
} finally {
  await browser.close()
}
