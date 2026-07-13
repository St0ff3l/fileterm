import assert from 'node:assert/strict'
import { mkdtemp, rm } from 'node:fs/promises'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import { WebDavSyncService } from '../../dist-electron/main/services/webdav-sync-service.js'

const profile = {
  id: 'webdav-ssh',
  name: 'WebDAV export',
  type: 'ssh',
  host: 'server.example.test',
  port: 22,
  username: 'ops',
  group: '默认',
  remotePath: '/',
  authType: 'password',
  password: 'login-secret',
  sftpEnabled: true,
  proxy: { type: 'http', host: 'proxy.example.test', port: 8080, password: 'proxy-secret' }
}

test('WebDAV upload omits connection secrets and download delegates to the duplicate-safe importer', async (context) => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'fileterm-webdav-'))
  let stored = ''
  const server = net.createServer((socket) => {
    let request = ''
    socket.on('data', (chunk) => {
      request += chunk.toString('utf8')
      const [head, body = ''] = request.split('\r\n\r\n')
      if (!request.includes('\r\n\r\n')) return
      if (head.startsWith('PUT')) {
        const length = Number(head.match(/content-length: (\d+)/i)?.[1] ?? 0)
        if (Buffer.byteLength(body) < length) return
        stored = body
        socket.end('HTTP/1.1 201 Created\r\nETag: "v1"\r\nContent-Length: 0\r\n\r\n')
      } else if (head.startsWith('HEAD')) socket.end('HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n')
      else socket.end(`HTTP/1.1 200 OK\r\nETag: "v1"\r\nContent-Length: ${Buffer.byteLength(stored)}\r\n\r\n${stored}`)
    })
  })
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
  const port = server.address().port
  context.after(async () => {
    await new Promise((resolve) => server.close(() => resolve()))
    await rm(directory, { recursive: true, force: true })
  })
  let imported = 0
  const service = new WebDavSyncService(
    directory,
    async () => [profile],
    async (items) => {
      imported = items.length
      return { imported, skipped: 0, failed: 0, items }
    }
  )
  await service.saveConfig({
    enabled: true,
    url: `http://127.0.0.1:${port}`,
    remotePath: 'fileterm.json',
    allowInsecureTls: true
  })
  await service.upload()
  assert.equal(stored.includes('login-secret'), false)
  assert.equal(stored.includes('proxy-secret'), false)
  const result = await service.download()
  assert.equal(imported, 1)
  assert.equal(result.imported, 1)
})
