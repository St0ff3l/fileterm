import { useEffect, useMemo, useRef, useState } from 'react'
import type { ConnectionProfile, NetworkSamplePoint, SessionSnapshot, SystemMetrics } from '@termdock/core'
import { copyText, hasSelectedText } from '../../app/app-utils'
import { t } from '../../i18n'

function parseMemory(memStr: string): number {
  if (!memStr) return 0
  const val = parseFloat(memStr)
  if (memStr.toUpperCase().includes('G')) return val * 1024 * 1024
  if (memStr.toUpperCase().includes('M')) return val * 1024
  if (memStr.toUpperCase().includes('K')) return val
  return val / 1024
}

export function SystemSidebar({
  activeProfile,
  activeSession,
  collapsed,
  onOpenSystemInfo,
  onToggleCollapsed
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  collapsed: boolean
  onOpenSystemInfo(): void
  onToggleCollapsed(): void
}) {
  const [sortMode, setSortMode] = useState<'memory' | 'cpu' | 'command'>('cpu')
  const metrics = activeSession?.systemMetrics
  const internalIp = metrics?.ip || '-'
  const accessAddress = activeProfile?.host || activeSession?.accessHost || '-'
  const rows = activeSession?.systemMetrics?.diskRows ?? []

  const sortedProcesses = useMemo(() => {
    const procs = [...(metrics?.topProcesses ?? [])]
    if (sortMode === 'command') {
      return procs
        .sort((a, b) => a.elapsedSeconds - b.elapsedSeconds || parseFloat(b.cpu) - parseFloat(a.cpu))
        .slice(0, 20)
    }
    return procs.sort((a, b) => {
      if (sortMode === 'cpu') {
        return parseFloat(b.cpu) - parseFloat(a.cpu)
      }
      if (sortMode === 'memory') {
        return parseMemory(b.memory) - parseMemory(a.memory)
      }
      return 0
    }).slice(0, 4)
  }, [metrics?.topProcesses, sortMode])

  return (
    <div className={`system-sidebar-layout ${collapsed ? 'is-collapsed' : ''}`}>
      <button
        aria-label={collapsed ? t.showSystemSidebar : t.hideSystemSidebar}
        className={`system-sidebar-toggle ${collapsed ? 'is-collapsed' : ''}`}
        onClick={onToggleCollapsed}
        title={collapsed ? t.showSystemSidebar : t.hideSystemSidebar}
        type="button"
      >
        <svg
          className="system-sidebar-toggle-icon"
          width="14"
          height="14"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <rect x="1.5" y="1.5" width="13" height="13" rx="2.5" />
          <path d="M5.25 1.5V14.5" />
        </svg>
      </button>
      {!collapsed ? (
        <>
          <section className="sys-card">
            <div className="connection-summary">
              <AddressLine label={t.privateIp} value={internalIp} />
              <AddressLine label={t.accessAddress} value={accessAddress} />
            </div>
            <button className="system-title" onClick={onOpenSystemInfo} type="button">{t.systemInfo}</button>
            <div className="metric-line"><span>{t.running}</span><strong className="value">{formatUptime(metrics?.uptimeSeconds, metrics?.uptime)}</strong></div>
            <div className="metric-line"><span>{t.load}</span><strong className="value">{metrics?.load ?? '-'}</strong></div>
            <Meter
              label={t.cpu}
              value={metrics?.cpuPercent ?? 0}
              tone={getMetricTone(metrics?.cpuPercent ?? 0)}
              caption=""
              percent={metrics ? `${metrics.cpuPercent}%` : '-'}
            />
            <MemoryMeter metrics={metrics} />
            <Meter
              label={t.swap}
              value={metrics?.swapPercent ?? 0}
              tone={getMetricTone(metrics?.swapPercent ?? 0)}
              caption={metrics?.swapUsage ?? '-'}
              percent={metrics ? `${metrics.swapPercent}%` : '-'}
              dotTone={getMetricTone(metrics?.swapPercent ?? 0)}
            />
            <div className="mini-tabs">
              <span className={sortMode === 'memory' ? 'active' : ''} onClick={() => setSortMode('memory')}>{t.memory}</span>
              <span className={sortMode === 'cpu' ? 'active' : ''} onClick={() => setSortMode('cpu')}>{t.cpu}</span>
              <span className={sortMode === 'command' ? 'active' : ''} onClick={() => setSortMode('command')}>{t.command}</span>
            </div>
            <ProcessTable rows={sortedProcesses} />
            <NetworkPanel metrics={metrics} />
          </section>
          <section className="disk-table">
            <div className="disk-head"><span>{t.path}</span><span>{t.availableSize}</span></div>
            {rows.length ? rows.map((row) => (
              <div className="disk-row" key={row.path}><span>{row.path}</span><span>{row.usage}</span></div>
            )) : Array.from({ length: 8 }).map((_, i) => (
              <div className="disk-row" key={`empty-${i}`}><span></span><span></span></div>
            ))}
          </section>
        </>
      ) : null}
    </div>
  )
}

function AddressLine({ label, value }: { label: string; value: string }) {
  const canCopy = value && value !== '-'
  const [copied, setCopied] = useState(false)

  return (
    <div className="address-row">
      <span>{label}</span>
      <strong
        className={`${canCopy ? 'copyable' : ''} ${copied ? 'copied' : ''}`}
        title={canCopy ? (copied ? '已复制' : `${t.copy}: ${value}`) : value}
        onClick={() => {
          if (canCopy && !hasSelectedText()) {
            copyText(value)
            setCopied(true)
            setTimeout(() => setCopied(false), 1500)
          }
        }}
      >
        {copied ? '已复制' : value}
      </strong>
    </div>
  )
}

function Meter({ label, value, tone, caption, percent, dotTone }: { label: string; value: number; tone: string; caption: string; percent?: string; dotTone?: string }) {
  return (
    <div className="meter-group">
      <div className="meter-header">
        <span>{label}</span>
        <strong className="metric-chip-summary">
          {dotTone && <i className={`metric-dot ${dotTone}`} />}
          <span>{caption}</span>
          {percent && <span className="metric-percent">{percent}</span>}
        </strong>
      </div>
      <div className="meter-track"><i className={`meter-fill ${tone}`} style={{ width: `${value}%` }} /></div>
    </div>
  )
}

function MemoryMeter({ metrics }: { metrics?: SystemMetrics }) {
  const total = parseUsageTotal(metrics?.memoryUsage)
  const app = parseMemory(metrics?.memoryAppUsage ?? '')
  const cache = parseMemory(metrics?.memoryCacheUsage ?? '')
  const kernel = parseMemory(metrics?.memoryKernelUsage ?? '')
  const memoryTone = getMetricTone(metrics?.memoryPercent ?? 0)
  // System meters intentionally use two color protocols:
  // - tone-* for status/warning severity (cpu/swap and memory fallback)
  // - app/cache/kernel for memory composition segments
  const segments = total > 0
    ? [
        { key: 'app', label: t.app, value: metrics?.memoryAppUsage ?? '-', width: Math.max(0, Math.min(100, (app / total) * 100)) },
        { key: 'cache', label: t.cacheLabel, value: metrics?.memoryCacheUsage ?? '-', width: Math.max(0, Math.min(100, (cache / total) * 100)) },
        { key: 'kernel', label: t.kernelLabel, value: metrics?.memoryKernelUsage ?? '-', width: Math.max(0, Math.min(100, (kernel / total) * 100)) }
      ].filter((segment) => parseMemory(segment.value) > 0)
    : []

  return (
    <div className="meter-group memory-meter-group">
      <div className="meter-header">
        <span>{t.memory}</span>
        <strong className="metric-chip-summary">
          <i className={`metric-dot ${memoryTone}`} />
          <span>{metrics?.memoryUsage ?? '-'}</span>
          <span className="metric-percent">{metrics ? `${metrics.memoryPercent}%` : '-'}</span>
        </strong>
      </div>
      <div className="meter-track meter-track-stacked">
        {segments.length ? segments.map((segment) => (
          <i
            className={`meter-fill stacked ${segment.key}`}
            key={segment.key}
            style={{ width: `${segment.width}%` }}
          />
        )) : <i className="meter-fill tone-warning" style={{ width: `${metrics?.memoryPercent ?? 0}%` }} />}

        {segments.length ? (
          <div className="memory-hover-popover">
            {segments.map((segment) => (
              <div className="memory-hover-row" key={segment.key}>
                <i className={`metric-dot ${segment.key}`} />
                <span className="label">{segment.label}</span>
                <span className="value">{segment.value}</span>
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  )
}

function parseUsageTotal(usage?: string) {
  if (!usage || !usage.includes('/')) return 0
  return parseMemory(usage.split('/')[1] ?? '')
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

function getMetricTone(percent: number) {
  // Status meters use the shared tone-* classes so dot/fill styling stays aligned.
  if (percent >= 85) return 'tone-danger'
  if (percent >= 60) return 'tone-warning'
  return 'tone-success'
}

function ProcessTable({ rows }: { rows: SystemMetrics['topProcesses'] }) {
  const displayRows = rows.length ? rows : Array.from({ length: 4 }).map(() => ({ memory: '', cpu: '', command: '', elapsedSeconds: 0 }))
  return (
    <div className="process-table scrollbar-scroll">
      {displayRows.map((row, i) => (
        <div className="process-row" key={rows.length ? `${row.command}-${row.memory}-${row.cpu}-${row.elapsedSeconds}` : `empty-${i}`}>
          <span>{row.memory}</span>
          <span>{row.cpu}</span>
          <span>{row.command}</span>
        </div>
      ))}
    </div>
  )
}

function buildLinePath(samples: NetworkSamplePoint[], key: 'rx' | 'tx', maxValue: number) {
  const width = 100
  const height = 100

  if (!samples.length) {
    return ''
  }

  if (samples.length === 1) {
    const y = height - (samples[0][key] / maxValue) * height
    return `M 0 ${y.toFixed(2)} L ${width} ${y.toFixed(2)}`
  }

  const points = samples.map((sample, index) => {
    const x = (index / (samples.length - 1)) * width
    const y = height - (sample[key] / maxValue) * height
    return { x, y }
  })

  let path = `M ${points[0].x.toFixed(2)} ${points[0].y.toFixed(2)}`

  for (let index = 0; index < points.length - 1; index += 1) {
    const current = points[index]
    const next = points[index + 1]
    const controlX = (next.x - current.x) / 2

    path += ` C ${(current.x + controlX).toFixed(2)} ${current.y.toFixed(2)}, ${(next.x - controlX).toFixed(2)} ${next.y.toFixed(2)}, ${next.x.toFixed(2)} ${next.y.toFixed(2)}`
  }

  return path
}

function buildScrollingWindow(samples: NetworkSamplePoint[], visibleCount: number) {
  const windowSize = visibleCount + 1
  const padded = Array.from({ length: Math.max(0, windowSize - samples.length) }, () => ({ rx: 0, tx: 0 }))
  return [...padded, ...samples].slice(-windowSize)
}

function areSampleWindowsEqual(left: NetworkSamplePoint[], right: NetworkSamplePoint[]) {
  if (left === right) {
    return true
  }
  if (left.length !== right.length) {
    return false
  }

  for (let index = 0; index < left.length; index += 1) {
    if (left[index]?.rx !== right[index]?.rx || left[index]?.tx !== right[index]?.tx) {
      return false
    }
  }

  return true
}

function formatTrafficLabel(value: number) {
  if (value >= 1024 * 1024) {
    return `${(value / 1024 / 1024).toFixed(value >= 10 * 1024 * 1024 ? 0 : 1)}M`
  }
  if (value >= 1024) {
    return `${Math.round(value / 1024)}K`
  }
  return `${Math.round(value)}B`
}

function NetworkPanel({ metrics }: { metrics?: SystemMetrics }) {
  const visibleSampleCount = 64
  const chartStep = 100 / Math.max(1, visibleSampleCount - 1)
  const [selectedInterface, setSelectedInterface] = useState(metrics?.activeNetworkInterface ?? '')
  const interfaceOptions = metrics?.networkInterfaces.length ? metrics.networkInterfaces : ['-']
  const currentRates = metrics?.networkRatesByInterface?.[selectedInterface] ?? metrics?.networkRates
  const rawSamples = metrics?.networkSamplesByInterface?.[selectedInterface]?.length
    ? metrics.networkSamplesByInterface[selectedInterface]
    : metrics?.networkSamples.length
      ? metrics.networkSamples
      : []
  const samples = useMemo(() => buildScrollingWindow(rawSamples, visibleSampleCount), [rawSamples])
  const [displaySamples, setDisplaySamples] = useState(samples)
  const [chartOffset, setChartOffset] = useState(-chartStep)
  const animationFrameRef = useRef<number | null>(null)
  const previousInterfaceRef = useRef(selectedInterface)
  const previousLastSampleRef = useRef(rawSamples.at(-1))
  const previousSampleCountRef = useRef(rawSamples.length)

  const activityValues = displaySamples.map((sample) => Math.max(sample.rx, sample.tx))
  const maxValue = Math.max(...activityValues, 1)
  const txPath = buildLinePath(displaySamples, 'tx', maxValue)
  const rxPath = buildLinePath(displaySamples, 'rx', maxValue)
  const chartScale = [maxValue, maxValue * 0.66, maxValue * 0.33]

  useEffect(() => {
    if (!interfaceOptions.includes(selectedInterface)) {
      setSelectedInterface(metrics?.activeNetworkInterface ?? interfaceOptions[0] ?? '')
    }
  }, [interfaceOptions, metrics?.activeNetworkInterface, selectedInterface])

  useEffect(() => {
    const interfaceChanged = previousInterfaceRef.current !== selectedInterface
    previousInterfaceRef.current = selectedInterface
    const latestSample = rawSamples.at(-1)
    const previousLastSample = previousLastSampleRef.current
    const sampleAdvanced = previousSampleCountRef.current !== rawSamples.length
      || previousLastSample?.rx !== latestSample?.rx
      || previousLastSample?.tx !== latestSample?.tx

    previousLastSampleRef.current = latestSample
    previousSampleCountRef.current = rawSamples.length

    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current)
      animationFrameRef.current = null
    }

    if (interfaceChanged) {
      setDisplaySamples((current) => areSampleWindowsEqual(current, samples) ? current : samples)
      setChartOffset(-chartStep)
      return
    }

    if (!sampleAdvanced) {
      setDisplaySamples((current) => areSampleWindowsEqual(current, samples) ? current : samples)
      setChartOffset((current) => current === -chartStep ? current : -chartStep)
      return
    }

    const startTime = performance.now()
    const duration = 420

    setDisplaySamples((current) => areSampleWindowsEqual(current, samples) ? current : samples)
    setChartOffset((current) => current === 0 ? current : 0)

    const animate = (now: number) => {
      const progress = Math.min(1, (now - startTime) / duration)
      const eased = 1 - Math.pow(1 - progress, 3)
      setChartOffset(-chartStep * eased)

      if (progress < 1) {
        animationFrameRef.current = requestAnimationFrame(animate)
      } else {
        animationFrameRef.current = null
      }
    }

    animationFrameRef.current = requestAnimationFrame(animate)

    return () => {
      if (animationFrameRef.current !== null) {
        cancelAnimationFrame(animationFrameRef.current)
        animationFrameRef.current = null
      }
    }
  }, [samples, selectedInterface])

  return (
    <>
      <div className="network-panel">
        <div className="network-rates">
          <span className="network-rate up">
            <i>↑</i>
            <strong>{currentRates?.tx ?? '0B'}</strong>
          </span>
          <span className="network-rate down">
            <i>↓</i>
            <strong>{currentRates?.rx ?? '0B'}</strong>
          </span>
        </div>
        <select
          className="network-select"
          value={selectedInterface}
          onChange={(event) => setSelectedInterface(event.target.value)}
        >
          {interfaceOptions.map((name) => (
            <option key={name} value={name}>{name === 'all' ? t.total : name}</option>
          ))}
        </select>
      </div>
      <div className="network-history">
        <div className="network-scale">
          {chartScale.map((value) => (
            <span key={value}>{formatTrafficLabel(value)}</span>
          ))}
        </div>
        <div className="grid-chart">
          <svg aria-label="Network history chart" className="network-chart-svg" preserveAspectRatio="none" viewBox="0 0 100 100">
            <path className="network-guide major" d="M 0 12 H 100" />
            <path className="network-guide minor" d="M 0 44 H 100" />
            <path className="network-guide minor" d="M 0 76 H 100" />
            <g transform={`translate(${chartOffset} 0)`}>
              <path className="network-path tx-path" d={txPath} />
              <path className="network-path rx-path" d={rxPath} />
            </g>
          </svg>
        </div>
      </div>
    </>
  )
}
