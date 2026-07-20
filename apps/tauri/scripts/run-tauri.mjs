import { spawn } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const env = { ...process.env }
const pathKey = Object.keys(env).find((key) => key.toLowerCase() === 'path') ?? 'PATH'
const pathEntries = (env[pathKey] ?? '').split(path.delimiter).filter(Boolean)

function prependExecutableDirectory(directory, executable) {
  if (!directory || !fs.existsSync(path.join(directory, executable))) {
    return
  }

  const normalizedDirectory = path.resolve(directory).toLowerCase()
  if (!pathEntries.some((entry) => path.resolve(entry).toLowerCase() === normalizedDirectory)) {
    pathEntries.unshift(directory)
  }
}

const cargoHome = env.CARGO_HOME ? path.resolve(env.CARGO_HOME) : path.join(os.homedir(), '.cargo')
prependExecutableDirectory(path.join(cargoHome, 'bin'), process.platform === 'win32' ? 'cargo.exe' : 'cargo')

if (process.platform === 'win32') {
  prependExecutableDirectory(path.join(env.LOCALAPPDATA ?? '', 'bin', 'NASM'), 'nasm.exe')
}

env[pathKey] = pathEntries.join(path.delimiter)

const tauriArgs = process.argv.slice(2)
// Only `dev` launches a window in-process. `build` bundles without ever opening
// a GTK surface, so it stays usable on headless CI runners.
const launchesWindow = tauriArgs[0] === 'dev'

function readDisplayEnvironment() {
  return {
    DISPLAY: env.DISPLAY?.trim() ?? '',
    WAYLAND_DISPLAY: env.WAYLAND_DISPLAY?.trim() ?? '',
    XDG_SESSION_TYPE: env.XDG_SESSION_TYPE?.trim() ?? '',
    XDG_RUNTIME_DIR: env.XDG_RUNTIME_DIR?.trim() ?? '',
    XAUTHORITY: env.XAUTHORITY?.trim() ?? ''
  }
}

function formatDisplayEnvironment() {
  return Object.entries(readDisplayEnvironment())
    .map(([key, value]) => `    ${key}=${value || '(unset)'}`)
    .join('\n')
}

// tao aborts with a bare `Failed to initialize GTK` panic when no display server
// is reachable. Fail earlier with something actionable instead.
function assertDisplayServerReachable() {
  if (process.platform !== 'linux' || !launchesWindow) {
    return
  }

  const { DISPLAY, WAYLAND_DISPLAY } = readDisplayEnvironment()
  if (DISPLAY || WAYLAND_DISPLAY) {
    return
  }

  console.error(
    [
      '[FileTerm] no display server detected; GTK cannot start a window.',
      formatDisplayEnvironment(),
      '',
      '  Both DISPLAY and WAYLAND_DISPLAY are unset. Common causes:',
      '    - running over SSH without X forwarding (reconnect with `ssh -X`, and install `xauth` on the host)',
      '    - running from a system console or service unit with no graphical session',
      '',
      '  FileTerm dev needs a local graphical session. Run it from a desktop terminal.'
    ].join('\n')
  )
  process.exit(1)
}

// A display can be present and GTK can still fail (missing XWayland, stale
// XAUTHORITY, mismatched user). Surface the environment so the failure is
// diagnosable instead of just a panic trace.
function reportDisplayFailure() {
  if (process.platform !== 'linux' || !launchesWindow) {
    return
  }

  const { DISPLAY, WAYLAND_DISPLAY, XDG_SESSION_TYPE } = readDisplayEnvironment()
  const hints = []

  if (!DISPLAY && WAYLAND_DISPLAY) {
    hints.push(
      '    - Wayland session without XWayland: install `xwayland`, or retry with `GDK_BACKEND=wayland npm run dev:tauri`'
    )
  }
  if (DISPLAY && XDG_SESSION_TYPE === 'wayland') {
    hints.push('    - Wayland session using XWayland: retry with `GDK_BACKEND=x11 npm run dev:tauri`')
  }
  hints.push('    - verify the GTK/WebKit runtime is installed, not just the -dev packages')
  hints.push('    - confirm the session user matches the one owning XAUTHORITY (avoid running under sudo)')

  console.error(
    [
      '',
      '[FileTerm] Tauri dev exited with an error. Display environment:',
      formatDisplayEnvironment(),
      '',
      '  If the failure was `Failed to initialize GTK`:',
      ...hints
    ].join('\n')
  )
}

assertDisplayServerReachable()

const tauriCli = require.resolve('@tauri-apps/cli/tauri.js')
const child = spawn(process.execPath, [tauriCli, ...tauriArgs], {
  env,
  stdio: 'inherit'
})

child.on('error', (error) => {
  console.error(`[FileTerm] failed to start the local Tauri CLI: ${error.message}`)
  process.exitCode = 1
})

child.on('exit', (code) => {
  if (code) {
    reportDisplayFailure()
  }
  process.exitCode = code ?? 1
})
