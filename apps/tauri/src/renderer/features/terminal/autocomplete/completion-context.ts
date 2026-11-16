export function hasControlCharacters(value: string) {
  return Array.from(value).some((character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127)
}

export type CompletionContext = {
  tokens: string[]
  token: string
  prefix: string
  rawToken: string
  quote: string
  commandIndex: number
}

export function completionContext(input: string): CompletionContext | null {
  if (!input || hasControlCharacters(input)) return null
  const tokens: Array<{ value: string; start: number }> = []
  let value = ''
  let start = 0
  let quote = ''
  let started = false
  for (let index = 0; index < input.length; index++) {
    const character = input[index]
    if (!started && character !== ' ') {
      start = index
      started = true
    }
    if (character === '\\' && quote !== "'") {
      if (index + 1 >= input.length) return null
      value += input[++index]
    } else if (character === quote) {
      quote = ''
    } else if ((character === '"' || character === "'") && !quote) {
      quote = character
    } else if (!quote && character === ' ') {
      if (started) tokens.push({ value, start })
      value = ''
      started = false
      start = index + 1
    } else if (!quote && ';|&'.includes(character)) {
      if (character !== ';' && input[index + 1] === character) index++
      tokens.length = 0
      value = ''
      started = false
      start = index + 1
    } else if ((quote !== "'" && '$`'.includes(character)) || (!quote && '<>(){}*?[]'.includes(character))) {
      return null
    } else {
      value += character
    }
  }
  tokens.push({ value, start: started ? start : input.length })
  let commandIndex = 0
  while (commandIndex < tokens.length - 1 && /^[A-Za-z_][\w]*=/.test(tokens[commandIndex].value)) commandIndex++
  wrappers: while (['sudo', 'doas', 'env', 'command', 'nohup'].includes(tokens[commandIndex]?.value)) {
    const wrapperIndex = commandIndex
    const wrapper = tokens[commandIndex++].value
    while (commandIndex < tokens.length - 1) {
      const token = tokens[commandIndex].value
      if (wrapper === 'env' && /^[A-Za-z_][\w]*=/.test(token)) {
        commandIndex++
        continue
      }
      if (!token.startsWith('-')) break
      commandIndex++
      if (['-u', '-g', '-h', '-p', '-C', '-T', '--user', '--group', '--host', '--prompt', '--chdir'].includes(token)) {
        if (commandIndex >= tokens.length - 1) {
          commandIndex = wrapperIndex
          break wrappers
        }
        commandIndex++
      }
      if (token === '--') break
    }
  }
  commandIndex = Math.min(commandIndex, tokens.length - 1)
  const last = tokens.at(-1)!
  return {
    tokens: tokens.map((token) => token.value),
    token: last.value,
    prefix: input.slice(0, last.start),
    rawToken: input.slice(last.start),
    quote,
    commandIndex
  }
}

export function completeToken(input: string, context: CompletionContext, value: string): string | null {
  if (hasControlCharacters(value) || !value.startsWith(context.token)) return null
  let suffix = value.slice(context.token.length)
  if (context.quote === "'") suffix = suffix.replaceAll("'", "'\\''")
  else if (context.quote === '"') suffix = suffix.replace(/[$`"\\]/gu, '\\$&')
  else suffix = suffix.replace(/[^\p{L}\p{M}\p{N}_.,+/@%:=~-]/gu, '\\$&')
  return input + suffix + (context.quote && !value.endsWith('/') ? context.quote : '')
}
