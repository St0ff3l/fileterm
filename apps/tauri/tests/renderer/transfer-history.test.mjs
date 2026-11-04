import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
const shared = await import('../../../../packages/shared/dist/index.js')
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/features/transfers/transfer-history.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports, Set, require: () => shared }
)
const { retainTransferHistory, updateTransferHistory } = exports
const task = (id, status = 'done', updatedAt = 0, extra = {}) => ({ id, status, updatedAt, ...extra })

test('progress received before workspace hydration keeps only the latest task state', () => {
  let pending = []
  for (let index = 0; index < 50000; index++) {
    pending = updateTransferHistory(pending, task(`active-${index % 8}`, 'running', index, { resumable: true }))
  }
  assert.equal(pending.length, 8)
  assert.equal(pending.at(-1).updatedAt, 49999)
})

test('running transfer history stays bounded across repeated progress and completion events', () => {
  let history = []
  for (let index = 0; index < 5000; index++) {
    const id = `task-${index}`
    history = updateTransferHistory(history, task(id, 'running', index, { resumable: true }))
    history = updateTransferHistory(history, task(id, 'done', index))
    assert.ok(history.length <= shared.TRANSFER_HISTORY_LIMIT)
  }
  assert.equal(history.length, shared.TRANSFER_HISTORY_LIMIT)
  assert.equal(history[0].id, 'task-4800')
  assert.equal(history.at(-1).id, 'task-4999')
  assert.equal(updateTransferHistory(history, history[0]), history)
  assert.equal(updateTransferHistory(history, task('task-0', 'done', 0)).length, shared.TRANSFER_HISTORY_LIMIT)
})

test('retention never expires unfinished, resumable or cleanup work even beyond the history limit', () => {
  const protectedTasks = Array.from({ length: 210 }, (_, index) => task(`active-${index}`, 'running'))
  protectedTasks.push(task('paused', 'paused'), task('retry', 'failed', 0, { resumable: true }))
  protectedTasks.push(task('cleanup', 'canceled', 0, { cleanupPending: true }))
  const history = retainTransferHistory([...protectedTasks, task('old-done')])
  assert.deepEqual(Array.from(history), protectedTasks)
})

test('retention matches journal timestamps, append-order ties and stable task references', () => {
  const tasks = [task('old', 'done', 1), task('tie-old', 'done', 2)]
  tasks.push(...Array.from({ length: 198 }, (_, index) => task(`new-${index}`, 'done', 3)))
  tasks.push(task('tie-new', 'done', 2))
  const history = retainTransferHistory(tasks)
  assert.equal(history.length, 200)
  assert.equal(history[0], tasks[1])
  assert.equal(history.at(-1), tasks.at(-1))
  const rust = readFileSync(new URL('../../src-tauri/src/services/transfers/model.rs', import.meta.url), 'utf8')
  assert.equal(Number(rust.match(/const JOURNAL_MAX_TASKS: usize = (\d+)/)[1]), shared.TRANSFER_HISTORY_LIMIT)
})
