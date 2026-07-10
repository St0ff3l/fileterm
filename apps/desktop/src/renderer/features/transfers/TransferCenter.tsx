import { startTransition, useEffect, useRef, useState } from 'react'
import type { FileTermDesktopApi, TransferTask } from '@fileterm/core'
import { isActiveTransfer } from '../../app/app-utils'
import { TransferBar } from './TransferBar'
import { TransferPopover } from './TransferPopover'
import { scopeTransfersToSession, type TransferSessionTab } from './transfer-scope'

export function TransferCenter({
  activeProfileId,
  activeTabId,
  desktopApi,
  fullWidth,
  initialTransfers,
  isPending,
  onError,
  sessionTabs,
  visible
}: {
  activeProfileId?: string
  activeTabId?: string | null
  desktopApi?: FileTermDesktopApi
  fullWidth: boolean
  initialTransfers: TransferTask[]
  isPending: boolean
  onError(scope: string, error: unknown): void
  sessionTabs: TransferSessionTab[]
  visible: boolean
}) {
  const [transfers, setTransfers] = useState(initialTransfers)
  const [showTransfers, setShowTransfers] = useState(false)
  const previousActiveCountRef = useRef(0)
  const scopedTransfers = scopeTransfersToSession(transfers, activeTabId, activeProfileId, sessionTabs)
  const activeCount = scopedTransfers.reduce((count, transfer) => count + (isActiveTransfer(transfer) ? 1 : 0), 0)

  useEffect(() => {
    setTransfers(initialTransfers)
  }, [initialTransfers])

  useEffect(() => {
    if (!desktopApi?.onTransferUpdate) {
      return
    }

    return desktopApi.onTransferUpdate((transfer) => {
      startTransition(() => {
        setTransfers((current) => {
          const index = current.findIndex((item) => item.id === transfer.id)
          if (index === -1) {
            return [transfer, ...current]
          }

          const next = [...current]
          next[index] = transfer
          return next
        })
      })
    })
  }, [desktopApi])

  useEffect(() => {
    if (activeCount > previousActiveCountRef.current) {
      setShowTransfers(true)
    }
    previousActiveCountRef.current = activeCount
  }, [activeCount])

  useEffect(() => {
    setShowTransfers(false)
  }, [activeProfileId, activeTabId])

  useEffect(() => {
    if (!visible) {
      setShowTransfers(false)
    }
  }, [visible])

  const runTransferAction = async (
    scope: string,
    action: (transferId: string) => Promise<{ transfers: TransferTask[] }>,
    transferId: string
  ) => {
    if (!desktopApi) {
      return
    }
    try {
      const snapshot = await action(transferId)
      setTransfers(snapshot.transfers)
    } catch (error) {
      onError(scope, error)
      throw error
    }
  }

  const clearTransfers = async (transferIds: string[]) => {
    if (!desktopApi || !transferIds.length) {
      return
    }
    try {
      const snapshot = await desktopApi.clearTransfers(transferIds)
      setTransfers(snapshot.transfers)
    } catch (error) {
      onError('清理传输记录', error)
    }
  }

  return (
    <>
      {visible ? (
        <TransferBar
          activeCount={activeCount}
          fullWidth={fullWidth}
          isPending={isPending}
          onOpen={() => setShowTransfers((current) => !current)}
        />
      ) : null}

      {showTransfers ? (
        <TransferPopover
          transfers={scopedTransfers}
          onDiscardTransfer={(transferId) =>
            desktopApi
              ? runTransferAction('丢弃传输断点', (id) => desktopApi.discardTransfer(id), transferId)
              : undefined
          }
          onPauseTransfer={(transferId) =>
            desktopApi ? runTransferAction('暂停传输', (id) => desktopApi.pauseTransfer(id), transferId) : undefined
          }
          onResumeTransfer={(transferId) =>
            desktopApi ? runTransferAction('继续传输', (id) => desktopApi.resumeTransfer(id), transferId) : undefined
          }
          onClearTransfers={clearTransfers}
          onClose={() => setShowTransfers(false)}
        />
      ) : null}
    </>
  )
}
