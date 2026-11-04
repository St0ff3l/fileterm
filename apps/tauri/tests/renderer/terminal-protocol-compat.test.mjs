import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function load(name) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(`../../src/renderer/xterm-adapters/${name}.ts`, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    { exports }
  )
  return exports
}

test('historical query replies are removed while user keys and bracketed paste stay intact', () => {
  const { isTerminalResponseSequence, stripHydratedTerminalResponses } = load('terminal-responses')
  const escape = '\x1b'
  const replies = ['\x1b[12;40R', '\x1b[?1;2c', '\x1b]11;rgb:ffff/ffff/ffff\x07', '\x1bP1$r0m\x1b\\']
  for (const reply of replies) {
    assert.equal(isTerminalResponseSequence(reply, escape), true)
    assert.equal(stripHydratedTerminalResponses(`a${reply}b`), 'ab')
  }
  for (const input of ['hello', '\x1b[A', '\x1b[200~paste\x1b[201~', '\x1b[I', '\x1b[O']) {
    assert.equal(stripHydratedTerminalResponses(input), input)
    assert.equal(isTerminalResponseSequence(input, escape), false)
  }
})

test('long native snapshots replay from a complete boundary and short transcripts retain exact content', () => {
  const { trimTranscript } = load('terminal-transcript-replay')
  const tail = 'x'.repeat(180000)
  const replay = trimTranscript(`31mfragment\r\n${tail}`)
  assert.equal(replay.length, tail.length + 1)
  assert.ok(replay === `\n${tail}`, 'the remainder starts after a complete CR control boundary')
  assert.ok(trimTranscript(`31mfragment\x1b[32m${tail}`) === `\x1b[32m${tail}`)
  assert.equal(trimTranscript('short line'), 'short line')
  assert.equal(trimTranscript('x'.repeat(210000)).length, 200000, 'a long unbroken line keeps the bounded fallback')
})
