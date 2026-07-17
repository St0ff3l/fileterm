import assert from 'node:assert/strict'
import test from 'node:test'
import { relativeRemoteTransferPath } from '../../src/main/services/transfers/transfer-path-utils.ts'

test('returns a normalized POSIX path inside the selected remote directory', () => {
  assert.equal(relativeRemoteTransferPath('/srv/releases', '/srv/releases/app/config.json'), 'app/config.json')
  assert.equal(relativeRemoteTransferPath('/', '/var/log/app.log'), 'var/log/app.log')
})

test('rejects sibling and traversal paths that could escape the local destination', () => {
  assert.throws(() => relativeRemoteTransferPath('/srv/releases', '/srv/private/key'), /远端路径不在下载目录内/)
  assert.throws(
    () => relativeRemoteTransferPath('/srv/releases', '/srv/releases/../../etc/passwd'),
    /远端路径不在下载目录内/
  )
  assert.throws(() => relativeRemoteTransferPath('/srv/releases', '/srv/releases'), /远端路径不在下载目录内/)
})
