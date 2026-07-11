export const CLINK_AUTOSUGGEST_HELP_URL = 'https://chrisant996.github.io/clink/clink.html#gettingstarted_autosuggest'

const CLINK_AUTOSUGGEST_PROMPT_PATTERN = new RegExp(
  '(?:\\u001b\\[[0-9;?]*[ -/]*[@-~]){1,10}F2' +
    '(?:\\u001b\\[[0-9;?]*[ -/]*[@-~])*[-=]\\s*' +
    '(?:\\u001b\\]8;[^\\u0007\\u001b]*(?:\\u0007|\\u001b\\\\))?' +
    'List Suggestions' +
    '(?:\\u001b\\]8;[^\\u0007\\u001b]*(?:\\u0007|\\u001b\\\\))?' +
    '(?:\\u001b\\[(?:\\d+;\\d+H|\\d+G))?',
  'g'
)

export function isClinkAutosuggestHelpUrl(uri: string) {
  return uri.startsWith(CLINK_AUTOSUGGEST_HELP_URL)
}

export function stripClinkAutosuggestPrompt(value: string) {
  return value.replace(CLINK_AUTOSUGGEST_PROMPT_PATTERN, (prompt) => {
    const resetStyle = prompt.includes('\u001b[0m') ? '\u001b[0m' : prompt.includes('\u001b[m') ? '\u001b[m' : ''
    const eraseRightPrompt = extractAnsiEraseCharacters(prompt)
    return `${resetStyle}${eraseRightPrompt}`
  })
}

function extractAnsiEraseCharacters(value: string) {
  let result = ''
  let searchFrom = 0

  while (searchFrom < value.length) {
    const eraseEnd = value.indexOf('X', searchFrom)
    if (eraseEnd < 0) {
      break
    }
    const sequenceStart = value.lastIndexOf('\u001b[', eraseEnd)
    const count = sequenceStart >= searchFrom ? value.slice(sequenceStart + 2, eraseEnd) : ''
    if (/^\d+$/.test(count)) {
      result += value.slice(sequenceStart, eraseEnd + 1)
    }
    searchFrom = eraseEnd + 1
  }

  return result
}

export function trimHydratedTerminalChunk(currentTranscript: string, chunk: string): string {
  if (!currentTranscript || !chunk) {
    return chunk
  }
  if (currentTranscript.endsWith(chunk)) {
    return ''
  }

  const suffix = currentTranscript.slice(-Math.min(currentTranscript.length, chunk.length))
  const patternLength = chunk.length
  const separatorIndex = patternLength
  const sequenceLength = patternLength + 1 + suffix.length
  const prefixLengths = new Uint32Array(sequenceLength)
  const symbolAt = (index: number) => {
    if (index < patternLength) {
      return chunk.charCodeAt(index)
    }
    if (index === separatorIndex) {
      return -1
    }
    return suffix.charCodeAt(index - separatorIndex - 1)
  }

  for (let index = 1; index < sequenceLength; index += 1) {
    let candidateLength = prefixLengths[index - 1] ?? 0
    while (candidateLength > 0 && symbolAt(index) !== symbolAt(candidateLength)) {
      candidateLength = prefixLengths[candidateLength - 1] ?? 0
    }
    if (symbolAt(index) === symbolAt(candidateLength)) {
      candidateLength += 1
    }
    prefixLengths[index] = candidateLength
  }

  const overlapLength = prefixLengths[sequenceLength - 1] ?? 0
  // A one- or two-character match is common terminal output, not reliable
  // evidence that an IPC batch was already included in a hydrated snapshot.
  // Full chunks are handled above; partial overlap must contain enough context
  // (normally a complete prompt) before it is safe to trim.
  return overlapLength >= 8 ? chunk.slice(overlapLength) : chunk
}
