import { DIAGNOSTIC_SCOPES, writeDiagnosticLog } from './diagnostic-log'

/** Detect recovered stalls without relying on an exception or animation frames. */
export function installRendererPerformanceLogging(): () => void {
  let previous = performance.now()
  let lastReport = -Infinity
  const reset = () => {
    previous = performance.now()
  }
  const timer = window.setInterval(() => {
    const now = performance.now()
    const elapsed = now - previous
    previous = now
    if (document.hidden || elapsed < 4000 || now - lastReport < 30000) return
    lastReport = now
    writeDiagnosticLog(
      'WARN',
      DIAGNOSTIC_SCOPES.performance,
      JSON.stringify({
        event: 'event-loop-gap',
        elapsedMs: Math.round(elapsed),
        focused: document.hasFocus(),
        version: window.fileterm?.appVersion,
        platform: window.fileterm?.platform
      })
    )
  }, 1000)
  // Background timer throttling must not be reported as a foreground stall.
  document.addEventListener('visibilitychange', reset)
  return () => {
    window.clearInterval(timer)
    document.removeEventListener('visibilitychange', reset)
  }
}
