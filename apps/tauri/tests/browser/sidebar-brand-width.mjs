/* global document, getComputedStyle */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const renderer = new URL('../../src/renderer/', import.meta.url)
function stylesheet(url) {
  return readFileSync(url, 'utf8').replace(/@import\s+['"]([^'"]+)['"]\s*;/g, (_, path) =>
    stylesheet(new URL(path, url))
  )
}
const css = ['features/layout/app-main-workspace.css', 'features/layout/tab-bar.css', 'styles/index.css']
  .map((path) => stylesheet(new URL(path, renderer)))
  .join('\n')
const browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL || 'chrome' })
try {
  const page = await browser.newPage({ viewport: { width: 1150, height: 820 }, reducedMotion: 'reduce' })
  await page.setContent(`
    <div class="fs-shell" style="--brand-width:214px;--sidebar-width:214px">
      <div class="fs-tabbar"><div class="titlebar-brand">FileTerm</div><div class="titlebar-tabarea"></div></div>
      <div class="home-workspace" style="grid-column:1 / -1;grid-row:2"><div class="home-tabs-bar"><div class="fs-tabbar"><div class="titlebar-brand has-home-brand-content">FileTerm</div><div class="titlebar-tabarea"></div></div></div><aside class="home-sidebar"></aside></div>
      <aside class="fs-sidebar"></aside>
    </div>`)
  await page.addStyleTag({ content: css })
  for (const platform of ['darwin', 'win32', 'linux']) {
    for (const sidebarWidth of [44, 190, 214, 320]) {
      const result = await page.evaluate(
        ({ platform, sidebarWidth }) => {
          document.documentElement.dataset.platform = platform
          const shell = document.querySelector('.fs-shell')
          shell.style.setProperty('--sidebar-width', `${sidebarWidth}px`)
          shell.style.setProperty('--host-rail-width', `${sidebarWidth}px`)
          shell.classList.add('is-home-active')
          const homeSidebar = parseFloat(
            getComputedStyle(document.querySelector('.home-workspace')).gridTemplateColumns
          )
          shell.classList.remove('is-home-active')
          shell.classList.add('has-host-rail')
          const hostSidebar = parseFloat(getComputedStyle(shell).gridTemplateColumns)
          shell.classList.remove('has-host-rail')
          return {
            brandWidths: [...document.querySelectorAll('.titlebar-brand')].map(
              (el) => el.getBoundingClientRect().width
            ),
            sidebarWidth: parseFloat(getComputedStyle(shell).gridTemplateColumns),
            homeSidebar,
            hostSidebar
          }
        },
        { platform, sidebarWidth }
      )
      assert.deepEqual(result.brandWidths, [214, 214], `${platform} sidebar=${sidebarWidth}`)
      assert.equal(result.sidebarWidth, sidebarWidth, `${platform} sidebar remains resizable`)
      assert.equal(result.homeSidebar, sidebarWidth, `${platform} home sidebar remains resizable`)
      assert.equal(result.hostSidebar, sidebarWidth, `${platform} host sidebar remains resizable`)
    }
  }
} finally {
  await browser.close()
}
