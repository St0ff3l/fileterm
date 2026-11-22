import type { KeyboardEvent, PointerEvent } from 'react'
import type { SystemMetrics, TopologyTcpProbeResult } from '@fileterm/core'
import { AppIcon, type AppIconName } from '../../../features/common/app-icon'
import { MaterialIcon } from '../../../components/common/material-icon/material-icon'
import { t } from '../../../i18n'
import { MaskedAddressText } from '../../system/system-resource-meters'
import { StatusIndicator, type StatusIndicatorStatus } from '../../common/status-indicator'
import {
  topologyNodeDimensions,
  topologyServiceProbeId,
  TOPOLOGY_SERVICE_RESULT_MAX_AGE_MS,
  type TopologyNode,
  type TopologyNodeMetric,
  type TopologyNodeRole
} from '../server-topology-model'

const NETWORK_ROWS_BY_SIZE = { compact: 1, standard: 4, expanded: 8 } as const
const PROCESS_ROWS_BY_SIZE = { compact: 1, standard: 2, expanded: 4 } as const
const SERVICE_ROWS_BY_SIZE = { compact: 1, standard: 2, expanded: 4 } as const

const roleLabels: Record<TopologyNodeRole, string> = {
  server: t.serverTopologyNodeRoleServer,
  firewall: t.serverTopologyNodeRoleFirewall,
  router: t.serverTopologyNodeRoleRouter,
  switch: t.serverTopologyNodeRoleSwitch,
  database: t.serverTopologyNodeRoleDatabase,
  storage: t.serverTopologyNodeRoleStorage,
  proxy: t.serverTopologyNodeRoleProxy,
  custom: t.serverTopologyNodeRoleCustom
}

const roleIcons: Record<TopologyNodeRole, AppIconName> = {
  server: 'server',
  firewall: 'shield',
  router: 'connections',
  switch: 'connections',
  database: 'database',
  storage: 'disk',
  proxy: 'cloud',
  custom: 'grid'
}

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
  role,
  protocol,
  details,
  status,
  statusLabel,
  metrics,
  serviceProbeResults,
  visibleMetrics,
  maskDetails,
  maskConnectionAddresses,
  relatedDescription,
  selected,
  positionOffset,
  onClick,
  onKeyDown,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  onPointerCancel,
  onLostPointerCapture
}: {
  node: TopologyNode
  fontScale: number
  name: string
  role: TopologyNodeRole
  protocol: string
  details: string | null
  status: StatusIndicatorStatus | null
  statusLabel: string
  metrics: SystemMetrics | null
  serviceProbeResults: Record<string, TopologyTcpProbeResult>
  visibleMetrics: TopologyNodeMetric[]
  maskDetails: boolean
  maskConnectionAddresses: boolean
  relatedDescription?: string
  selected: boolean
  positionOffset?: { x: number; y: number }
  onClick(): void
  onKeyDown(event: KeyboardEvent<HTMLButtonElement>): void
  onPointerDown(event: PointerEvent<HTMLButtonElement>): void
  onPointerMove(event: PointerEvent<HTMLButtonElement>): void
  onPointerUp(event: PointerEvent<HTMLButtonElement>): void
  onPointerCancel(event: PointerEvent<HTMLButtonElement>): void
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
      onPointerCancel={onPointerCancel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      style={{
        left: node.x + (positionOffset?.x ?? 0),
        top: node.y + (positionOffset?.y ?? 0),
        width: dimensions.width,
        height: dimensions.height
      }}
      title={`${name}${details ? ` · ${maskDetails ? '••••••••' : details}` : ''}`}
      type="button"
    >
      <span className="server-topology__node-meta">
        {protocol === 'SERIAL' ? <MaterialIcon name="usb" /> : <AppIcon name={roleIcons[role]} />}
        <span className="server-topology__node-role">{roleLabels[role]}</span>
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
      {node.services.length ? (
        <span className="server-topology__node-services">
          {node.services.slice(0, SERVICE_ROWS_BY_SIZE[node.size]).map((service) => {
            const result = serviceProbeResults[topologyServiceProbeId(node.id, service.id)]
            const fresh = result && Date.now() - result.checkedAt <= TOPOLOGY_SERVICE_RESULT_MAX_AGE_MS
            const state = fresh ? result.status : 'unknown'
            const indicatorStatus =
              state === 'reachable' ? 'connected' : state === 'unreachable' ? 'disconnected' : 'idle'
            const statusLabel =
              state === 'reachable'
                ? t.serverTopologyServiceReachable
                : state === 'unreachable'
                  ? t.serverTopologyServiceUnreachable
                  : t.serverTopologyServiceUnknown
            return (
              <span
                className="server-topology__node-service"
                key={service.id}
                title={`${service.host}:${service.port}`}
              >
                <span>{service.name}</span>
                <span className="server-topology__service-state">
                  <StatusIndicator aria-hidden="true" status={indicatorStatus} />
                  {statusLabel}
                </span>
              </span>
            )
          })}
          {node.services.length > SERVICE_ROWS_BY_SIZE[node.size] ? (
            <small>{`+${node.services.length - SERVICE_ROWS_BY_SIZE[node.size]}`}</small>
          ) : null}
        </span>
      ) : null}
    </button>
  )
}
