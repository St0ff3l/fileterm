import http from 'node:http'

const MAX_REQUEST_BYTES = 1024 * 1024
const DEFAULT_HOST = '127.0.0.1'
const DEFAULT_PORT = 9419
const DEFAULT_API_KEY = 'fileterm-fixture-key'
const DEFAULT_CHUNK_DELAY_MS = 320
const MAX_CHUNK_DELAY_MS = 10_000
const ONE_TIME_MODES_LIMIT = 200

function readPositiveInteger(name, fallback, maximum = Number.MAX_SAFE_INTEGER) {
  const value = process.env[name]
  if (!value) return fallback
  const parsed = Number.parseInt(value, 10)
  if (!Number.isSafeInteger(parsed) || parsed <= 0 || parsed > maximum) {
    throw new Error(`${name} 必须是 1 到 ${maximum} 之间的整数`)
  }
  return parsed
}

const host = process.env.FILETERM_AI_FIXTURE_HOST?.trim() || DEFAULT_HOST
const port = readPositiveInteger('FILETERM_AI_FIXTURE_PORT', DEFAULT_PORT, 65_535)
const apiKey = process.env.FILETERM_AI_FIXTURE_API_KEY?.trim() || DEFAULT_API_KEY
const chunkDelayMs = readPositiveInteger('FILETERM_AI_FIXTURE_DELAY_MS', DEFAULT_CHUNK_DELAY_MS, MAX_CHUNK_DELAY_MS)

if (host !== '127.0.0.1' && host !== '::1' && host !== 'localhost') {
  throw new Error('FILETERM_AI_FIXTURE_HOST 只能使用 loopback 地址，防止 fixture 意外暴露到网络。')
}

const completedOneTimeModes = new Set()

function log(event, details = {}) {
  // Never record user prompts or Authorization headers in this fixture. It is
  // designed for release QA, where terminal context can still be sensitive.
  console.log(`${new Date().toISOString()} ${event} ${JSON.stringify(details)}`)
}

function sendJson(response, status, payload) {
  const body = JSON.stringify(payload)
  response.writeHead(status, {
    'Cache-Control': 'no-store',
    Connection: 'close',
    'Content-Length': Buffer.byteLength(body),
    'Content-Type': 'application/json; charset=utf-8'
  })
  response.end(body)
}

function sendError(response, status, code, message) {
  sendJson(response, status, { error: { code, message } })
}

async function readJson(request) {
  let bytes = 0
  const chunks = []
  for await (const chunk of request) {
    bytes += chunk.length
    if (bytes > MAX_REQUEST_BYTES) {
      const error = new Error('请求过大')
      error.status = 413
      throw error
    }
    chunks.push(chunk)
  }

  try {
    return JSON.parse(Buffer.concat(chunks).toString('utf8'))
  } catch {
    const error = new Error('请求不是有效 JSON')
    error.status = 400
    throw error
  }
}

function contentText(value) {
  if (typeof value === 'string') return value
  if (!Array.isArray(value)) return ''
  return value
    .map((part) => {
      if (typeof part === 'string') return part
      if (part && typeof part === 'object' && typeof part.text === 'string') return part.text
      return ''
    })
    .join('')
}

function latestUserMessage(messages) {
  if (!Array.isArray(messages)) return ''
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index]
    if (message?.role === 'user') return contentText(message.content)
  }
  return ''
}

function isCommandProposal(messages) {
  return (
    Array.isArray(messages) &&
    messages.some((message) => {
      return (
        message?.role === 'system' &&
        typeof message.content === 'string' &&
        message.content.includes('Return exactly one JSON object')
      )
    })
  )
}

function requestedMode(prompt, commandProposal) {
  if (commandProposal && /fixture:multiline\b/i.test(prompt)) {
    return 'multiline-command'
  }
  if (commandProposal) {
    // Command-proposal mode is selected by the request envelope, not by a
    // magic phrase in the user message. This lets QA cover the real flow
    // where a user first asks for an explanation and then sends "重新来".
    return 'command'
  }
  if (/fixture:(command|multiline)\b/i.test(prompt)) {
    return /fixture:multiline\b/i.test(prompt) ? 'multiline-command' : 'command'
  }
  if (/fixture:fail-once\b/i.test(prompt)) return 'fail-once'
  if (/fixture:disconnect-once\b/i.test(prompt)) return 'disconnect-once'
  if (/fixture:markdown\b/i.test(prompt)) return 'markdown'
  if (/fixture:slow\b/i.test(prompt)) return 'slow'
  return 'normal'
}

function responseText(mode) {
  if (mode === 'command') {
    return JSON.stringify({
      answer: 'Fixture prepared a read-only command card. Review it before using it.',
      commands: [
        {
          command: 'pwd',
          explanation: 'Prints the current working directory without changing the remote host.',
          risk: 'read-only'
        }
      ]
    })
  }
  if (mode === 'multiline-command') {
    return JSON.stringify({
      answer: 'Fixture prepared a multi-line command card. It must remain unavailable for one-click terminal input.',
      commands: [
        {
          command: "printf '%s\\n' fixture-one\nprintf '%s\\n' fixture-two",
          explanation: 'A deterministic multi-line fixture command.',
          risk: 'read-only'
        }
      ]
    })
  }
  if (mode === 'slow') {
    return Array.from({ length: 40 }, (_, index) => `fixture-stream-${index + 1} `).join('')
  }
  if (mode === 'markdown') {
    return [
      '# Fixture Markdown',
      '',
      'This reply has **bold text**, `inline code`, and a [safe external link](https://example.com).',
      '',
      '- stream as ordinary Markdown',
      '- keep code separate from command cards',
      '',
      '> Provider HTML and non-HTTP(S) links must not become active content.',
      '',
      '```sh',
      "printf '%s\\n' fixture-markdown",
      '```',
      '',
      '| Surface | Expected behavior |',
      '| --- | --- |',
      '| Raw HTML | omitted |',
      '| `javascript:` link | inactive |',
      '',
      '<img src="https://example.com/fixture.png" onerror="window.filetermMarkdownExecuted = true">',
      '[unsafe link](javascript:window.filetermMarkdownExecuted=true)'
    ].join('\n')
  }
  return 'Fixture response received. Streaming, usage reporting, and local history can be verified with this deterministic reply.'
}

function splitForStreaming(text, mode) {
  if (mode === 'slow') {
    return text.match(/.{1,18}/g) ?? [text]
  }
  return text.match(/.{1,32}/g) ?? [text]
}

function oneTimeKey(mode, prompt) {
  return `${mode}:${prompt.slice(0, 512)}`
}

function markOneTimeMode(key) {
  completedOneTimeModes.add(key)
  if (completedOneTimeModes.size <= ONE_TIME_MODES_LIMIT) return
  completedOneTimeModes.delete(completedOneTimeModes.values().next().value)
}

function writeSse(response, payload) {
  response.write(`data: ${JSON.stringify(payload)}\n\n`)
}

function streamCompletion(request, response, { mode, model, promptLength }) {
  const text = responseText(mode)
  const chunks = splitForStreaming(text, mode)
  let index = 0
  let closed = false
  let timer = null

  const cleanUp = () => {
    closed = true
    if (timer) clearTimeout(timer)
  }

  request.once('close', cleanUp)
  response.once('close', cleanUp)
  response.writeHead(200, {
    'Cache-Control': 'no-cache, no-transform',
    Connection: 'keep-alive',
    'Content-Type': 'text/event-stream; charset=utf-8',
    'X-Accel-Buffering': 'no'
  })
  response.flushHeaders()

  const emitNext = () => {
    if (closed || response.writableEnded) return

    if (mode === 'disconnect-once' && index === 1) {
      log('fixture-stream-disconnected', { mode, promptLength })
      response.destroy()
      return
    }

    if (index < chunks.length) {
      writeSse(response, {
        choices: [{ delta: { content: chunks[index] }, finish_reason: null, index: 0 }],
        id: 'fileterm-fixture-stream',
        model,
        object: 'chat.completion.chunk'
      })
      index += 1
      timer = setTimeout(emitNext, mode === 'slow' ? chunkDelayMs : Math.min(chunkDelayMs, 80))
      return
    }

    writeSse(response, {
      choices: [{ delta: {}, finish_reason: 'stop', index: 0 }],
      id: 'fileterm-fixture-stream',
      model,
      object: 'chat.completion.chunk',
      usage: { completion_tokens: Math.max(1, chunks.length), prompt_tokens: Math.max(1, Math.ceil(promptLength / 4)) }
    })
    response.write('data: [DONE]\n\n')
    response.end()
    log('fixture-stream-completed', { mode, promptLength })
  }

  emitNext()
}

function handleCompletion(request, response, payload) {
  const model = typeof payload?.model === 'string' && payload.model.trim() ? payload.model.trim() : 'fileterm-fixture'
  const prompt = latestUserMessage(payload?.messages)
  const commandProposal = isCommandProposal(payload?.messages)
  const mode = requestedMode(prompt, commandProposal)
  const onceKey = oneTimeKey(mode, prompt)
  const firstAttempt = !completedOneTimeModes.has(onceKey)

  log('fixture-request', {
    commandProposal,
    mode,
    promptLength: prompt.length,
    stream: Boolean(payload?.stream)
  })

  if (mode === 'fail-once' && firstAttempt) {
    markOneTimeMode(onceKey)
    sendError(
      response,
      503,
      'fixture-temporary-failure',
      'Fixture intentionally failed once. Retry the same message to continue.'
    )
    return
  }

  if (mode === 'disconnect-once' && firstAttempt) {
    markOneTimeMode(onceKey)
  }

  if (!payload?.stream) {
    sendJson(response, 200, {
      choices: [{ finish_reason: 'stop', index: 0, message: { content: 'OK', role: 'assistant' } }],
      id: 'fileterm-fixture-test',
      model,
      object: 'chat.completion',
      usage: { completion_tokens: 1, prompt_tokens: 3, total_tokens: 4 }
    })
    return
  }

  streamCompletion(request, response, { mode, model, promptLength: prompt.length })
}

const server = http.createServer(async (request, response) => {
  const url = new URL(request.url ?? '/', `http://${host}:${port}`)
  if (request.method === 'GET' && url.pathname === '/healthz') {
    sendJson(response, 200, { ok: true, service: 'fileterm-ai-copilot-fixture' })
    return
  }
  if (request.method !== 'POST' || url.pathname !== '/v1/chat/completions') {
    sendError(response, 404, 'not-found', 'Only GET /healthz and POST /v1/chat/completions are available.')
    return
  }

  if (request.headers.authorization !== `Bearer ${apiKey}`) {
    log('fixture-unauthorized', { hasAuthorization: Boolean(request.headers.authorization) })
    sendError(response, 401, 'invalid-api-key', 'Use the QA-only fixture API key configured for this local process.')
    return
  }

  try {
    handleCompletion(request, response, await readJson(request))
  } catch (error) {
    const status = typeof error?.status === 'number' ? error.status : 400
    sendError(response, status, 'invalid-request', error instanceof Error ? error.message : 'Invalid request')
  }
})

server.on('clientError', (error, socket) => {
  log('fixture-client-error', { code: error.code ?? 'unknown' })
  socket.end('HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n')
})

server.listen(port, host, () => {
  log('fixture-listening', {
    baseUrl: `http://${host}:${port}/v1`,
    chunkDelayMs,
    endpoint: '/v1/chat/completions',
    health: `http://${host}:${port}/healthz`
  })
})

function shutDown(signal) {
  log('fixture-stopping', { signal })
  server.close(() => process.exit(0))
  setTimeout(() => process.exit(0), 2_000).unref()
}

process.once('SIGINT', () => shutDown('SIGINT'))
process.once('SIGTERM', () => shutDown('SIGTERM'))
