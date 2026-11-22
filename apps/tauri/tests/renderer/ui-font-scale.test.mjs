import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function loadUiFontScale(properties) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL('../../src/renderer/app/ui-font-scale.ts', import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require: () => ({
        DEFAULT_UI_FONT_SCALE_PERCENT: 100,
        DEFAULT_UI_ZOOM_PERCENT: 100,
        MAX_EFFECTIVE_UI_TEXT_SCALE_PERCENT: 200,
        MIN_UI_FONT_SCALE_PERCENT: 80,
        MAX_UI_FONT_SCALE_PERCENT: 150,
        MIN_UI_ZOOM_PERCENT: 80,
        MAX_UI_ZOOM_PERCENT: 200
      }),
      document: {
        documentElement: {
          style: { setProperty: (name, value) => properties.set(name, value) }
        }
      }
    }
  )
  return exports.applyUiFontScalePercent
}

test('UI font scale updates fixed-size tokens and clamps invalid ranges', () => {
  const properties = new Map()
  const applyUiFontScalePercent = loadUiFontScale(properties)

  applyUiFontScalePercent(150, 100)
  assert.equal(properties.get('--ui-font-size-9-5px'), '14.25px')
  assert.equal(properties.get('--ui-font-size-12px'), '18px')

  applyUiFontScalePercent(150, 150)
  assert.equal(properties.get('--ui-font-size-12px'), '16px')
  applyUiFontScalePercent(150, 100)
  assert.equal(properties.get('--ui-font-size-12px'), '18px')

  applyUiFontScalePercent(250, 100)
  assert.equal(properties.get('--ui-font-size-12px'), '18px')

  applyUiFontScalePercent(150, 250)
  assert.equal(properties.get('--ui-font-size-12px'), '12px')

  applyUiFontScalePercent(Number.NaN, 100)
  assert.equal(properties.get('--ui-font-size-12px'), '12px')
})
