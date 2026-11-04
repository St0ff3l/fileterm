import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/box-drawing.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports }
)
const { getBoxDrawingRects, drawBoxDrawingGlyph, isBoxDrawingGlyph } = exports
const rects = (glyph, width = 9, height = 17, thickness = 1) =>
  getBoxDrawingRects(glyph.codePointAt(0), width, height, thickness)
const ink = (rectangles, x, y) =>
  rectangles.some((r) => x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)

test('all straight box characters have bounded geometry at odd/even device sizes', () => {
  for (const [width, height, thickness] of [
    [9, 17, 1],
    [11, 21, 2],
    [12, 24, 2],
    [18, 36, 3]
  ]) {
    for (let cp = 0x2500; cp <= 0x257f; cp++) {
      assert.equal(isBoxDrawingGlyph(cp), true)
      if (cp >= 0x256d && cp <= 0x2573) continue
      const rectangles = getBoxDrawingRects(cp, width, height, thickness)
      assert.ok(rectangles.length, `U+${cp.toString(16)} has ink`)
      for (const r of rectangles) {
        assert.ok(r.x >= 0 && r.y >= 0 && r.width > 0 && r.height > 0)
        assert.ok(r.x + r.width <= width && r.y + r.height <= height)
        assert.ok(Object.values(r).every(Number.isInteger), 'straight strokes use device pixels')
      }
    }
  }
  assert.equal(isBoxDrawingGlyph(0x2580), false)
  assert.equal(isBoxDrawingGlyph(0x24ff), false)
})

test('table corners, tees and crosses share the straight rule edge profiles', () => {
  for (const [glyph, directions] of [
    ['┌', 'RD'],
    ['┐', 'LD'],
    ['└', 'RU'],
    ['┘', 'LU'],
    ['├', 'URD'],
    ['┤', 'ULD'],
    ['┬', 'LRD'],
    ['┴', 'LRU'],
    ['┼', 'ULRD'],
    ['╔', 'RD'],
    ['╗', 'LD'],
    ['╚', 'RU'],
    ['╝', 'LU'],
    ['╠', 'URD'],
    ['╣', 'ULD'],
    ['╦', 'LRD'],
    ['╩', 'LRU'],
    ['╬', 'ULRD']
  ]) {
    const doubled = glyph.codePointAt(0) >= 0x2550
    const horizontal = rects(doubled ? '═' : '─')
    const vertical = rects(doubled ? '║' : '│')
    const rectangles = rects(glyph)
    for (let y = 0; y < 17; y++) {
      assert.equal(ink(rectangles, 0, y), directions.includes('L') && ink(horizontal, 0, y), `${glyph} left`)
      assert.equal(ink(rectangles, 8, y), directions.includes('R') && ink(horizontal, 8, y), `${glyph} right`)
    }
    for (let x = 0; x < 9; x++) {
      assert.equal(ink(rectangles, x, 0), directions.includes('U') && ink(vertical, x, 0), `${glyph} up`)
      assert.equal(ink(rectangles, x, 16), directions.includes('D') && ink(vertical, x, 16), `${glyph} down`)
    }
  }
})

test('double elbows and crossings preserve separate rails and the central channel', () => {
  const elbow = rects('╔')
  for (const [x, y] of [
    [3, 7],
    [4, 7],
    [3, 8],
    [5, 9],
    [6, 9],
    [5, 10]
  ])
    assert.ok(ink(elbow, x, y))
  for (const [x, y] of [
    [4, 8],
    [4, 9],
    [5, 8],
    [2, 7],
    [3, 6]
  ])
    assert.equal(ink(elbow, x, y), false)
  const cross = rects('╬')
  assert.equal(ink(cross, 4, 8), false)
  for (const [x, y] of [
    [3, 7],
    [5, 7],
    [3, 9],
    [5, 9]
  ])
    assert.ok(ink(cross, x, y))
  const mixed = rects('╪')
  assert.ok(ink(mixed, 4, 8), 'single vertical line continues between double horizontal rails')
})

test('mixed weights and half lines keep their Unicode directions', () => {
  const mixed = rects('┍')
  assert.equal(ink(mixed, 4, 16), true)
  assert.equal(ink(mixed, 5, 16), false, 'down is light')
  assert.equal(ink(mixed, 8, 7), true)
  assert.equal(ink(mixed, 8, 8), true, 'right is heavy')
  assert.equal(ink(rects('╴'), 0, 8), true)
  assert.equal(ink(rects('╴'), 8, 8), false)
  assert.equal(ink(rects('╷'), 4, 0), false)
  assert.equal(ink(rects('╷'), 4, 16), true)
  assert.equal(ink(rects('╽'), 3, 0), false, 'upper arm is light')
  assert.equal(ink(rects('╽'), 3, 16), true, 'lower arm is heavy')
  assert.equal(ink(rects('╿'), 3, 0), true, 'upper arm is heavy')
  assert.equal(ink(rects('╿'), 3, 16), false, 'lower arm is light')
})

test('dash characters retain intentional gaps and the requested segment count', () => {
  for (const [glyph, count] of [
    ['╌', 2],
    ['┄', 3],
    ['┈', 4],
    ['╎', 2],
    ['┆', 3],
    ['┊', 4]
  ]) {
    const rectangles = rects(glyph, 24, 32)
    assert.equal(rectangles.length, count)
    const vertical = ['╎', '┆', '┊'].includes(glyph)
    for (let i = 1; i < rectangles.length; i++) {
      const previous = rectangles[i - 1]
      assert.ok(
        vertical ? rectangles[i].y > previous.y + previous.height : rectangles[i].x > previous.x + previous.width
      )
    }
  }
})

test('transparent intersections are filled once; rounded/diagonal paths use strokes', () => {
  let fills = 0,
    strokes = 0,
    curves = 0
  const context = {
    beginPath() {},
    rect() {},
    moveTo() {},
    lineTo() {},
    fill() {
      fills++
    },
    stroke() {
      strokes++
    },
    quadraticCurveTo() {
      curves++
    }
  }
  drawBoxDrawingGlyph(context, '┼'.codePointAt(0), 0, 9, 17, 1)
  assert.equal(fills, 1)
  for (const glyph of '╭╮╯╰╱╲╳') drawBoxDrawingGlyph(context, glyph.codePointAt(0), 0, 9, 17, 1)
  assert.equal(strokes, 7)
  assert.equal(curves, 4)
})
