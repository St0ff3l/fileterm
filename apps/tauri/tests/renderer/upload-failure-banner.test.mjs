import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
const t = { uploadPermissionDenied: 'Server denied a file operation.', uploadFailed: 'Upload failed' }
const source = readFileSync(new URL('../../src/renderer/hooks/workspace-ipc-sync-utils.ts', import.meta.url), 'utf8')
vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, {
  exports,
  require: (name) => (name === '../i18n' ? { t } : {})
})

test('upload permission feedback preserves the failed operation, path and server error', () => {
  const detail = 'SFTP REMOVE cleanup path="/dir/probe.fileterm-part": Permission denied'
  assert.equal(
    exports.uploadFailureBanner({ direction: 'upload', status: 'failed', message: `command error: ${detail}` }),
    `${t.uploadPermissionDenied} ${detail}`
  )
  assert.equal(exports.uploadFailureBanner({ direction: 'download', status: 'failed', message: detail }), null)
  assert.equal(exports.uploadFailureBanner({ direction: 'upload', status: 'completed', message: detail }), null)
})

test('no-delete protection reports the preserved target and checkpoint', () => {
  const detail =
    'SFTP COMMIT path="/dir/file": 已启用服务器禁止删除模式，不能安全覆盖已有目标；原文件未更改，断点保留于 "/dir/file.fileterm-part"'
  assert.equal(
    exports.uploadFailureBanner({ direction: 'upload', status: 'failed', message: `command error: ${detail}` }),
    `${t.uploadFailed}: ${detail}`
  )
})

test('backup cleanup denial reports the completed upload without calling it a failed upload', () => {
  const detail =
    'SFTP REMOVE backup path="/dir/file.fileterm-backup-id": Permission denied; 上传已完成且目标已更新，旧文件备份仍保留；请勿重试上传'
  assert.equal(
    exports.uploadFailureBanner({ direction: 'upload', status: 'failed', message: `command error: ${detail}` }),
    detail
  )
})
