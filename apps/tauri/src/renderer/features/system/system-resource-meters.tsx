import { useCallback, useEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import type { ResourceMonitoringMetric, SystemMetrics } from '@fileterm/core'
import { copyText, hasSelectedText } from '../../app/app-utils'
import { t } from '../../i18n'
import { DropdownSelect } from '../common/dropdown-select'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { formatSystemLoad } from './system-metric-format'

export function parseMemory(memStr: string): number {
  if (!memStr) return 0
  const val = parseFloat(memStr)
  if (memStr.toUpperCase().includes('G')) return val * 1024 * 1024
  if (memStr.toUpperCase().includes('M')) return val * 1024
  if (memStr.toUpperCase().includes('K')) return val
  return val / 1024
}

export function ResourceMetricCards({
  availableFileSystems,
  fileSystem,
  metrics,
  onFileSystemChange,
  scrollRef,
  visibleMetrics
}: {
  availableFileSystems: SystemMetrics['fileSystemRows']
  fileSystem?: SystemMetrics['fileSystemRows'][number]
  metrics?: SystemMetrics
  onFileSystemChange(value: string): void
  scrollRef: RefObject<HTMLDivElement | null>
  visibleMetrics: ResourceMonitoringMetric[]
}) {
  const resourceMetrics = visibleMetrics.filter((metric) => metric !== 'processes' && metric !== 'network')
  const systemLoad = formatSystemLoad(metrics, t)

  return (
    <div className="system-metrics-scroll-region">
      <div className="system-metrics-scroll" ref={scrollRef}>
        <div className="metric-line system-running-line">
          <span>{t.running}</span>
          <strong className="value">
            <MetricHoverDetail
              className="metric-line-hover-detail"
              value={formatUptime(metrics?.uptimeSeconds, metrics?.uptime)}
            />
          </strong>
        </div>
        {resourceMetrics.map((metric) => {
          switch (metric) {
            case 'load':
              return (
                <div className="metric-line" key={metric}>
                  <span>{t.load}</span>
                  <strong className="value">
                    <MetricHoverDetail className="metric-line-hover-detail" value={systemLoad.value} />
                  </strong>
                </div>
              )
            case 'cpu':
              return (
                <Meter
                  key={metric}
                  label={t.cpu}
                  value={metrics?.cpuPercent ?? 0}
                  tone={getMetricTone(metrics?.cpuPercent ?? 0)}
                  caption=""
                  percent={metrics ? `${metrics.cpuPercent}%` : '-'}
                />
              )
            case 'memory':
              return <MemoryMeter key={metric} metrics={metrics} />
            case 'swap':
              return (
                <Meter
                  key={metric}
                  label={t.swap}
                  value={metrics?.swapPercent ?? 0}
                  tone={getMetricTone(metrics?.swapPercent ?? 0)}
                  caption={metrics?.swapUsage ?? '-'}
                  percent={metrics ? `${metrics.swapPercent}%` : '-'}
                  dotTone={getMetricTone(metrics?.swapPercent ?? 0)}
                />
              )
            case 'disk':
              return (
                <DiskMeter
                  key={metric}
                  fileSystem={fileSystem}
                  fileSystems={availableFileSystems}
                  onFileSystemChange={onFileSystemChange}
                />
              )
            case 'gpu':
            case 'gpuMemory':
            case 'gpuTemperature':
            case 'gpuPower':
              return <GpuResourceMeter key={metric} metric={metric} metrics={metrics} />
            default:
              return null
          }
        })}
      </div>
      <VerticalScrollbar ariaLabel={t.scrollContent} scrollRef={scrollRef} />
    </div>
  )
}

export function AddressLine({ label, value }: { label: string; value: string }) {
  const canCopy = value && value !== '-'
  const [copied, setCopied] = useState(false)

  return (
    <div className="address-row">
      <span>{label}</span>
      <strong
        className={`${canCopy ? 'copyable' : ''} ${copied ? 'copied' : ''}`}
        aria-label={canCopy ? (copied ? t.copied : `${t.copy}: ${value}`) : value}
        onClick={() => {
          if (canCopy && !hasSelectedText()) {
            copyText(value)
            setCopied(true)
            setTimeout(() => setCopied(false), 1500)
          }
        }}
      >
        <MetricHoverDetail className="address-hover-detail" value={copied ? t.copied : value} />
      </strong>
    </div>
  )
}

export function MetricHoverDetail({ value, className = 'metric-caption' }: { value: string; className?: string }) {
  const anchorRef = useRef<HTMLSpanElement>(null)
  const textRef = useRef<HTMLSpanElement>(null)
  const [position, setPosition] = useState<{
    left: number
    top: number
    placement: 'above' | 'below'
  } | null>(null)
  const hasDetail = Boolean(value && value !== '-')

  const updatePosition = useCallback(() => {
    const anchor = anchorRef.current
    const text = textRef.current
    if (!anchor || !text || !hasDetail || text.scrollWidth <= text.clientWidth + 1 || typeof window === 'undefined') {
      setPosition(null)
      return
    }

    const rect = anchor.getBoundingClientRect()
    const viewportMargin = 8
    const tooltipWidth = 280
    const maxLeft = Math.max(viewportMargin, window.innerWidth - viewportMargin - tooltipWidth)
    const left = Math.min(maxLeft, Math.max(viewportMargin, rect.left))
    const placement = rect.top < 72 ? 'below' : 'above'

    setPosition({
      left,
      top: placement === 'above' ? rect.top - 6 : rect.bottom + 6,
      placement
    })
  }, [hasDetail])

  useEffect(() => {
    if (!position) return
    const update = () => updatePosition()
    window.addEventListener('resize', update)
    window.addEventListener('scroll', update, true)
    return () => {
      window.removeEventListener('resize', update)
      window.removeEventListener('scroll', update, true)
    }
  }, [position, updatePosition])

  return (
    <>
      <span
        ref={anchorRef}
        className={`${className} metric-hover-detail`.trim()}
        aria-label={hasDetail ? value : undefined}
        onMouseEnter={() => {
          if (hasDetail) updatePosition()
        }}
        onMouseLeave={() => setPosition(null)}
      >
        <span ref={textRef} className="metric-hover-detail-text">
          {value}
        </span>
      </span>
      {position && typeof document !== 'undefined'
        ? createPortal(
            <span
              className={`metric-hover-detail-tooltip is-${position.placement}`}
              role="tooltip"
              style={{ left: position.left, top: position.top }}
            >
              {value}
            </span>,
            document.body
          )
        : null}
    </>
  )
}

function Meter({
  label,
  value,
  tone,
  caption,
  percent,
  dotTone
}: {
  label: ReactNode
  value: number
  tone: string
  caption: string
  percent?: string
  dotTone?: string
}) {
  return (
    <div className="meter-group">
      <div className="meter-header">
        <span className="meter-label">{label}</span>
        <strong className="metric-chip-summary">
          {dotTone && <i className={`metric-dot ${dotTone}`} />}
          <MetricHoverDetail value={caption} />
          {percent && <span className="metric-percent">{percent}</span>}
        </strong>
      </div>
      <div className="meter-track">
        <i className={`meter-fill ${tone}`} style={{ width: `${value}%` }} />
      </div>
    </div>
  )
}

function MemoryMeter({ metrics }: { metrics?: SystemMetrics }) {
  const total = parseUsageTotal(metrics?.memoryUsage)
  const app = parseMemory(metrics?.memoryAppUsage ?? '')
  const cache = parseMemory(metrics?.memoryCacheUsage ?? '')
  const kernel = parseMemory(metrics?.memoryKernelUsage ?? '')
  const memoryTone = getMetricTone(metrics?.memoryPercent ?? 0)
  // Detailed collectors use app/cache/kernel composition segments. Platforms
  // without that breakdown fall back to the same severity tone as the status dot.
  const segments =
    total > 0
      ? [
          {
            key: 'app',
            label: t.app,
            value: metrics?.memoryAppUsage ?? '-',
            width: Math.max(0, Math.min(100, (app / total) * 100))
          },
          {
            key: 'cache',
            label: t.cacheLabel,
            value: metrics?.memoryCacheUsage ?? '-',
            width: Math.max(0, Math.min(100, (cache / total) * 100))
          },
          {
            key: 'kernel',
            label: t.kernelLabel,
            value: metrics?.memoryKernelUsage ?? '-',
            width: Math.max(0, Math.min(100, (kernel / total) * 100))
          }
        ].filter((segment) => parseMemory(segment.value) > 0)
      : []

  const memoryTrackRef = useRef<HTMLDivElement>(null)
  const [memoryPopoverPosition, setMemoryPopoverPosition] = useState<{
    left: number
    top: number
    placement: 'above' | 'below'
  } | null>(null)
  const memoryPopoverCloseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const updateMemoryPopoverPosition = useCallback(() => {
    const anchor = memoryTrackRef.current
    if (!anchor || !segments.length || typeof window === 'undefined') {
      setMemoryPopoverPosition(null)
      return
    }

    const rect = anchor.getBoundingClientRect()
    const viewportMargin = 8
    const popoverWidth = 140
    const popoverHeight = segments.length * 24 + 20
    const maxLeft = Math.max(viewportMargin, window.innerWidth - viewportMargin - popoverWidth)
    const left = Math.min(maxLeft, Math.max(viewportMargin, rect.right - popoverWidth))
    const placement = rect.top >= popoverHeight + viewportMargin ? 'above' : 'below'

    setMemoryPopoverPosition({
      left,
      top: placement === 'above' ? rect.top - 6 : rect.bottom + 6,
      placement
    })
  }, [segments.length])

  const openMemoryPopover = useCallback(() => {
    if (memoryPopoverCloseTimerRef.current) {
      clearTimeout(memoryPopoverCloseTimerRef.current)
      memoryPopoverCloseTimerRef.current = null
    }
    updateMemoryPopoverPosition()
  }, [updateMemoryPopoverPosition])

  const closeMemoryPopover = useCallback(() => {
    if (memoryPopoverCloseTimerRef.current) {
      clearTimeout(memoryPopoverCloseTimerRef.current)
    }
    memoryPopoverCloseTimerRef.current = setTimeout(() => {
      setMemoryPopoverPosition(null)
      memoryPopoverCloseTimerRef.current = null
    }, 80)
  }, [])

  useEffect(() => {
    if (!memoryPopoverPosition) return
    const update = () => updateMemoryPopoverPosition()
    window.addEventListener('resize', update)
    window.addEventListener('scroll', update, true)
    return () => {
      window.removeEventListener('resize', update)
      window.removeEventListener('scroll', update, true)
    }
  }, [memoryPopoverPosition, updateMemoryPopoverPosition])

  useEffect(
    () => () => {
      if (memoryPopoverCloseTimerRef.current) {
        clearTimeout(memoryPopoverCloseTimerRef.current)
      }
    },
    []
  )

  return (
    <div className="meter-group memory-meter-group">
      <div className="meter-header">
        <span className="meter-label">{t.memory}</span>
        <strong className="metric-chip-summary">
          <i className={`metric-dot ${memoryTone}`} />
          <span className="memory-hover-value" onMouseEnter={openMemoryPopover} onMouseLeave={closeMemoryPopover}>
            <MetricHoverDetail value={metrics?.memoryUsage ?? '-'} />
          </span>
          <span className="metric-percent">{metrics ? `${metrics.memoryPercent}%` : '-'}</span>
        </strong>
      </div>
      <div className="meter-track meter-track-stacked" ref={memoryTrackRef}>
        {segments.length ? (
          segments.map((segment) => (
            <i
              className={`meter-fill stacked ${segment.key}`}
              key={segment.key}
              style={{ width: `${segment.width}%` }}
            />
          ))
        ) : (
          <i className={`meter-fill ${memoryTone}`} style={{ width: `${metrics?.memoryPercent ?? 0}%` }} />
        )}
        <span
          aria-hidden="true"
          className="memory-hover-track-target"
          onMouseEnter={openMemoryPopover}
          onMouseLeave={closeMemoryPopover}
        />
      </div>
      {memoryPopoverPosition && segments.length && typeof document !== 'undefined'
        ? createPortal(
            <div
              className={`memory-hover-popover is-${memoryPopoverPosition.placement}`}
              role="tooltip"
              style={{ left: memoryPopoverPosition.left, top: memoryPopoverPosition.top }}
            >
              {segments.map((segment) => (
                <div className="memory-hover-row" key={segment.key}>
                  <i className={`metric-dot ${segment.key}`} />
                  <span className="label">{segment.label}</span>
                  <span className="value">{segment.value}</span>
                </div>
              ))}
            </div>,
            document.body
          )
        : null}
    </div>
  )
}

function DiskMeter({
  fileSystem,
  fileSystems,
  onFileSystemChange
}: {
  fileSystem?: SystemMetrics['fileSystemRows'][number]
  fileSystems: SystemMetrics['fileSystemRows']
  onFileSystemChange(value: string): void
}) {
  const rawPercent = fileSystem ? parseFloat(fileSystem.usagePercent) : Number.NaN
  const hasUsage = Number.isFinite(rawPercent)
  const percent = hasUsage ? clampPercent(rawPercent) : 0
  const caption = fileSystem ? `${fileSystem.available} / ${fileSystem.size}` : '-'

  return (
    <Meter
      label={
        <span className="disk-meter-label">
          <span>{t.disk}</span>
          {fileSystems.length > 1 ? (
            <DropdownSelect
              ariaLabel={t.disk}
              className="disk-select"
              hideArrow
              menuPlacement="auto"
              menuWidth="auto"
              options={fileSystems.map((row) => ({ value: row.mountPoint, label: row.mountPoint }))}
              onChange={onFileSystemChange}
              value={fileSystem?.mountPoint ?? ''}
            />
          ) : null}
        </span>
      }
      value={hasUsage ? percent : 0}
      tone={getMetricTone(percent)}
      caption={caption}
      percent={hasUsage ? `${percent}%` : '-'}
      dotTone={getMetricTone(percent)}
    />
  )
}

function GpuResourceMeter({ metrics, metric }: { metrics?: SystemMetrics; metric: ResourceMonitoringMetric }) {
  const gpuRows = metrics?.gpuInfoRows ?? []

  if (!gpuRows.length) {
    return null
  }

  return (
    <div className="gpu-monitor-group">
      {gpuRows.map((row, index) => {
        const gpuLabel = gpuRows.length > 1 ? `${t.gpu} ${index + 1}` : t.gpu
        const metricLabelKey =
          metric === 'gpuMemory' ? 'gpuMemory' : metric === 'gpuTemperature' ? 'gpuTemperature' : 'gpuPower'
        const metricLabel = gpuRows.length > 1 ? `${t[metricLabelKey]} ${index + 1}` : t[metricLabelKey]
        const usage = clampPercent(row.usagePercent ?? 0)
        const memory = clampPercent(row.memoryPercent ?? 0)
        const memoryCaption = `${row.memoryUsed ?? '-'} / ${row.memory || '-'}`
        return (
          <div className="gpu-monitor-card" key={`${metric}-${row.model}-${index}`}>
            {metric === 'gpu' ? (
              <Meter
                label={gpuLabel}
                value={usage}
                tone={getMetricTone(usage)}
                caption={row.model}
                percent={row.usagePercent == null ? '-' : `${usage}%`}
              />
            ) : null}
            {metric === 'gpuMemory' ? (
              <Meter
                label={metricLabel}
                value={memory}
                tone={getMetricTone(memory)}
                caption={memoryCaption}
                percent={row.memoryPercent == null ? '-' : `${memory}%`}
                dotTone={getMetricTone(memory)}
              />
            ) : null}
            {metric === 'gpuTemperature' ? (
              <GpuMetricLine
                label={metricLabel}
                value={row.temperatureCelsius == null ? '-' : `${Math.round(row.temperatureCelsius)}°C`}
              />
            ) : null}
            {metric === 'gpuPower' ? (
              <GpuMetricLine
                label={metricLabel}
                value={
                  row.powerUsage && row.powerUsage !== '-'
                    ? `${row.powerUsage}${row.powerLimit && row.powerLimit !== '-' ? ` / ${row.powerLimit}` : ''}`
                    : '-'
                }
              />
            ) : null}
          </div>
        )
      })}
    </div>
  )
}

function GpuMetricLine({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric-line gpu-monitor-line">
      <span>{label}</span>
      <strong className="value">
        <MetricHoverDetail className="metric-line-hover-detail" value={value} />
      </strong>
    </div>
  )
}

export function CollapsedResourceMeters({
  fileSystem,
  metrics,
  visibleMetrics
}: {
  fileSystem?: SystemMetrics['fileSystemRows'][number]
  metrics?: SystemMetrics
  visibleMetrics: ResourceMonitoringMetric[]
}) {
  const diskPercent = parseUsagePercent(fileSystem?.usagePercent)
  const itemsByMetric: Partial<Record<ResourceMonitoringMetric, { label: string; value: number; detail: string }>> = {
    cpu: {
      label: t.cpu,
      value: metrics?.cpuPercent ?? 0,
      detail: metrics ? `${metrics.cpuPercent}%` : '-'
    },
    memory: {
      label: t.memory,
      value: metrics?.memoryPercent ?? 0,
      detail: metrics?.memoryUsage ?? '-'
    },
    swap: { label: t.swap, value: metrics?.swapPercent ?? 0, detail: metrics?.swapUsage ?? '-' },
    disk: {
      label: t.disk,
      value: fileSystem ? diskPercent : 0,
      detail: fileSystem ? `${fileSystem.available} / ${fileSystem.size}` : '-'
    }
  }
  const items = visibleMetrics.flatMap((key) => {
    const item = itemsByMetric[key]
    return item ? [{ key, ...item }] : []
  })

  return (
    <div className="collapsed-resource-meters" aria-label={items.map((item) => item.label).join(' ')}>
      {items.map((item) => {
        const value = clampPercent(item.value)
        const tone = getMetricTone(value)
        const title = `${item.label}: ${item.detail}${item.key === 'cpu' ? '' : ` · ${value}%`}`
        return (
          <div
            className={`collapsed-resource-meter collapsed-resource-${item.key}`}
            key={item.key}
            aria-label={title}
            title={title}
          >
            <span className="collapsed-resource-track">
              <i className={`collapsed-resource-fill ${tone}`} style={{ height: `${value}%` }} />
            </span>
            <span className="collapsed-resource-info" aria-hidden="true">
              <span className="collapsed-resource-label">{item.label}</span>
              <span className="collapsed-resource-value">{value}%</span>
            </span>
          </div>
        )
      })}
    </div>
  )
}

function clampPercent(value: number) {
  if (!Number.isFinite(value)) {
    return 0
  }
  return Math.max(0, Math.min(100, Math.round(value)))
}

function parseUsagePercent(value?: string) {
  if (!value) {
    return 0
  }
  return clampPercent(parseFloat(value))
}

export function selectPrimaryFileSystem(rows: SystemMetrics['fileSystemRows']) {
  return (
    rows.find((row) => row.mountPoint === '/') ??
    rows.find((row) => /^c:\\?$/i.test(row.mountPoint)) ??
    rows.find((row) => !isEphemeralFileSystem(row))
  )
}

export function isEphemeralFileSystem(row: SystemMetrics['fileSystemRows'][number]) {
  const mountPoint = row.mountPoint.toLowerCase()
  const name = row.name.toLowerCase()

  return (
    /^\/(?:dev|proc|sys|run|tmp)(?:\/|$)/.test(mountPoint) ||
    /^(?:tmpfs|devtmpfs|proc|sysfs|overlay|squashfs|ramfs)/.test(name)
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
