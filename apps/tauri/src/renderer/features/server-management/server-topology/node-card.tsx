import type { KeyboardEvent, PointerEvent } from 'react'
import type { SystemMetrics } from '@fileterm/core'
import { MaterialIcon } from '../../../components/common/material-icon/material-icon'
import { t } from '../../../i18n'
import { MaskedAddressText } from '../../system/system-resource-meters'
import { StatusIndicator, type StatusIndicatorStatus } from '../../common/status-indicator'
import { topologyNodeDimensions, type TopologyNode, type TopologyNodeMetric } from '../server-topology-model'

const NETWORK_ROWS_BY_SIZE = { compact: 1, standard: 4, expanded: 8 } as const
const PROCESS_ROWS_BY_SIZE = { compact: 1, standard: 2, expanded: 4 } as const

function percent(value: number | undefined | null) {
  return value === undefined || value === null || !Number.isFinite(value)
    ? null
    : `${Math.round(Math.max(0, Math.min(100, value)))}%`
}

function metricValue(metric: TopologyNodeMetric, metrics: SystemMetrics | null) {
  if (!metrics) return null
  if (metric === 'cpu') return percent(metrics.cpuPercent)
  if (metric === 'memory') {
    const usage = percent(metrics.memoryPercent)
    return usage ? `${usage}${metrics.memoryUsage ? ` · ${metrics.memoryUsage}` : ''}` : null
  }
  if (metric === 'disk') {
    const filesystem = metrics.fileSystemRows.find((row) => row.mountPoint === '/') ?? metrics.fileSystemRows[0]
    return filesystem ? `${filesystem.usagePercent} · ${filesystem.used} / ${filesystem.size}` : null
  }
  if (metric === 'gpu') {
    const gpu = metrics.gpuInfoRows[0]
    if (!gpu) return null
    const usage = percent(gpu.usagePercent)
    const label = gpu.model || t.gpu
    return `${label}${usage ? ` · ${usage}` : ''}${metrics.gpuInfoRows.length > 1 ? ` · +${metrics.gpuInfoRows.length - 1}` : ''}`
  }
  return null
}

export function TopologyNodeCard({
  node,
  fontScale,
  name,
  protocol,
  details,
  status,
  statusLabel,
  metrics,
  visibleMetrics,
  maskDetails,
  maskConnectionAddresses,
  relatedDescription,
  selected,
  onClick,
  onKeyDown,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  onLostPointerCapture
}: {
  node: TopologyNode
  fontScale: number
  name: string
  protocol: string
  details: string | null
  status: StatusIndicatorStatus | null
  statusLabel: string
  metrics: SystemMetrics | null
  visibleMetrics: TopologyNodeMetric[]
  maskDetails: boolean
  maskConnectionAddresses: boolean
  relatedDescription?: string
  selected: boolean
  onClick(): void
  onKeyDown(event: KeyboardEvent<HTMLButtonElement>): void
  onPointerDown(event: PointerEvent<HTMLButtonElement>): void
  onPointerMove(event: PointerEvent<HTMLButtonElement>): void
  onPointerUp(event: PointerEvent<HTMLButtonElement>): void
  onLostPointerCapture(event: PointerEvent<HTMLButtonElement>): void
}) {
  const dimensions = topologyNodeDimensions(node, fontScale)
  const networkRows = metrics?.networkInterfaceRows ?? []
  const visibleNetworkRows = networkRows.slice(0, NETWORK_ROWS_BY_SIZE[node.size])
  const processes = [...(metrics?.topProcesses ?? [])]
    .sort((first, second) => Number.parseFloat(second.cpu) - Number.parseFloat(first.cpu))
    .slice(0, PROCESS_ROWS_BY_SIZE[node.size])

  return (
    <button
      aria-description={relatedDescription}
      aria-pressed={selected}
      className={`server-topology__node is-${node.size}`}
      data-node-id={node.id}
      onClick={onClick}
      onKeyDown={onKeyDown}
      onLostPointerCapture={onLostPointerCapture}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      style={{ left: node.x, top: node.y, width: dimensions.width, height: dimensions.height }}
      title={`${name}${details ? ` · ${maskDetails ? '••••••••' : details}` : ''}`}
      type="button"
    >
      <span className="server-topology__node-meta">
        <MaterialIcon name={protocol === 'SERIAL' ? 'usb' : 'dns'} />
        <span>{protocol}</span>
        {status ? <StatusIndicator role="img" aria-label={statusLabel} title={statusLabel} status={status} /> : null}
      </span>
      <strong>{name}</strong>
      {details ? (
        <small>{maskDetails ? <MaskedAddressText focusable={false} value={details} /> : details}</small>
      ) : null}
      {visibleMetrics.length ? (
        <span className="server-topology__node-metrics">
          {visibleMetrics.map((metric) => {
            if (metric === 'ip') {
              const value = metrics?.ip || null
              return (
                <span className="server-topology__node-metric" key={metric}>
                  <span>{t.privateIp}</span>
                  <span className="server-topology__metric-value">
                    {value ? (
                      maskConnectionAddresses ? (
                        <MaskedAddressText focusable={false} value={value} />
                      ) : (
                        value
                      )
                    ) : (
                      t.serverDashboardNoMetrics
                    )}
                  </span>
                </span>
              )
            }
            if (metric === 'network') {
              return (
                <span className="server-topology__node-network" key={metric}>
                  {visibleNetworkRows.length ? (
                    visibleNetworkRows.map((row) => (
                      <span className="server-topology__node-metric" key={row.name}>
                        <span>{row.name}</span>
                        <span className="server-topology__metric-value">
                          ↓ {row.rxRate} · ↑ {row.txRate}
                        </span>
                      </span>
                    ))
                  ) : (
                    <span className="server-topology__node-metric">
                      <span>{t.resourceMonitoringNetwork}</span>
                      <span className="server-topology__metric-value">{t.serverDashboardNoMetrics}</span>
                    </span>
                  )}
                  {networkRows.length > visibleNetworkRows.length ? (
                    <small className="server-topology__more-interfaces">
                      {t.serverTopologyMoreInterfaces.replace(
                        '{count}',
                        String(networkRows.length - visibleNetworkRows.length)
                      )}
                    </small>
                  ) : null}
                </span>
              )
            }
            if (metric === 'processes') {
              return (
                <span className="server-topology__node-processes" key={metric}>
                  {processes.length ? (
                    processes.map((process) => {
                      const command = process.command.trim().split(/\s+/, 1)[0].split(/[\\/]/).pop()
                      const processName = command || `${t.resourceMonitoringProcesses} ${process.pid}`
                      return (
                        <span className="server-topology__node-metric" key={process.pid}>
                          <span>{processName}</span>
                          <span className="server-topology__metric-value">
                            {process.cpu}% · {process.memory}
                          </span>
                        </span>
                      )
                    })
                  ) : (
                    <span className="server-topology__node-metric">
                      <span>{t.resourceMonitoringProcesses}</span>
                      <span className="server-topology__metric-value">{t.serverDashboardNoMetrics}</span>
                    </span>
                  )}
                </span>
              )
            }
            return (
              <span className="server-topology__node-metric" key={metric}>
                <span>
                  {metric === 'cpu' ? t.cpu : metric === 'memory' ? t.memory : metric === 'disk' ? t.disk : t.gpu}
                </span>
                <span className="server-topology__metric-value">
                  {metricValue(metric, metrics) ?? t.serverDashboardNoMetrics}
                </span>
              </span>
            )
          })}
        </span>
      ) : null}
    </button>
  )
}
