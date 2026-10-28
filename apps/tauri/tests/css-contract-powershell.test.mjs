import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

test('PowerShell CSS check uses the same rules and propagates failures', { skip: process.platform !== 'win32' }, () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url))
  const script = join(root, '.agents/skills/fileterm-common-components/scripts/check-css-contract.ps1')
  const shellScript = join(root, '.agents/skills/fileterm-common-components/scripts/check-css-contract.sh')
  const gitExec = execFileSync('git', ['--exec-path'], { encoding: 'utf8' }).trim()
  const bash = resolve(gitExec, '../../../bin/bash.exe')
  const pwsh = execFileSync('where.exe', ['pwsh.exe'], { encoding: 'utf8' }).trim().split(/\r?\n/)[0]
  const git = execFileSync('where.exe', ['git.exe'], { encoding: 'utf8' }).trim().split(/\r?\n/)[0]
  const run = (command, args, env = process.env) => spawnSync(command, args, { cwd: root, env, encoding: 'utf8' })

  const expected = run(bash, [shellScript])
  const actual = run(pwsh, ['-NoProfile', '-File', script])
  assert.equal(actual.status, expected.status)
  assert.equal(actual.stdout.replace(/\r\n/g, '\n'), expected.stdout.replace(/\r\n/g, '\n'))

  const missingBase = run(pwsh, ['-NoProfile', '-File', script], {
    ...process.env,
    CSS_CONTRACT_BASE: 'refs/fileterm-test-missing-base'
  })
  assert.equal(missingBase.status, 1)
  assert.match(missingBase.stdout, /Could not find merge-base/)

  const missingRg = run(pwsh, ['-NoProfile', '-File', script], {
    ...process.env,
    PATH: `${dirname(git)};${join(process.env.SystemRoot, 'System32')}`
  })
  assert.notEqual(missingRg.status, 0)
  assert.match(missingRg.stderr, /ripgrep/)
})
