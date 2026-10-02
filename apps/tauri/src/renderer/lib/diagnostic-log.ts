import type { FileTermDesktopApi } from '@fileterm/core'

export type DiagnosticLogLevel = Parameters<FileTermDesktopApi['writeDiagnosticLog']>[0]

export const DIAGNOSTIC_SCOPES = {
  workspace: 'renderer:workspace',
  monitoring: 'renderer:monitoring',
  error: 'renderer:error'
} as const

export type DiagnosticScope = (typeof DIAGNOSTIC_SCOPES)[keyof typeof DIAGNOSTIC_SCOPES]

/** Local app.log diagnostics; failures must never interrupt a UI action or event. */
export function writeDiagnosticLog(
  level: DiagnosticLogLevel,
  scope: DiagnosticScope,
  message: string,
  api: FileTermDesktopApi | undefined = window.fileterm
): void {
  try {
    void api?.writeDiagnosticLog(level, scope, message).catch(() => undefined)
  } catch {
    // A missing/stale bridge is not a reason to fail the operation being diagnosed.
  }
}
