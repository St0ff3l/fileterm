import { spawn } from 'node:child_process'

const binary = process.argv[2] || process.env.FILETERM_MCP_BINARY
const REQUEST_TIMEOUT_MS = 10_000

if (!binary) {
  console.error('Usage: node scripts/mcp-stdio-smoke.mjs <fileterm-binary>')
  process.exitCode = 2
} else {
  await runSmoke(binary)
}

async function runSmoke(binaryPath) {
  const child = spawn(binaryPath, ['mcp'], {
    stdio: ['pipe', 'pipe', 'pipe']
  })
  let stdoutBuffer = ''
  let stderrBuffer = ''
  const pending = new Map()
  let nextRequestId = 1
  let closeResolve
  let closeReject
  const closePromise = new Promise((resolve, reject) => {
    closeResolve = resolve
    closeReject = reject
  })

  child.stdout.setEncoding('utf8')
  child.stderr.setEncoding('utf8')
  child.stdout.on('data', (chunk) => {
    stdoutBuffer += chunk
    while (true) {
      const newline = stdoutBuffer.indexOf('\n')
      if (newline < 0) break
      const line = stdoutBuffer.slice(0, newline).trim()
      stdoutBuffer = stdoutBuffer.slice(newline + 1)
      if (!line) continue
      let message
      try {
        message = JSON.parse(line)
      } catch (error) {
        rejectPending(new Error(`MCP stdout is not JSON: ${line}`, { cause: error }))
        continue
      }
      if (message.id === undefined || message.id === null) continue
      const waiter = pending.get(String(message.id))
      if (!waiter) continue
      pending.delete(String(message.id))
      clearTimeout(waiter.timer)
      waiter.resolve(message)
    }
  })
  child.stderr.on('data', (chunk) => {
    stderrBuffer += chunk
    if (stderrBuffer.length > 4_000) stderrBuffer = stderrBuffer.slice(-4_000)
  })
  child.on('error', (error) => {
    rejectPending(error)
    closeReject(error)
  })
  child.on('close', (code, signal) => {
    if (code === 0) {
      closeResolve({ code, signal })
    } else {
      const detail = stderrBuffer.trim()
      closeReject(new Error(`MCP process exited with code=${code} signal=${signal}${detail ? `: ${detail}` : ''}`))
    }
    rejectPending(new Error('MCP process closed before returning a response'))
  })

  try {
    const initialize = await request(child, pending, nextRequestId++, {
      jsonrpc: '2.0',
      method: 'initialize',
      params: {
        protocolVersion: '2025-03-26',
        capabilities: {},
        clientInfo: { name: 'fileterm-package-smoke', version: '1.0.0' }
      }
    })
    assert(initialize.result?.protocolVersion, 'initialize did not negotiate a protocol version')
    assert(initialize.result?.serverInfo?.name === 'fileterm-mcp-server', 'unexpected MCP server name')

    const toolsList = await request(child, pending, nextRequestId++, {
      jsonrpc: '2.0',
      method: 'tools/list',
      params: {}
    })
    const tools = toolsList.result?.tools
    assert(Array.isArray(tools), 'tools/list did not return a tool array')
    const byName = new Map(tools.map((tool) => [tool.name, tool]))
    const requiredTools = [
      'fileterm_list_connections',
      'fileterm_execute_remote_command',
      'fileterm_execute_interactive_remote_command'
    ]
    for (const name of requiredTools) {
      assert(byName.has(name), `tools/list is missing ${name}`)
    }

    const remoteProperties = byName.get('fileterm_execute_remote_command').inputSchema?.properties ?? {}
    assert(remoteProperties.sudo_password, 'remote exec schema is missing sudo_password')
    assert(remoteProperties.su_password, 'remote exec schema is missing su_password')

    const interactiveTool = byName.get('fileterm_execute_interactive_remote_command')
    const interactiveProperties = interactiveTool.inputSchema?.properties ?? {}
    for (const forbidden of ['stdin', 'password', 'sudo_password', 'su_password', 'answers']) {
      assert(!Object.hasOwn(interactiveProperties, forbidden), `interactive exec exposes forbidden ${forbidden}`)
    }
    assert(
      /安全|secret|password/i.test(interactiveTool.description ?? ''),
      'interactive exec description does not describe local secure input'
    )

    child.stdin.end()
    await closePromise
    console.log(`MCP stdio smoke passed: ${tools.length} tools, initialize/tools-list/schema checks`)
  } catch (error) {
    child.kill()
    process.exitCode = 1
    console.error(error instanceof Error ? error.message : error)
  }

  function rejectPending(error) {
    for (const waiter of pending.values()) {
      clearTimeout(waiter.timer)
      waiter.reject(error)
    }
    pending.clear()
  }
}

function request(child, pending, id, message) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(String(id))
      reject(new Error(`MCP request ${message.method} timed out after ${REQUEST_TIMEOUT_MS}ms`))
    }, REQUEST_TIMEOUT_MS)
    pending.set(String(id), { resolve, reject, timer })
    child.stdin.write(`${JSON.stringify({ ...message, id })}\n`)
  })
}

function assert(condition, message) {
  if (!condition) throw new Error(message)
}
