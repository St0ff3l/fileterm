import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import http from 'node:http'
import process from 'node:process'

const repoRoot = new URL('../', import.meta.url)
const fixtureScript = new URL('./ai-copilot-fixture-provider.mjs', import.meta.url)
const apiKey = 'fileterm-fixture-key'

async function reserveLoopbackPort() {
  const server = http.createServer()
  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', resolve)
  })
  const address = server.address()
  assert(address && typeof address === 'object' && address.port > 0, '无法取得 fixture loopback 端口')
  await new Promise((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())))
  return address.port
}

async function waitForHealth(url, child) {
  const deadline = Date.now() + 5_000
  let lastError = null
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`AI fixture 在启动时退出，exit code=${child.exitCode}`)
    }
    try {
      const response = await fetch(`${url}/healthz`, { signal: AbortSignal.timeout(500) })
      if (response.ok) return
      lastError = new Error(`healthz 返回 HTTP ${response.status}`)
    } catch (error) {
      lastError = error
    }
    await new Promise((resolve) => setTimeout(resolve, 50))
  }
  throw new Error(`AI fixture 未在超时内启动: ${lastError instanceof Error ? lastError.message : lastError}`)
}

async function readSseText(response) {
  assert(response.body, 'fixture SSE 响应缺少 body')
  const decoder = new TextDecoder()
  let buffer = ''
  let text = ''

  for await (const chunk of response.body) {
    buffer += decoder.decode(chunk, { stream: true })
    let separatorIndex = buffer.indexOf('\n\n')
    while (separatorIndex >= 0) {
      const frame = buffer.slice(0, separatorIndex)
      buffer = buffer.slice(separatorIndex + 2)
      for (const line of frame.split('\n')) {
        if (!line.startsWith('data: ')) continue
        const data = line.slice('data: '.length)
        if (data === '[DONE]') return text
        const payload = JSON.parse(data)
        text += payload.choices?.[0]?.delta?.content ?? ''
      }
      separatorIndex = buffer.indexOf('\n\n')
    }
  }

  return text
}

async function sendChat(url, messages, stream = true, extra = {}) {
  const response = await fetch(`${url}/v1/chat/completions`, {
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/json'
    },
    method: 'POST',
    body: JSON.stringify({ messages, model: 'fileterm-fixture', stream, ...extra })
  })
  const body = stream && response.ok ? await readSseText(response) : await response.text()
  return { body, response }
}

async function waitForExit(child) {
  if (child.exitCode !== null) return
  await new Promise((resolve) => child.once('exit', resolve))
}

const port = await reserveLoopbackPort()
const baseUrl = `http://127.0.0.1:${port}`
const child = spawn(process.execPath, [fixtureScript.pathname], {
  cwd: repoRoot,
  env: { ...process.env, FILETERM_AI_FIXTURE_PORT: String(port) },
  stdio: ['ignore', 'pipe', 'pipe']
})
let stderr = ''
child.stderr.setEncoding('utf8')
child.stderr.on('data', (chunk) => {
  stderr += chunk
})

try {
  await waitForHealth(baseUrl, child)

  const retryMessages = [{ content: 'fixture:fail-once', role: 'user' }]
  const failed = await sendChat(baseUrl, retryMessages, false)
  assert.equal(failed.response.status, 503, 'fail-once fixture 没有模拟首个失败')
  const recovered = await sendChat(baseUrl, retryMessages)
  assert.equal(recovered.response.status, 200, '重试请求没有恢复')
  assert.match(recovered.body, /Fixture response received/)

  const toolMessages = [
    { content: 'The current request uses the FileTerm tool contract.', role: 'system' },
    { content: 'fixture:tool-compat', role: 'user' }
  ]
  const toolCallResponse = await fetch(`${baseUrl}/v1/chat/completions`, {
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/json'
    },
    method: 'POST',
    body: JSON.stringify({
      messages: toolMessages,
      model: 'fileterm-fixture',
      stream: true,
      tools: [{ type: 'function' }]
    })
  })
  const toolCallBody = await toolCallResponse.text()
  assert.equal(toolCallResponse.status, 200, 'tool-call fixture 请求失败')
  assert.match(toolCallBody, /fileterm_execute_remote_command/, 'tool-call fixture 没有返回 FileTerm tool')
  assert.doesNotMatch(toolCallBody, /commands/, 'tool-call 请求不应退回旧命令卡 envelope')

  const toolResult = await sendChat(
    baseUrl,
    [
      ...toolMessages,
      {
        content: null,
        role: 'assistant',
        tool_calls: [
          {
            id: 'call-fileterm-fixture',
            type: 'function',
            function: { name: 'fileterm_execute_remote_command', arguments: '{"command":"id -u"}' }
          }
        ]
      },
      { content: '0\\n', role: 'tool', tool_call_id: 'call-fileterm-fixture' }
    ],
    true,
    { tools: [{ type: 'function' }] }
  )
  assert.equal(toolResult.response.status, 200, 'tool-call fixture 二次请求失败')
  assert.match(toolResult.body, /Fixture tool loop completed/, 'tool-call fixture 没有消费 tool result')

  const sudoToolResponse = await fetch(`${baseUrl}/v1/chat/completions`, {
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/json'
    },
    method: 'POST',
    body: JSON.stringify({
      messages: [{ content: 'fixture:tool-sudo', role: 'user' }],
      model: 'fileterm-fixture',
      stream: true
    })
  })
  const sudoToolBody = await sudoToolResponse.text()
  assert.equal(sudoToolResponse.status, 200, 'sudo tool-call fixture 请求失败')
  assert.match(sudoToolBody, /sudo id -u/, 'sudo tool-call fixture 没有返回提权命令')

  console.log('AI Copilot fixture smoke passed: retry recovery, tool-call loop, and sudo tool contract')
} catch (error) {
  const detail = stderr.trim()
  throw new Error(
    `${error instanceof Error ? error.message : error}${detail ? `\nfixture stderr:\n${detail}` : ''}`,
    error instanceof Error ? { cause: error } : undefined
  )
} finally {
  if (child.exitCode === null) {
    child.kill('SIGTERM')
    await waitForExit(child)
  }
}
