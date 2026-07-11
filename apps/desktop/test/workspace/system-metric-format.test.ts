import assert from 'node:assert/strict'
import test from 'node:test'
import { formatSystemLoad } from '../../src/renderer/features/system/system-metric-format.ts'

test('labels normalized Windows load as busy logical processors', () => {
  assert.deepEqual(
    formatSystemLoad(
      { load: '1.80', loadUnit: 'busy-logical-processors' },
      {
        busyLogicalProcessorsUnit: '核',
        busyLogicalProcessorsDescription: 'Windows 负载按当前 CPU 使用率折算为繁忙逻辑处理器数量。'
      }
    ),
    {
      value: '1.80核',
      title: 'Windows 负载按当前 CPU 使用率折算为繁忙逻辑处理器数量。'
    }
  )
})

test('keeps POSIX load averages unchanged', () => {
  assert.deepEqual(
    formatSystemLoad(
      { load: '0.10, 0.08, 0.05' },
      { busyLogicalProcessorsUnit: '核', busyLogicalProcessorsDescription: '' }
    ),
    {
      value: '0.10, 0.08, 0.05',
      title: ''
    }
  )
})
