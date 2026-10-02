import { DIAGNOSTIC_SCOPES, writeDiagnosticLog } from './diagnostic-log'

type ErrorSource = 'react-boundary' | 'window-error' | 'unhandled-rejection' | 'bootstrap'

const previousReports = new Map<ErrorSource, { message: string; at: number }>()
const MAX_FIELD_BYTES = 3000
const encoder = new TextEncoder()

function limited(value: unknown, maxBytes = MAX_FIELD_BYTES): string {
  if (typeof value !== 'string') return ''
  let result = ''
  let bytes = 0
  for (const character of value) {
    // Budget the escaped UTF-8 representation so long Unicode/control-heavy
    // messages cannot make Rust truncate the component stack at the end.
    bytes += encoder.encode(JSON.stringify(character).slice(1, -1)).length
    if (bytes > maxBytes) return `${result}…[truncated]`
    result += character
  }
  return result
}

/** Do not serialize arbitrary rejection payloads: they can contain API data or credentials. */
function describeError(error: unknown) {
  if (typeof error === 'string') return { name: '', message: limited(error), stack: '' }
  if (error && typeof error === 'object' && 'message' in error) {
    const detail = error as { name?: unknown; message?: unknown; stack?: unknown }
    return { name: limited(detail.name, 100), message: limited(detail.message), stack: limited(detail.stack) }
  }
  return { name: '', message: 'Non-Error rejection (payload omitted)', stack: '' }
}

/** Best-effort local diagnostics only; never set React state or throw from an error handler. */
export function reportRendererError(source: ErrorSource, error: unknown, componentStack?: string | null): void {
  try {
    const api = window.fileterm
    const windowMode = new URLSearchParams(window.location.search).get('window') ?? 'main'
    const message = JSON.stringify({
      source,
      version: limited(api?.appVersion, 100),
      platform: limited(api?.platform, 30),
      window: limited(windowMode, 100),
      ...describeError(error),
      componentStack: limited(componentStack)
    })
    const previous = previousReports.get(source)
    const now = Date.now()
    if (previous?.message === message && now - previous.at < 5000) return
    previousReports.set(source, { message, at: now })
    writeDiagnosticLog('ERROR', DIAGNOSTIC_SCOPES.error, message, api)
  } catch {
    // Even an unusual rejection with throwing property getters must not recurse.
  }
}

/** Register once per renderer entry point; teardown also supports hot module replacement. */
export function installRendererErrorLogging(): () => void {
  const onError = (event: ErrorEvent) => {
    // Resource-loading events do not have a JS exception/message to diagnose.
    if (!event.error && !event.message) return
    reportRendererError('window-error', event.error ?? event.message)
  }
  const onRejection = (event: PromiseRejectionEvent) => {
    reportRendererError('unhandled-rejection', event.reason)
  }
  window.addEventListener('error', onError)
  window.addEventListener('unhandledrejection', onRejection)
  return () => {
    window.removeEventListener('error', onError)
    window.removeEventListener('unhandledrejection', onRejection)
  }
}
