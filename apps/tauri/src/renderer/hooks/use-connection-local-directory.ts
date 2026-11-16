import { useEffect, useRef } from 'react'
import { withParentRow } from '../app/app-utils'
import type { LocalNetworkShareSource, UseFileOperationsOptions } from './file-operations-types'

type Options = Pick<
  UseFileOperationsOptions,
  | 'desktopApi'
  | 'localPath'
  | 'setLocalPath'
  | 'setLocalItems'
  | 'setIsLocalDirectoryLoading'
  | 'onStatusMessage'
  | 'formatError'
> & {
  enabled: boolean
  tabs: UseFileOperationsOptions['workspace']['tabs']
  tabId?: string
  defaultPath?: string
  localNetworkShareSource: LocalNetworkShareSource | null
  setLocalNetworkShareSource(source: LocalNetworkShareSource | null): void
}

export function useConnectionLocalDirectory({
  enabled,
  tabs,
  desktopApi,
  tabId,
  defaultPath,
  localPath,
  localNetworkShareSource,
  setLocalPath,
  setLocalItems,
  setIsLocalDirectoryLoading,
  setLocalNetworkShareSource,
  onStatusMessage,
  formatError
}: Options) {
  const callbacks = useRef({ onStatusMessage, formatError })
  useEffect(() => {
    callbacks.current = { onStatusMessage, formatError }
  }, [onStatusMessage, formatError])
  const current = useRef({ path: localPath, source: localNetworkShareSource })
  const remembered = useRef(new Map<string, typeof current.current & { defaultPath?: string }>())
  useEffect(() => {
    current.current = { path: localPath, source: localNetworkShareSource }
  }, [localPath, localNetworkShareSource])

  useEffect(() => {
    if (!enabled || !desktopApi) return
    let canceled = false
    let loaded = false
    const previous = remembered.current.get(tabId ?? '')
    const saved = previous?.defaultPath === defaultPath ? previous : undefined
    const requested = saved?.path || defaultPath?.trim() || undefined
    setIsLocalDirectoryLoading(true)
    void (async () => {
      try {
        const listing = await desktopApi.listLocalDirectory(requested).catch(async (error: unknown) => {
          if (canceled || !requested) throw error
          callbacks.current.onStatusMessage(
            callbacks.current.formatError('读取本机默认目录', error, { targetPath: requested })
          )
          return desktopApi.listLocalDirectory()
        })
        if (canceled) return
        const source = listing.path === saved?.path ? saved.source : null
        setLocalNetworkShareSource(source)
        setLocalPath(listing.path)
        setLocalItems(withParentRow(listing.path, listing.items, source?.mountPath))
        current.current = { path: listing.path, source }
        loaded = true
      } catch (error) {
        if (!canceled) callbacks.current.onStatusMessage(callbacks.current.formatError('读取本机目录', error))
      } finally {
        if (!canceled) setIsLocalDirectoryLoading(false)
      }
    })()
    return () => {
      canceled = true
      if (loaded) remembered.current.set(tabId ?? '', { ...current.current, defaultPath })
    }
  }, [
    enabled,
    desktopApi,
    tabId,
    defaultPath,
    setLocalPath,
    setLocalItems,
    setIsLocalDirectoryLoading,
    setLocalNetworkShareSource
  ])
  useEffect(() => {
    const openIds = new Set(tabs.map((tab) => tab.id))
    for (const key of remembered.current.keys()) {
      if (key && !openIds.has(key)) remembered.current.delete(key)
    }
  }, [tabs])
}
