/* global document, window, createImageBitmap, requestAnimationFrame */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
const { build } = require(process.env.ESBUILD_MODULE_PATH || 'esbuild')
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')

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
  for (const platform of ['win32', 'linux'])
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
      .home span:not(.icon) { overflow: hidden }
      .label { padding: 2px 0; overflow: hidden }
    </style><div id="root"></div>`)
      await page.evaluate((platform) => {
        window.fileterm = { platform }
      }, platform)
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
        for (const selector of ['.home', '.settings']) {
          const clipped = await page.locator(selector).screenshot()
          await page.addStyleTag({ content: `${selector} span:not(.icon) { overflow: visible !important }` })
          const unclipped = await page.locator(selector).screenshot()
          const bounds = await page.evaluate(
            async (images) => {
              return Promise.all(
                images.map(async (encoded) => {
                  const bitmap = await createImageBitmap(await (await fetch(`data:image/png;base64,${encoded}`)).blob())
                  const canvas = document.createElement('canvas')
                  canvas.width = bitmap.width
                  canvas.height = bitmap.height
                  const context = canvas.getContext('2d')
                  context.drawImage(bitmap, 0, 0)
                  const pixels = context.getImageData(0, 0, bitmap.width, bitmap.height).data
                  let top = bitmap.height,
                    bottom = -1
                  for (let y = 0; y < bitmap.height; y++)
                    for (let x = 0; x < bitmap.width; x++) {
                      const i = (y * bitmap.width + x) * 4
                      if (pixels[i + 2] > pixels[i] + 40 && pixels[i + 2] > pixels[i + 1] + 40) {
                        top = Math.min(top, y)
                        bottom = Math.max(bottom, y)
                      }
                    }
                  return [top, bottom]
                })
              )
            },
            [clipped.toString('base64'), unclipped.toString('base64')]
          )
          assert.ok(
            bounds[0][1] >= 0 && bounds[0].every((value, i) => Math.abs(value - bounds[1][i]) <= 1),
            `${family} at ${scale}: ${selector} label is clipped: ${bounds}`
          )
          await page
            .locator('style')
            .last()
            .evaluate((style) => style.remove())
        }
      }
      await page.close()
    }
  const page = await browser.newPage({ viewport: { width: 320, height: 600 }, deviceScaleFactor: 2 })
  const homeCss = readFileSync(new URL('../../src/renderer/styles/features/home.css', import.meta.url), 'utf8')
  const workspaceCss = readFileSync(
    new URL('../../src/renderer/features/workspace/home-workspace.css', import.meta.url),
    'utf8'
  )
  await page.setContent(`<style>
    @font-face { font-family: Inter; src: url(data:font/ttf;base64,${font('inter/Inter-normal-500.ttf')}); font-weight: 500 }
    @font-face { font-family: 'Noto Sans SC'; src: url(data:font/ttf;base64,${font('noto-sans-sc/NotoSansSC-Variable.ttf')}); font-weight: 100 900 }
    * { box-sizing: border-box }
    body { margin: 0; background: white; font-family: var(--font-ui) }
    button { font: inherit; line-height: 1 }
    ${homeCss}
    ${workspaceCss}
    .home-sidebar { width: 240px; height: 500px }
    .sidebar-nav-link { color: blue }
  </style><aside class="home-sidebar"><div class="sidebar-brand"><h2 class="brand-title">FileTerm</h2><span class="brand-version">v2.3.2-beta.3</span></div><nav class="sidebar-nav"><button class="sidebar-nav-link"><span class="material-symbols-outlined">●</span><span>后台会话</span></button><button class="sidebar-nav-link"><span class="material-symbols-outlined">●</span><span>设置</span></button></nav></aside>`)
  for (const platform of ['darwin', 'win32', 'linux']) {
    const positions = []
    for (const family of ['Inter, "Noto Sans SC"', '"Noto Sans SC"']) {
      await page.evaluate(
        async ({ platform, family }) => {
          document.documentElement.dataset.platform = platform
          document.documentElement.style.setProperty('--font-ui', family)
          await document.fonts.load(`500 13.5px ${family}`, '后台会话设置')
          await document.fonts.ready
        },
        { platform, family }
      )
      positions.push(
        await page
          .locator('.sidebar-nav-link')
          .evaluateAll((rows) => rows.map((row) => row.getBoundingClientRect().top))
      )
      assert.deepEqual(
        await page
          .locator('.sidebar-nav-link')
          .evaluateAll((rows) => rows.map((row) => row.getBoundingClientRect().height)),
        [40, 40],
        `${platform} ${family}: navigation row height changed`
      )
      const clipped = await page.locator('.sidebar-nav').screenshot()
      await page.addStyleTag({
        content: '.home-sidebar .sidebar-nav-link > span:not(.material-symbols-outlined) { overflow: visible }'
      })
      const unclipped = await page.locator('.sidebar-nav').screenshot()
      assert.deepEqual(clipped, unclipped, `${platform} ${family}: navigation label is clipped`)
      await page
        .locator('style')
        .last()
        .evaluate((style) => style.remove())
    }
    assert.deepEqual(positions[0], positions[1], `${platform}: navigation positions change with font`)
  }
  await page.close()
} finally {
  await browser.close()
}
