import { createContext, use, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import type { FileTermDesktopApi } from '@fileterm/core'
import { DIAGNOSTIC_SCOPES, writeDiagnosticLog } from '../../../lib/diagnostic-log'
import { columnWidthStorageKey, readColumnWidths, writeColumnWidths } from './column-width-storage'

type LayoutScope = { profileId: string | null; remember: boolean; api?: FileTermDesktopApi }
const Context = createContext<LayoutScope>({ profileId: null, remember: false })

export function TableColumnLayoutProvider({
  profileId,
  remember,
  api,
  children
}: {
  profileId: string | null
  remember?: boolean
  api?: FileTermDesktopApi
  children: ReactNode
}) {
  const parent = use(Context)
  const enabled = remember ?? parent.remember
  const desktopApi = api ?? parent.api
  const value = useMemo(() => ({ profileId, remember: enabled, api: desktopApi }), [profileId, enabled, desktopApi])
  return <Context value={value}>{children}</Context>
}

export function useRememberedColumnWidths(tableId: string, count: number) {
  const { profileId, remember, api } = use(Context)
  const key = profileId ? columnWidthStorageKey(profileId, tableId) : null
  const [state, setState] = useState<{ key: string | null; widths: number[] | null }>({ key, widths: null })
  const revision = useRef(0)

  useEffect(() => {
    let disposed = false
    const startedRevision = revision.current
    setState({ key, widths: null })
    if (remember && api && key) {
      void readColumnWidths(api, key, count)
        .then((widths) => {
          if (!disposed && revision.current === startedRevision) setState({ key, widths })
        })
        .catch((cause: unknown) => {
          writeDiagnosticLog(
            'WARN',
            DIAGNOSTIC_SCOPES.workspace,
            `Could not restore table columns: ${String(cause)}`,
            api
          )
        })
    }
    return () => {
      disposed = true
    }
  }, [key, remember, api, count])

  const setWidths = (widths: number[] | null) => {
    revision.current++
    setState({ key, widths })
  }
  const commitWidths = (widths: number[] | null) => {
    if (!remember || !api || !key) return
    void writeColumnWidths(api, key, widths).catch((cause: unknown) => {
      writeDiagnosticLog('WARN', DIAGNOSTIC_SCOPES.workspace, `Could not save table columns: ${String(cause)}`, api)
    })
  }
  return [state.key === key ? state.widths : null, setWidths, commitWidths] as const
}
