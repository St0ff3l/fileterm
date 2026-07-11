import assert from 'node:assert/strict'
import test from 'node:test'
import {
  assertPosixMetricsComplete,
  buildPosixMetricsCommand,
  POSIX_METRICS_COMPLETE_MARKER
} from '../../src/main/services/sessions/system-metrics/posix-script.ts'

test('POSIX metrics script bounds optional collectors and emits a completion marker', () => {
  const script = buildPosixMetricsCommand('linux')

  assert.match(script, /run_bounded\(\)/)
  assert.match(script, /timeout -k 1 "\$limit" "\$@"/)
  assert.match(script, /trap 'rm -f "\$before_file" "\$after_file"'/)
  assert.match(script, /run_bounded 2 df "\$df_flags"/)
  assert.match(script, /ip route show default/)
  assert.match(script, /tailscale\*\|zt\*\|zerotier\*/)
  assert.match(script, /is_virtual_iface "\$iface" && continue/)
  assert.match(script, /local_mounts=.*overlay\|squashfs/s)
  assert.match(script, /df "\$df_flags" \$local_mounts/)
  assert.match(script, /\/\^AnonPages:\/ \{ anonpages=\$2 \* 1024 \}/)
  assert.match(script, /app=anonpages\+shmem/)
  assert.match(script, /cache=remaining-app/)
  assert.match(script, /printf "%.1fM\|0\|0\|%s\\n", \$3\/1024, proc_name/)
  assert.match(script, /run_bounded 1 nvidia-smi/)
  assert.match(script, /run_bounded 1 ps/)
  assert.equal(script.trimEnd().endsWith(`echo "${POSIX_METRICS_COMPLETE_MARKER}"`), true)
})

test('POSIX metrics output is rejected when collection stops before completion', () => {
  assert.throws(() => assertPosixMetricsComplete('__PLATFORM__linux\n__CPU__10\n', 'Linux'), /did not emit/)
  assert.doesNotThrow(() =>
    assertPosixMetricsComplete(`__PLATFORM__linux\n${POSIX_METRICS_COMPLETE_MARKER}\n`, 'Linux')
  )
})
