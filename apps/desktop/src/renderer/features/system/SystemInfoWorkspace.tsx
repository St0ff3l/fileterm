import type { ReactNode } from 'react'
import type { ConnectionProfile, SessionSnapshot, SystemMetrics } from '@fileterm/core'
import { t } from '../../i18n'

export function SystemInfoWorkspace({
  activeProfile,
  activeSession
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
}) {
  const metrics = activeSession?.systemMetrics

  if (!activeSession || activeSession.connected !== true || !metrics) {
    return (
      <section className="system-info-workspace system-info-workspace-empty">
        <div className="system-info-empty">
          <strong>{activeSession ? t.remoteDisconnected : t.noConnection}</strong>
          <p>{activeSession ? t.remoteDisconnectedDescription : t.noConnectionDescription}</p>
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
    { label: t.accessAddress, value: activeProfile?.host || activeSession.accessHost || '-' },
    { label: t.privateIp, value: metrics.ip || '-' },
    { label: t.running, value: formatUptime(metrics.uptimeSeconds, metrics.uptime) },
    { label: t.load, value: metrics.load || '-' }
  ]

  return (
    <section className="system-info-workspace">
      <header className="system-info-header">
        <div>
          <strong>{t.systemInfo}</strong>
          <p>{t.systemInfoDescription}</p>
        </div>
      </header>

      <div className="system-info-grid">
        <DataCard title={t.overview}>
          <DescriptionList rows={summaryRows} />
        </DataCard>

        <DataCard title={t.cpuDetails}>
          <Table
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
            columns={[t.model, t.vendor, t.driver, t.memory]}
            rows={metrics.gpuInfoRows.map((row) => [
              row.model,
              row.vendor,
              row.driver,
              row.memory
            ])}
          />
        </DataCard>

        <DataCard title={t.cpuUsage}>
          <Table
            columns={[t.user, t.system, t.nice, t.idle, t.ioWait, t.irq, t.softIrq, t.realtime]}
            rows={[[
              formatPercent(metrics.cpuUsage.user),
              formatPercent(metrics.cpuUsage.system),
              formatPercent(metrics.cpuUsage.nice),
              formatPercent(metrics.cpuUsage.idle),
              formatPercent(metrics.cpuUsage.ioWait),
              formatPercent(metrics.cpuUsage.irq),
              formatPercent(metrics.cpuUsage.softIrq),
              formatPercent(metrics.cpuUsage.steal)
            ]]}
          />
        </DataCard>

        <div className="system-info-split">
          <DataCard title={t.memoryUsageTitle}>
            <Table
              columns={[t.total, t.used, t.remaining, t.usage]}
              rows={[[
                metrics.memoryBreakdown.total,
                metrics.memoryBreakdown.used,
                metrics.memoryBreakdown.available,
                formatPercent(metrics.memoryBreakdown.percent)
              ]]}
            />
          </DataCard>
          <DataCard title={t.swapUsageTitle}>
            <Table
              columns={[t.total, t.used, t.remaining, t.usage]}
              rows={[[
                metrics.swapBreakdown.total,
                metrics.swapBreakdown.used,
                metrics.swapBreakdown.available,
                formatPercent(metrics.swapBreakdown.percent)
              ]]}
            />
          </DataCard>
        </div>

        <DataCard title={t.networkInterfaces}>
          <Table
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

function DescriptionList({ rows }: { rows: Array<{ label: string; value: string }> }) {
  return (
    <div className="system-description-list">
      {rows.map((row) => (
        <div className="system-description-row" key={row.label}>
          <span>{row.label}</span>
          <strong title={row.value}>{row.value}</strong>
        </div>
      ))}
    </div>
  )
}

function Table({
  columns,
  rows
}: {
  columns: string[]
  rows: string[][]
}) {
  return (
    <div className="system-table-shell">
      <table className="system-table">
        <thead>
          <tr>
            {columns.map((column) => <th key={column}>{column}</th>)}
          </tr>
        </thead>
        <tbody>
          {rows.length ? rows.map((row, rowIndex) => (
            <tr key={`${columns[0]}-${rowIndex}`}>
              {row.map((cell, cellIndex) => <td key={`${columns[cellIndex]}-${cellIndex}`}>{cell || '-'}</td>)}
            </tr>
          )) : (
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

  const zhDayMatch = value.match(/^(\d+)\s*天$/)
  if (zhDayMatch) {
    return `${zhDayMatch[1]}${t.uptimeDayUnit}`
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
