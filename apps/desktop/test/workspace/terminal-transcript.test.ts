import assert from 'node:assert/strict'
import test from 'node:test'
import {
  CLINK_AUTOSUGGEST_HELP_URL,
  isClinkAutosuggestHelpUrl,
  stripClinkAutosuggestPrompt,
  trimHydratedTerminalChunk
} from '../../src/renderer/app/terminal-transcript.ts'

test('removes a fully hydrated terminal chunk', () => {
  assert.equal(trimHydratedTerminalChunk('banner\r\nprompt$ ', 'prompt$ '), '')
})

test('keeps only the non-hydrated tail of a partially overlapping chunk', () => {
  assert.equal(trimHydratedTerminalChunk('banner\r\nprompt$ ', 'prompt$ command\r\n'), 'command\r\n')
})

test('preserves chunks that do not overlap hydrated output', () => {
  assert.equal(trimHydratedTerminalChunk('prompt$ ', '\u001b[32mresult\u001b[0m\r\n'), '\u001b[32mresult\u001b[0m\r\n')
})

test('does not trim a coincidental short overlap from real output', () => {
  assert.equal(trimHydratedTerminalChunk('job: ok\nabc', 'abcdef\n'), 'abcdef\n')
})

test('handles ANSI and CRLF overlap split across IPC batches', () => {
  const current = 'login\r\n\u001b[32mstoffel@debian\u001b[0m:~$ '
  const chunk = '\u001b[32mstoffel@debian\u001b[0m:~$ printf ok\r\nok\r\n'
  assert.equal(trimHydratedTerminalChunk(current, chunk), 'printf ok\r\nok\r\n')
})

test('removes the Clink F2 autosuggestion help prompt without removing typed input', () => {
  const chunk =
    '\u001b[?25l\u001b[38;5;203mip\u001b[59X\u001b[90m\u001b[7m\u001b[59CF2\u001b[27m=' +
    `\u001b]8;id=19712-1;${CLINK_AUTOSUGGEST_HELP_URL}\u001b\\List Suggestions\u001b[9;42H\u001b[?25h`

  assert.equal(stripClinkAutosuggestPrompt(chunk), '\u001b[?25l\u001b[38;5;203mip\u001b[59X\u001b[?25h')
  assert.equal(isClinkAutosuggestHelpUrl(CLINK_AUTOSUGGEST_HELP_URL), true)
  assert.equal(isClinkAutosuggestHelpUrl('https://example.com'), false)
})

test('removes the Clink 1.9.25 right prompt emitted by the Windows host', () => {
  const chunk =
    '\u001b[m\u001b[90m\u001b[53X\u001b[7m\u001b[53CF2\u001b[27m=' +
    `\u001b]8;id=35172-1;${CLINK_AUTOSUGGEST_HELP_URL}\u001b\\List Suggestions\u001b[15;48H`

  assert.equal(stripClinkAutosuggestPrompt(chunk), '\u001b[m\u001b[53X')
})
