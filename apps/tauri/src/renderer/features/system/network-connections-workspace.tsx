import { useEffect, useMemo, useRef, useState } from 'react'
import type { NetworkConnectionsSnapshot } from '@fileterm/core'
import { Button } from '../../components/common/button/button'
import { Input } from '../../components/common/input/input'
import { DropdownSelect } from '../common/dropdown-select'
import { waitForMinimumBusyDuration } from '../common/operation-timing'
import { t } from '../../i18n'
import {
  connectionsFor,
  formatSocketBytes,
  socketAddressLabel,
  socketKey,
  socketRowKeys,
  trafficTotal,
  type NetworkTableSort
} from './network-connections-model'
import { NetworkSocketTable } from './network-socket-table'
import './system-info-header.css'
import './network-connections-workspace.css'

export function NetworkConnectionsWorkspace({
  tabId,
  connected,
  isActive
}: {
  tabId: string
  connected: boolean
  isActive: boolean
}) {
  const [snapshot, setSnapshot] = useState<NetworkConnectionsSnapshot | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [refresh, setRefresh] = useState(0)
  const [query, setQuery] = useState('')
  const [mode, setMode] = useState('listeners')
  const [selectedKey, setSelectedKey] = useState('')
  const [sort, setSort] = useState<NetworkTableSort>({ column: 4, direction: 'asc' })
  const [peerSort, setPeerSort] = useState<NetworkTableSort>({ column: 0, direction: 'asc' })
  const [updatedAt, setUpdatedAt] = useState('')
  const requestPending = useRef(false)

  useEffect(() => {
    if (!isActive || !connected || !tabId) {
      setBusy(false)
      return
    }
    let disposed = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const load = async (showLoading = false) => {
      if (document.hidden) {
        timer = setTimeout(() => void load(), 3000)
        return
      }
      requestPending.current = true
      const startedAt = performance.now()
      if (showLoading) setBusy(true)
      try {
        if (!window.fileterm) throw new Error('NETWORK_SESSION_UNAVAILABLE')
        const result = await window.fileterm.getNetworkConnections(tabId)
        if (!disposed) {
          setSnapshot(result)
          setError('')
          setUpdatedAt(new Date().toLocaleTimeString())
        }
      } catch (cause) {
        if (!disposed) {
          const message = String(cause)
          setError(
            message.includes('PLATFORM_UNSUPPORTED')
              ? t.networkUnsupported
              : message.includes('SS_REQUIRED')
                ? t.networkSsRequired
                : t.networkLoadFailed
          )
        }
        return // A failed collector needs an explicit retry, not a background retry loop.
      } finally {
        if (showLoading) await waitForMinimumBusyDuration(startedAt)
        requestPending.current = false
        if (!disposed) setBusy(false)
      }
      if (!disposed) timer = setTimeout(() => void load(), 3000)
    }
    void load(true)
    return () => {
      disposed = true
      clearTimeout(timer)
    }
  }, [tabId, refresh, isActive, connected])

  const sockets = snapshot?.sockets ?? []
  const listeners = sockets.filter((socket) => socket.listening)
  const visible = useMemo(() => {
    const text = query.trim().toLowerCase()
    return sockets.filter(
      (socket) =>
        (mode !== 'listeners' || socket.listening) &&
        `${socket.pid ?? ''} ${socket.process ?? ''} ${socket.protocol} ${socket.localAddress} ${socket.localPort} ${socket.remoteAddress} ${socket.remotePort}`
          .toLowerCase()
          .includes(text)
    )
  }, [snapshot, mode, query])
  const selected = visible.find((socket) => socketKey(socket) === selectedKey) ?? visible[0]
  const peers = selected ? (selected.listening ? connectionsFor(selected, sockets) : [selected]) : []
  const rowKeys = socketRowKeys(visible)
  const peerKeys = socketRowKeys(peers)
  const addressLabel = (address: string) => socketAddressLabel(address, t.networkAllIpv4, t.networkAllIpv6)

  return (
    <section className="network-connections-workspace selectable-text">
      <header className="system-info-header">
        <strong>{t.networkConnections}</strong>
        <p>{t.networkConnectionsDescription}</p>
      </header>
      <div className="network-connections-toolbar">
        <Input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t.networkFilter}
          aria-label={t.networkFilter}
        />
        <DropdownSelect
          value={mode}
          onChange={setMode}
          ariaLabel={t.networkView}
          options={[
            { value: 'listeners', label: t.networkListeners },
            { value: 'all', label: t.networkAllSockets }
          ]}
        />
        <DropdownSelect
          value={String(sort.column)}
          onChange={(value) => setSort({ column: Number(value), direction: 'asc' })}
          ariaLabel={t.networkSort}
          options={[
            { value: '4', label: t.networkSortPort },
            { value: '1', label: t.networkSortProcess },
            ...[
              'PID',
              t.networkProcess,
              t.networkProtocol,
              t.networkLocalAddress,
              t.networkPort,
              t.networkPeerIps,
              t.networkPeerCount,
              t.upload,
              t.download
            ].flatMap((label, column) => (column === 4 || column === 1 ? [] : [{ value: String(column), label }]))
          ]}
        />
        <Button
          className="network-connections-refresh"
          size="md"
          loading={busy}
          reserveLoadingSpace
          disabled={!connected}
          onClick={() => {
            if (!requestPending.current) setRefresh((value) => value + 1)
          }}
        >
          {t.refresh}
        </Button>
      </div>
      <div className="network-connections-status" role="status">
        {!connected
          ? t.networkDisconnected
          : error ||
            (snapshot
              ? `${listeners.length} ${t.networkListeners} · ${sockets.length} ${t.networkAllSockets} · ${updatedAt}`
              : t.networkLoading)}
        {snapshot?.truncated ? ` · ${t.networkTruncated}` : ''}
      </div>
      {snapshot?.source === 'netstat' ? <p className="network-connections-note">{t.networkInstallSsHint}</p> : null}
      <NetworkSocketTable
        layoutId="network-sockets"
        sort={sort}
        onSortChange={setSort}
        headers={[
          'PID',
          t.networkProcess,
          t.networkProtocol,
          t.networkLocalAddress,
          t.networkPort,
          t.networkPeerIps,
          t.networkPeerCount,
          t.upload,
          t.download
        ]}
        empty={busy ? t.networkLoading : t.networkEmpty}
        rows={visible.map((socket, index) => {
          const related = socket.listening ? connectionsFor(socket, sockets) : [socket]
          return {
            key: rowKeys[index],
            selected: selected === socket,
            onSelect: () => setSelectedKey(socketKey(socket)),
            sortValues: [
              socket.pid,
              socket.process ?? '',
              socket.protocol,
              socket.localAddress,
              socket.localPort,
              new Set(related.map((peer) => peer.remoteAddress)).size,
              related.length,
              trafficTotal(related, 'txBytes'),
              trafficTotal(related, 'rxBytes')
            ],
            cells: [
              socket.pid ?? '—',
              socket.process ?? t.networkUnknownProcess,
              socket.protocol.toUpperCase(),
              addressLabel(socket.localAddress),
              <button
                key="select-port"
                type="button"
                className="network-port-button"
                aria-pressed={selected === socket}
                onClick={() => setSelectedKey(socketKey(socket))}
              >
                {socket.localPort}
              </button>,
              new Set(related.map((peer) => peer.remoteAddress)).size,
              related.length,
              formatSocketBytes(trafficTotal(related, 'txBytes')),
              formatSocketBytes(trafficTotal(related, 'rxBytes'))
            ]
          }
        })}
      />
      <div className="network-connections-detail-title">
        <span>{t.networkPeers}</span>
        {selected ? (
          <span className="network-connections-selected-endpoint">
            {addressLabel(selected.localAddress)} · {t.networkPort} {selected.localPort} ·{' '}
            {selected.protocol.toUpperCase()}
          </span>
        ) : null}
      </div>
      <NetworkSocketTable
        layoutId="network-peers"
        sort={peerSort}
        onSortChange={setPeerSort}
        headers={[t.networkRemoteAddress, t.networkPort, t.networkState, t.upload, t.download]}
        empty={t.networkNoPeers}
        rows={peers.map((peer, index) => ({
          key: peerKeys[index],
          sortValues: [peer.remoteAddress, peer.remotePort, peer.state, peer.txBytes, peer.rxBytes],
          cells: [
            peer.remoteAddress,
            peer.remotePort,
            peer.state,
            formatSocketBytes(peer.txBytes),
            formatSocketBytes(peer.rxBytes)
          ]
        }))}
      />
      <p className="network-connections-note network-connections-footer-note">{t.networkSocketHint}</p>
    </section>
  )
}
