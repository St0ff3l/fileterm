import { useEffect, useMemo, useRef, useState } from 'react'
import type { ConnectionProfile, NetworkSamplePoint, SessionSnapshot, SystemMetrics } from '@termdock/core'
import { copyText } from '../../app/app-utils'
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
  activeSession
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
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
    }).slice(0, 20)
  }, [metrics?.topProcesses, sortMode])

  return (
    <>
      <section className="sys-card">
        <div className="connection-summary">
          <AddressLine label={t.privateIp} value={internalIp} />
          <AddressLine label={t.accessAddress} value={accessAddress} />
        </div>
        <button className="system-title" type="button">{t.systemInfo}</button>
        <div className="metric-line"><span>{t.running}</span><strong>{metrics?.uptime ?? '-'}</strong></div>
        <div className="metric-line"><span>{t.load}</span><strong>{metrics?.load ?? '-'}</strong></div>
        <Meter label={t.cpu} value={metrics?.cpuPercent ?? 0} tone="green" caption={metrics ? `${metrics.cpuPercent}%` : '0%'} />
        <MemoryMeter metrics={metrics} />
        <Meter label={t.swap} value={metrics?.swapPercent ?? 0} tone="yellow" caption={metrics?.swapUsage ?? '0/0'} />
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
        {rows.map((row) => (
          <div className="disk-row" key={row.path}><span>{row.path}</span><span>{row.usage}</span></div>
        ))}
      </section>
    </>
  )
}

function AddressLine({ label, value }: { label: string; value: string }) {
  const canCopy = value && value !== '-'

  return (
    <div className="address-row">
      <span>{label}</span>
      <strong title={value}>{value}</strong>
      <button
        className="copy-link"
        disabled={!canCopy}
        onClick={() => {
          if (canCopy) {
            copyText(value)
          }
        }}
        type="button"
      >
        复制
      </button>
    </div>
  )
}

function Meter({ label, value, tone, caption }: { label: string; value: number; tone: string; caption: string }) {
  return (
    <div className="meter-row">
      <span>{label}</span>
      <div className="meter-track"><i className={`meter-fill ${tone}`} style={{ width: `${value}%` }} /></div>
      <strong>{caption}</strong>
    </div>
  )
}

function MemoryMeter({ metrics }: { metrics?: SystemMetrics }) {
  const total = parseUsageTotal(metrics?.memoryUsage)
  const app = parseMemory(metrics?.memoryAppUsage ?? '')
  const cache = parseMemory(metrics?.memoryCacheUsage ?? '')
  const kernel = parseMemory(metrics?.memoryKernelUsage ?? '')
  const segments = total > 0
    ? [
        { key: 'app', label: '应用', value: metrics?.memoryAppUsage ?? '-', width: Math.max(0, Math.min(100, (app / total) * 100)) },
        { key: 'cache', label: '缓存', value: metrics?.memoryCacheUsage ?? '-', width: Math.max(0, Math.min(100, (cache / total) * 100)) },
        { key: 'kernel', label: '内核', value: metrics?.memoryKernelUsage ?? '-', width: Math.max(0, Math.min(100, (kernel / total) * 100)) }
      ].filter((segment) => parseMemory(segment.value) > 0)
    : []

  return (
    <>
      <div className="meter-row">
        <span>{t.memory}</span>
        <div className="meter-track meter-track-stacked">
          {segments.length ? segments.map((segment) => (
            <i
              className={`meter-fill stacked ${segment.key}`}
              key={segment.key}
              style={{ width: `${segment.width}%` }}
            />
          )) : <i className="meter-fill orange" style={{ width: `${metrics?.memoryPercent ?? 0}%` }} />}
        </div>
        <strong>{metrics?.memoryUsage ?? '0/0'}</strong>
      </div>
      {segments.length ? (
        <div className="metric-subline legend">
          {segments.map((segment) => (
            <span className="metric-chip" key={segment.key}>
              <i className={`metric-dot ${segment.key}`} />
              {segment.label} {segment.value}
            </span>
          ))}
        </div>
      ) : null}
    </>
  )
}

function parseUsageTotal(usage?: string) {
  if (!usage || !usage.includes('/')) return 0
  return parseMemory(usage.split('/')[1] ?? '')
}

function ProcessTable({ rows }: { rows: SystemMetrics['topProcesses'] }) {
  return (
    <div className="process-table scrollbar-scroll">
      {rows.length ? rows.map((row) => (
        <div className="process-row" key={`${row.command}-${row.memory}-${row.cpu}-${row.elapsedSeconds}`}>
          <span>{row.memory}</span>
          <span>{row.cpu}</span>
          <span>{row.command}</span>
        </div>
      )) : <div className="process-empty" />}
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

function resampleSeries(samples: NetworkSamplePoint[], targetLength: number) {
  if (!samples.length) {
    return Array.from({ length: targetLength }, () => ({ rx: 0, tx: 0 }))
  }

  if (samples.length === targetLength) {
    return samples
  }

  if (samples.length > targetLength) {
    return Array.from({ length: targetLength }, (_value, index) => {
      const start = Math.floor((index / targetLength) * samples.length)
      const end = Math.floor(((index + 1) / targetLength) * samples.length)
      const bucket = samples.slice(start, Math.max(start + 1, end))
      const rx = bucket.reduce((sum, item) => sum + item.rx, 0) / bucket.length
      const tx = bucket.reduce((sum, item) => sum + item.tx, 0) / bucket.length

      return {
        rx: Math.round(rx),
        tx: Math.round(tx)
      }
    })
  }

  return Array.from({ length: targetLength }, (_value, index) => {
    if (targetLength === 1) {
      return samples[0]
    }

    const position = (index / (targetLength - 1)) * (samples.length - 1)
    const leftIndex = Math.floor(position)
    const rightIndex = Math.min(samples.length - 1, Math.ceil(position))
    const progress = position - leftIndex
    const left = samples[leftIndex]
    const right = samples[rightIndex]

    return {
      rx: Math.round(left.rx + (right.rx - left.rx) * progress),
      tx: Math.round(left.tx + (right.tx - left.tx) * progress)
    }
  })
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
  const [selectedInterface, setSelectedInterface] = useState(metrics?.activeNetworkInterface ?? '')
  const interfaceOptions = metrics?.networkInterfaces.length ? metrics.networkInterfaces : ['-']
  const currentRates = metrics?.networkRatesByInterface?.[selectedInterface] ?? metrics?.networkRates
  const rawSamples = metrics?.networkSamplesByInterface?.[selectedInterface]?.length
    ? metrics.networkSamplesByInterface[selectedInterface]
    : metrics?.networkSamples.length
      ? metrics.networkSamples
      : Array.from({ length: 120 }, () => ({ rx: 0, tx: 0 }))
  const samples = useMemo(() => resampleSeries(rawSamples, 64), [rawSamples])
  const [displaySamples, setDisplaySamples] = useState(samples)
  const animationFrameRef = useRef<number | null>(null)
  const previousInterfaceRef = useRef(selectedInterface)

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

    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current)
      animationFrameRef.current = null
    }

    if (interfaceChanged) {
      setDisplaySamples(samples)
      return
    }

    const fromSamples = displaySamples.length === samples.length ? displaySamples : samples
    const startTime = performance.now()
    const duration = 260

    const animate = (now: number) => {
      const progress = Math.min(1, (now - startTime) / duration)
      const eased = 1 - Math.pow(1 - progress, 3)

      setDisplaySamples(
        samples.map((sample, index) => ({
          rx: Math.round(fromSamples[index].rx + (sample.rx - fromSamples[index].rx) * eased),
          tx: Math.round(fromSamples[index].tx + (sample.tx - fromSamples[index].tx) * eased)
        }))
      )

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
            <i>UP</i>
            <strong>{currentRates?.tx ?? '0B'}</strong>
          </span>
          <span className="network-rate down">
            <i>DN</i>
            <strong>{currentRates?.rx ?? '0B'}</strong>
          </span>
        </div>
        <select
          className="network-select"
          value={selectedInterface}
          onChange={(event) => setSelectedInterface(event.target.value)}
        >
          {interfaceOptions.map((name) => (
            <option key={name} value={name}>{name === 'all' ? '总计' : name}</option>
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
            <path className="network-path tx-path" d={txPath} />
            <path className="network-path rx-path" d={rxPath} />
          </svg>
        </div>
      </div>
    </>
  )
}
