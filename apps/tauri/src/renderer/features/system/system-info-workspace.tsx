import '../workspace/workspace-list-rows.css'
import './system-info-header.css'
import './system-info-workspace.css'
import { useRef, type ReactNode } from 'react'
import type { ConnectionProfile, SessionSnapshot, TabStatus } from '@fileterm/core'
import { t } from '../../i18n'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { formatSystemLoad } from './system-metric-format'
import { TableColumnResizer } from '../../components/common/table-column-resizer/table-column-resizer'
import { useRememberedColumnWidths } from '../common/table-column-layout/table-column-layout'
import { MaskedAddressText, MetricHoverDetail } from './system-resource-meters'

export function SystemInfoWorkspace({
  activeProfile,
  activeSession,
  connectionStatus,
  maskConnectionAddresses
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  connectionStatus: TabStatus | null
  maskConnectionAddresses: boolean
}) {
  const metrics = activeSession?.systemMetrics
  const scrollRef = useRef<HTMLDivElement | null>(null)

  if (!activeSession) {
    return (
      <section className="system-info-workspace system-info-workspace-empty selectable-text">
        <div className="system-info-empty">
          <strong>{t.noConnection}</strong>
          <p>{t.noConnectionDescription}</p>
        </div>
      </section>
    )
  }

  const isExplicitlyDisconnected =
    connectionStatus === 'closed' ||
    connectionStatus === 'error' ||
    (connectionStatus === null && activeSession.connected !== true)

  if (isExplicitlyDisconnected) {
    return (
      <section className="system-info-workspace system-info-workspace-empty selectable-text">
        <div className="system-info-empty">
          <strong>{t.remoteDisconnected}</strong>
          <p>{t.remoteDisconnectedDescription}</p>
        </div>
      </section>
    )
  }

  if (activeSession.resourceMonitoringUnavailableReason === 'interactive-gateway-target-route-required') {
    return (
      <section className="system-info-workspace system-info-workspace-empty selectable-text">
        <div className="system-info-empty">
          <strong>{t.interactiveGatewayResourceMonitoringTitle}</strong>
          <p>{t.interactiveGatewayResourceMonitoringDescription}</p>
        </div>
      </section>
    )
  }

  if (!metrics) {
    return (
      <section className="system-info-workspace system-info-workspace-empty selectable-text">
        <div className="system-info-empty">
          <strong>{t.loadingSystemInfo}</strong>
          <p>{t.loadingSystemInfoDescription}</p>
        </div>
      </section>
    )
  }

  const summaryRows = [
    { label: t.osName, value: metrics.identity.osName },
    { label: t.kernelName, value: metrics.identity.kernelName },
    { label: t.kernelVersion, value: metrics.identity.kernelVersion },
    { label: t.architecture, value: metrics.identity.architecture },
    { label: t.hostname, value: metrics.identity.hostname },
    { label: t.accessAddress, value: activeProfile?.host || activeSession.accessHost || '-', maskable: true },
    { label: t.privateIp, value: metrics.ip || '-', maskable: true },
    { label: t.running, value: formatUptime(metrics.uptimeSeconds, metrics.uptime) },
    { label: t.load, value: formatSystemLoad(metrics, t).value }
  ]
  return (
    <section className="system-info-workspace selectable-text">
      <div className="system-info-scroll" ref={scrollRef}>
        <div className="system-info-scroll-content">
          <header className="system-info-header">
            <div>
              <strong>{t.systemInfo}</strong>
              <p>{t.systemInfoDescription}</p>
            </div>
          </header>

          <div className="system-info-grid">
            <DataCard title={t.overview}>
              <DescriptionList rows={summaryRows} maskConnectionAddresses={maskConnectionAddresses} />
            </DataCard>

            <DataCard title={t.cpuDetails}>
              <Table
                layoutId="system-cpu"
                columns={[t.model, t.cores, t.frequency, t.cache, t.bogomips]}
                rows={metrics.cpuInfoRows.map((row) => [
                  row.model,
                  String(row.cores || '-'),
                  row.frequencyMHz === '-' ? '-' : `${row.frequencyMHz} MHz`,
                  row.cache,
                  row.bogomips
                ])}
              />
            </DataCard>

            <DataCard title={t.gpuDetails}>
              <Table
                layoutId="system-gpu"
                columns={[t.model, t.vendor, t.driver, t.memory]}
                rows={metrics.gpuInfoRows.map((row) => [row.model, row.vendor, row.driver, row.memory])}
              />
            </DataCard>

            <DataCard title={t.cpuUsage}>
              <Table
                layoutId="system-cpu-usage"
                columns={[t.user, t.system, t.nice, t.idle, t.ioWait, t.irq, t.softIrq, t.realtime]}
                rows={[
                  [
                    formatPercent(metrics.cpuUsage.user),
                    formatPercent(metrics.cpuUsage.system),
                    formatPercent(metrics.cpuUsage.nice),
                    formatPercent(metrics.cpuUsage.idle),
                    formatPercent(metrics.cpuUsage.ioWait),
                    formatPercent(metrics.cpuUsage.irq),
                    formatPercent(metrics.cpuUsage.softIrq),
                    formatPercent(metrics.cpuUsage.steal)
                  ]
                ]}
              />
            </DataCard>

            <div className="system-info-split">
              <DataCard title={t.memoryUsageTitle}>
                <Table
                  layoutId="system-memory"
                  columns={[t.total, t.used, t.remaining, t.usage]}
                  rows={[
                    [
                      metrics.memoryBreakdown.total,
                      metrics.memoryBreakdown.used,
                      metrics.memoryBreakdown.available,
                      formatPercent(metrics.memoryBreakdown.percent)
                    ]
                  ]}
                />
              </DataCard>
              <DataCard title={t.swapUsageTitle}>
                <Table
                  layoutId="system-swap"
                  columns={[t.total, t.used, t.remaining, t.usage]}
                  rows={[
                    [
                      metrics.swapBreakdown.total,
                      metrics.swapBreakdown.used,
                      metrics.swapBreakdown.available,
                      formatPercent(metrics.swapBreakdown.percent)
                    ]
                  ]}
                />
              </DataCard>
            </div>

            <DataCard title={t.networkInterfaces}>
              <Table
                layoutId="system-network-interfaces"
                columns={[t.name, t.send, t.receive, t.sendRate, t.receiveRate]}
                rows={metrics.networkInterfaceRows.map((row) => [
                  row.name,
                  row.txTotal,
                  row.rxTotal,
                  row.txRate,
                  row.rxRate
                ])}
              />
            </DataCard>

            <DataCard title={t.fileSystems}>
              <Table
                layoutId="system-filesystems"
                columns={[t.name, t.size, t.usage, t.available, t.mountPoint]}
                rows={metrics.fileSystemRows.map((row) => [
                  row.name,
                  `${row.used} / ${row.size}`,
                  row.usagePercent,
                  row.available,
                  row.mountPoint
                ])}
              />
            </DataCard>
          </div>
        </div>
      </div>
      <VerticalScrollbar ariaLabel={t.scrollContent} scrollRef={scrollRef} />
    </section>
  )
}

function DataCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="system-info-card">
      <div className="system-info-card-head">
        <strong>{title}</strong>
      </div>
      {children}
    </section>
  )
}

function DescriptionList({
  rows,
  maskConnectionAddresses
}: {
  rows: Array<{ label: string; value: string; maskable?: boolean }>
  maskConnectionAddresses: boolean
}) {
  return (
    <div className="system-description-list">
      {rows.map((row) => (
        <div className="system-description-row" key={row.label}>
          <span>{row.label}</span>
          <strong
            className={maskConnectionAddresses && row.maskable && row.value !== '-' ? 'is-masked' : ''}
            tabIndex={maskConnectionAddresses && row.maskable && row.value !== '-' ? 0 : undefined}
          >
            {maskConnectionAddresses && row.maskable && row.value !== '-' ? (
              <MaskedAddressText value={row.value} focusable={false} />
            ) : (
              <MetricHoverDetail className="system-info-hover-detail" value={row.value} />
            )}
          </strong>
        </div>
      ))}
      {rows.length % 2 ? (
        <div className="system-description-row system-description-placeholder" aria-hidden="true" />
      ) : null}
    </div>
  )
}

function Table({ columns, rows, layoutId }: { columns: string[]; rows: string[][]; layoutId: string }) {
  const [widths, setWidths, commitWidths] = useRememberedColumnWidths(layoutId, columns.length)
  return (
    <div className="system-table-shell">
      <table
        className="system-table"
        style={{
          width: widths ? `max(100%, ${widths.reduce((sum, width) => sum + width, 0)}px)` : '100%',
          minWidth: widths ? undefined : columns.length * 110
        }}
      >
        {widths ? (
          <colgroup>
            {widths.map((width, index) => (
              <col key={index} style={{ width }} />
            ))}
          </colgroup>
        ) : null}
        <thead>
          <tr>
            {columns.map((column, index) => (
              <th key={column} scope="col">
                <span className="table-column-heading">{column}</span>
                {index > 0 ? (
                  <TableColumnResizer label={columns[index - 1]} onResize={setWidths} onResizeEnd={commitWidths} />
                ) : null}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.length ? (
            rows.map((row, rowIndex) => (
              <tr key={`${columns[0]}-${rowIndex}`}>
                {row.map((cell, cellIndex) => (
                  <td key={`${columns[cellIndex]}-${cellIndex}`}>
                    <MetricHoverDetail className="system-info-hover-detail" value={cell || '-'} />
                  </td>
                ))}
              </tr>
            ))
          ) : (
            <tr>
              <td colSpan={columns.length}>-</td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}

function formatPercent(value: number) {
  return `${value.toFixed(1)}%`
}

function formatUptime(uptimeSeconds?: number, fallback?: string) {
  if (!uptimeSeconds || uptimeSeconds < 0) {
    return formatLegacyUptime(fallback)
  }

  const days = Math.floor(uptimeSeconds / 86400)
  const hours = Math.floor((uptimeSeconds % 86400) / 3600)
  const minutes = Math.floor((uptimeSeconds % 3600) / 60)
  const parts: string[] = []

  if (days > 0) {
    parts.push(`${days}${t.uptimeDayUnit}`)
  }
  if (hours > 0) {
    parts.push(`${hours}${t.uptimeHourUnit}`)
  }
  if (!days && !hours && minutes > 0) {
    parts.push(`${minutes}${t.uptimeMinuteUnit}`)
  }

  return parts.length ? parts.join(' ') : t.uptimeJustNow
}

function formatLegacyUptime(fallback?: string) {
  if (!fallback) {
    return '-'
  }

  const value = fallback.trim()
  if (!value) {
    return '-'
  }

  const enDayHourMatch = value.match(/^(\d+)\s+days?,\s+(\d+):(\d+)$/i)
  if (enDayHourMatch) {
    const [, days, hours, minutes] = enDayHourMatch
    return compactUptimeParts([
      `${days}${t.uptimeDayUnit}`,
      Number(hours) > 0 ? `${Number(hours)}${t.uptimeHourUnit}` : '',
      Number(minutes) > 0 ? `${Number(minutes)}${t.uptimeMinuteUnit}` : ''
    ])
  }

  const enDayMatch = value.match(/^(\d+)\s+days?$/i)
  if (enDayMatch) {
    return `${enDayMatch[1]}${t.uptimeDayUnit}`
  }

  const enHourMinuteMatch = value.match(/^(\d+):(\d+)$/)
  if (enHourMinuteMatch) {
    const [, hours, minutes] = enHourMinuteMatch
    return compactUptimeParts([
      Number(hours) > 0 ? `${Number(hours)}${t.uptimeHourUnit}` : '',
      Number(minutes) > 0 ? `${Number(minutes)}${t.uptimeMinuteUnit}` : ''
    ])
  }

  return value
}

function compactUptimeParts(parts: string[]) {
  const filtered = parts.filter(Boolean)
  return filtered.length ? filtered.join(' ') : t.uptimeJustNow
}
