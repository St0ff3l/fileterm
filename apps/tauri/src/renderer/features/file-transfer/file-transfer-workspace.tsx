import { useEffect, useRef, useState, type PointerEvent } from 'react'
import type {
  FileTermDesktopApi,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceTab,
  WorkspaceSnapshot
} from '@fileterm/core'
import { StableButtonLabel } from '../common/stable-button-content'
import { Button } from '../../components/common/button/button'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { DropdownSelect } from '../common/dropdown-select'
import { TransferPane } from './transfer-pane'
import type { TransferClipboard } from './transfer-pane-context-menu'
import { useTransferPane } from './use-transfer-pane'
import { shouldRefreshTransferPane } from './transfer-refresh'
import { t } from '../../i18n'
import './file-transfer-workspace.css'

type Side = 'left' | 'right'
type CopyRequest = { side: Side; source: string; destination: string; directory: string; items: RemoteFileItem[] }

export function FileTransferWorkspace({
  api,
  onApplySnapshot,
  tabs,
  sessions,
  initialTabId,
  initialPath,
  isActive
}: {
  api?: FileTermDesktopApi
  onApplySnapshot?(snapshot: WorkspaceSnapshot): void
  tabs: WorkspaceTab[]
  sessions: Record<string, SessionSnapshot>
  initialTabId?: string
  initialPath?: string
  isActive: boolean
}) {
  const left = useTransferPane(api, sessions, initialTabId, initialPath)
  const right = useTransferPane(api, sessions)
  const [activeSide, setActiveSide] = useState<Side | null>(null)
  const [policy, setPolicy] = useState<'skip' | 'replace'>('skip')
  const [clipboard, setClipboard] = useState<TransferClipboard | null>(null)
  const [pending, setPending] = useState<CopyRequest | null>(null)
  const [busy, setBusy] = useState(false)
  const [fileActionBusy, setFileActionBusy] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [dropTarget, setDropTarget] = useState<Side | null>(null)
  const [dragging, setDragging] = useState(false)
  const dragCleanup = useRef<(() => void) | null>(null)
  const options = [
    { value: 'local', label: t.localComputer },
    ...tabs
      .filter((tab) => tab.sessionType === 'ssh' || tab.sessionType === 'sftp' || tab.sessionType === 'ftp')
      .map((tab) => ({
        value: tab.id,
        label: `${tab.title} · ${tab.sessionType === 'ftp' ? 'FTP' : 'SFTP'}`,
        disabled: !sessions[tab.id]?.connected || sessions[tab.id]?.fileAccessMode === 'root'
      }))
  ]
  const sourcePane = activeSide === 'left' ? left : right
  const targetPane = activeSide === 'left' ? right : left
  const endpointsReady = Boolean(
    api &&
    !left.error &&
    !right.error &&
    left.connected &&
    right.connected &&
    left.endpoint !== right.endpoint &&
    left.path &&
    right.path
  )
  const canCopy = endpointsReady && !busy && !fileActionBusy && !left.loading && !right.loading
  const copyDisabled =
    activeSide === null ||
    !endpointsReady ||
    busy ||
    fileActionBusy ||
    !sourcePane.items.some((item) => item.name !== '..' && sourcePane.selected.includes(item.path))
  const startCopy = (side: Side, items?: RemoteFileItem[]) => {
    const source = side === 'left' ? left : right
    const destination = side === 'left' ? right : left
    const selected = items ?? source.items.filter((item) => source.selected.includes(item.path) && item.name !== '..')
    if (!canCopy || !selected.length) return
    setError(null)
    setPending({
      side,
      source: source.endpoint,
      destination: destination.endpoint,
      directory: destination.path,
      items: selected
    })
  }
  const submitCopy = async () => {
    if (!pending || !api || busy) return
    setBusy(true)
    setError(null)
    let queued = 0
    try {
      for (const item of pending.items) {
        if (pending.source !== 'local' && pending.destination !== 'local') {
          await api.copyBetweenServers(
            pending.source,
            pending.destination,
            item.path,
            pending.directory,
            policy === 'replace'
          )
        } else {
          const options = { conflictPolicy: policy as 'skip' | 'replace' }
          if (pending.source === 'local')
            await api.uploadFile(pending.destination, item.path, pending.directory, options)
          else await api.downloadRemotePath(pending.source, item.path, item.type, pending.directory, options)
        }
        queued += 1
      }
      setMessage(`${t.fileTransferQueued}: ${queued}`)
      setPending(null)
    } catch (error) {
      setError(String(error))
      setPending(null)
      setMessage(`${t.fileTransferQueued}: ${queued}`)
    } finally {
      setBusy(false)
    }
  }
  const contextMenuProps = (side: Side) => {
    const pane = side === 'left' ? left : right
    return {
      api,
      onApplySnapshot,
      tab: tabs.find((tab) => tab.id === pane.endpoint),
      busy: busy || fileActionBusy,
      policy,
      clipboard,
      clipboardConnected: Boolean(
        clipboard &&
        (clipboard.endpoint === 'local' ||
          (sessions[clipboard.endpoint]?.connected && sessions[clipboard.endpoint]?.fileAccessMode !== 'root'))
      ),
      canCopyAcross: canCopy,
      onBusyChange: setFileActionBusy,
      onClipboardChange: setClipboard,
      onMutated: async () => {
        await Promise.all([left.load(left.path), right.load(right.path)])
      },
      onPasteAcross: (source: TransferClipboard) => {
        if (busy || fileActionBusy || !pane.connected || pane.loading || pane.error || source.operation !== 'copy')
          return
        setPending({
          side,
          source: source.endpoint,
          destination: pane.endpoint,
          directory: pane.path,
          items: source.items
        })
      }
    }
  }
  const beginDrag = (side: Side, event: PointerEvent<HTMLElement>, items: RemoteFileItem[]) => {
    if (event.button !== 0 || busy || !canCopy) return
    dragCleanup.current?.()
    const x = event.clientX,
      y = event.clientY
    let moved = false
    const move = (event: globalThis.PointerEvent) => {
      if (Math.hypot(event.clientX - x, event.clientY - y) < 6 && !moved) return
      moved = true
      setDragging(true)
      const target = document
        .elementFromPoint(event.clientX, event.clientY)
        ?.closest<HTMLElement>('[data-transfer-pane]')?.dataset.transferPane
      setDropTarget(target && target !== side ? (target as Side) : null)
    }
    const cleanup = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      window.removeEventListener('pointercancel', cleanup)
      window.removeEventListener('blur', cleanup)
      window.removeEventListener('keydown', key)
      setDragging(false)
      setDropTarget(null)
      dragCleanup.current = null
    }
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') cleanup()
    }
    const up = (event: globalThis.PointerEvent) => {
      const target = document
        .elementFromPoint(event.clientX, event.clientY)
        ?.closest<HTMLElement>('[data-transfer-pane]')?.dataset.transferPane
      cleanup()
      if (moved && target && target !== side) startCopy(side, items)
    }
    dragCleanup.current = cleanup
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    window.addEventListener('pointercancel', cleanup)
    window.addEventListener('blur', cleanup)
    window.addEventListener('keydown', key)
  }
  useEffect(() => () => dragCleanup.current?.(), [])
  useEffect(() => {
    if (!isActive) dragCleanup.current?.()
  }, [isActive])
  const leftProfileId = sessions[left.endpoint]?.profileId
  const rightProfileId = sessions[right.endpoint]?.profileId
  useEffect(() => {
    if (!api || !isActive) return
    return api.onTransferUpdate((task) => {
      if (!left.error && shouldRefreshTransferPane(task, left.endpoint, left.path, leftProfileId))
        void left.load(left.path)
      if (!right.error && shouldRefreshTransferPane(task, right.endpoint, right.path, rightProfileId))
        void right.load(right.path)
    })
  }, [
    api,
    isActive,
    left.endpoint,
    left.path,
    right.endpoint,
    right.path,
    leftProfileId,
    rightProfileId,
    left.error,
    right.error
  ])
  return (
    <div
      className={`file-transfer-workspace ${dragging ? 'is-dragging' : ''}`}
      onPointerDown={(event) => {
        if (
          !(event.target instanceof Element) ||
          event.target.closest('[data-transfer-pane], button, input, select, [role="menu"], [role="dialog"]')
        )
          return
        setActiveSide(null)
        left.setSelected([])
        right.setSelected([])
      }}
    >
      <header className="file-transfer-workspace__toolbar">
        <div>
          <h2>{t.fileTransferWorkspace}</h2>
          <p>{t.fileTransferRelayHint}</p>
        </div>
        <DropdownSelect
          className="file-transfer-workspace__policy"
          ariaLabel={t.fileTransferConflict}
          value={policy}
          options={[
            { value: 'skip', label: t.fileTransferSkip },
            { value: 'replace', label: t.fileTransferReplace }
          ]}
          onChange={(value) => setPolicy(value === 'replace' ? 'replace' : 'skip')}
          disabled={busy}
        />
        {/* Refresh preserves the last valid selection; startCopy guards the aria-disabled action. */}
        <Button
          variant="primary"
          size="compact"
          loading={busy}
          reserveLoadingSpace
          type="button"
          disabled={copyDisabled}
          aria-disabled={copyDisabled || !canCopy}
          onClick={() => {
            if (activeSide) startCopy(activeSide)
          }}
        >
          <StableButtonLabel
            label={
              activeSide === null ? t.copy : activeSide === 'left' ? t.fileTransferCopyRight : t.fileTransferCopyLeft
            }
            reserveLabel={t.fileTransferCopyRight}
          />
        </Button>
      </header>
      <div className="file-transfer-workspace__panes">
        <TransferPane
          key={`left:${left.endpoint}`}
          contextMenuProps={contextMenuProps('left')}
          pane={left}
          side="left"
          options={options}
          active={activeSide === 'left'}
          dropTarget={dropTarget === 'left'}
          onActivate={() => setActiveSide('left')}
          onCopy={(items) => startCopy('left', items)}
          onDragStart={(event, items) => beginDrag('left', event, items)}
        />
        <TransferPane
          key={`right:${right.endpoint}`}
          contextMenuProps={contextMenuProps('right')}
          pane={right}
          side="right"
          options={options}
          active={activeSide === 'right'}
          dropTarget={dropTarget === 'right'}
          onActivate={() => setActiveSide('right')}
          onCopy={(items) => startCopy('right', items)}
          onDragStart={(event, items) => beginDrag('right', event, items)}
        />
      </div>
      <p className={`file-transfer-workspace__hint file-transfer-workspace__feedback ${error ? 'is-error' : ''}`}>
        {error ? (
          <span role="alert">{error}</span>
        ) : message ? (
          <span role="status">{message}</span>
        ) : (
          <>
            {t.fileTransferHelp}
            {sourcePane.endpoint === targetPane.endpoint ? ` · ${t.fileTransferDifferentEndpoints}` : ''}
          </>
        )}
      </p>
      {pending ? (
        <ConfirmActionDialog
          title={t.fileTransferConfirm}
          confirmLabel={t.copy}
          confirmVariant={policy === 'replace' ? 'danger' : 'primary'}
          isSubmitting={busy}
          description={
            <>
              <p>{pending.items.map((item) => item.name).join(', ')}</p>
              <p>
                {options.find((option) => option.value === pending.source)?.label} →{' '}
                {options.find((option) => option.value === pending.destination)?.label}
              </p>
              <p>{pending.directory}</p>
              <p>{policy === 'replace' ? t.fileTransferReplaceWarning : t.fileTransferSkip}</p>
            </>
          }
          onClose={() => {
            if (!busy) setPending(null)
          }}
          onConfirm={() => void submitCopy()}
        />
      ) : null}
    </div>
  )
}
