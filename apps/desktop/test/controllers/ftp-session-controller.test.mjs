import assert from 'node:assert/strict'
import test from 'node:test'
import { FileInfo, FileType } from 'basic-ftp'
import { LiveFtpSessionController } from '../../dist-electron/main/services/sessions/ftp-session-controller.js'

class FakeFtpClient {
  closed = true
  currentPath = '/'
  accessCalls = []
  closeCalls = 0
  activeLists = 0
  maxActiveLists = 0

  parseList() {
    return []
  }

  async access(options) {
    this.accessCalls.push(options)
    this.closed = false
  }

  async cd(targetPath) {
    this.currentPath = targetPath
  }

  async pwd() {
    return this.currentPath
  }

  async list() {
    this.activeLists += 1
    this.maxActiveLists = Math.max(this.maxActiveLists, this.activeLists)
    await new Promise((resolve) => setTimeout(resolve, 5))
    this.activeLists -= 1

    const folder = new FileInfo('z-folder')
    folder.type = FileType.Directory
    folder.rawName = folder.name
    folder.rawIndex = 1
    const file = new FileInfo('a.txt')
    file.type = FileType.File
    file.size = 12
    file.rawName = file.name
    file.rawIndex = 0
    return [file, folder]
  }

  close() {
    this.closeCalls += 1
    this.closed = true
  }
}

function createProfile(overrides = {}) {
  return {
    id: 'ftp-profile',
    name: 'FTP test',
    type: 'ftp',
    host: 'ftp.example.test',
    port: 21,
    group: '',
    username: 'tester',
    password: 'secret',
    secure: true,
    securityMode: 'implicit',
    remotePath: '/srv',
    ...overrides
  }
}

test('FTP controller connects with the selected security mode and returns sorted directory rows', async () => {
  const ftp = new FakeFtpClient()
  const controller = new LiveFtpSessionController('ftp-tab', createProfile(), ftp)

  await controller.connect()
  assert.equal(controller.getSummary(), 'Connected to ftp.example.test:21')
  assert.equal(ftp.accessCalls.length, 1)
  assert.equal(ftp.accessCalls[0].secure, 'implicit')
  assert.equal(controller.getRemotePath(), '/srv')

  const rows = await controller.listRemoteFiles()
  assert.deepEqual(
    rows.map((row) => [row.name, row.type]),
    [
      ['..', 'folder'],
      ['z-folder', 'folder'],
      ['a.txt', 'file']
    ]
  )

  await controller.disconnect()
  assert.equal(ftp.closeCalls, 1)
  assert.equal(controller.getSummary(), 'Ready to connect ftp.example.test:21')
})

test('FTP controller serializes operations and reconnects a closed protocol client', async () => {
  const ftp = new FakeFtpClient()
  const controller = new LiveFtpSessionController('ftp-tab', createProfile({ securityMode: 'explicit' }), ftp)

  await controller.connect()
  ftp.closed = true
  await Promise.all([controller.listRemoteFiles(), controller.listRemoteFiles()])

  assert.equal(ftp.accessCalls.length, 2)
  assert.equal(ftp.accessCalls[1].secure, true)
  assert.equal(ftp.maxActiveLists, 1)
  await controller.disconnect()
})

test('FTP controller rejects invalid permission modes before sending a protocol command', async () => {
  const controller = new LiveFtpSessionController('ftp-tab', createProfile(), new FakeFtpClient())
  await assert.rejects(controller.changeRemotePermissions('/srv/a.txt', { mode: '999' }), /八进制/)
})
