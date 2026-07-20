import { spawn } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

// bun rewrites `npm run` inside package.json scripts to `bun run`, but supports
// neither npm's `-w` nor `--prefix`. It forwards the unrecognized flag to the
// script instead and re-invokes the same script from the repo root, appending
// the flag again on every pass until posix_spawn fails with E2BIG. Resolving
// the workspace directory here keeps npm, bun, pnpm, and yarn on one path.
function detectPackageManager() {
  const name = (process.env.npm_config_user_agent ?? '').split('/')[0]
  return ['bun', 'pnpm', 'yarn'].includes(name) ? name : 'npm'
}

function runScript(workspaceDir, script) {
  const cwd = path.resolve(repoRoot, workspaceDir)
  const manifest = path.join(cwd, 'package.json')

  if (!fs.existsSync(manifest)) {
    return Promise.reject(new Error(`no package.json found in ${cwd}`))
  }

  const manager = detectPackageManager()

  return new Promise((resolve, reject) => {
    const child = spawn(manager, ['run', script], {
      cwd,
      stdio: 'inherit',
      // npm, pnpm, and yarn resolve to .cmd shims on Windows, which spawn
      // cannot execute without a shell.
      shell: process.platform === 'win32'
    })

    child.on('error', reject)
    child.on('exit', (code, signal) => {
      if (code !== 0) {
        const reason = signal ? `was terminated by signal ${signal}` : `exited with code ${code}`
        reject(new Error(`\`${manager} run ${script}\` in ${workspaceDir} ${reason}`))
        return
      }
      resolve()
    })
  })
}

const [script, ...workspaceDirs] = process.argv.slice(2)

if (!script || workspaceDirs.length === 0) {
  console.error('Usage: node ./scripts/workspace-run.mjs <script> <workspace-dir> [workspace-dir...]')
  process.exit(1)
}

// Sequential on purpose: later workspaces consume the dist/ output of earlier
// ones, so this doubles as the dependency order.
for (const workspaceDir of workspaceDirs) {
  try {
    await runScript(workspaceDir, script)
  } catch (error) {
    console.error(`[FileTerm] ${error.message}`)
    process.exit(1)
  }
}
