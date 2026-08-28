import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import type {
  ConnectionProfile,
  NetworkSamplePoint,
  ResourceMonitoringMetric,
  SessionSnapshot,
  SystemMetrics
} from '@fileterm/core'
import { copyText, hasSelectedText } from '../../app/app-utils'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { DropdownSelect } from '../common/DropdownSelect'
import { VerticalScrollbar } from '../common/VerticalScrollbar'
import { formatSystemLoad } from './system-metric-format'

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
  showResourceMeters,
  visibleMetrics,
  onOpenSystemInfo,
  onToggleCollapsed
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  collapsed: boolean
  showResourceMeters: boolean
  visibleMetrics: ResourceMonitoringMetric[]
  onOpenSystemInfo(): void
  onToggleCollapsed(): void
}) {
  const [sortMode, setSortMode] = useState<'memory' | 'cpu' | 'command'>('cpu')
  const metrics = activeSession?.systemMetrics
  const internalIp = metrics?.ip || '-'
  const accessAddress = activeProfile?.host || activeSession?.accessHost || '-'
  const availableFileSystems = useMemo(
    () => (metrics?.fileSystemRows ?? []).filter((row) => row.mountPoint === '/' || !isEphemeralFileSystem(row)),
    [metrics?.fileSystemRows]
  )
  const rows = useMemo(() => {
    // Prefer the normalized filesystem payload that also drives the disk
    // meter. The legacy `diskRows` block can be absent in a partial stream,
    // which previously left this table with only its empty placeholders even
    // while the meter above already displayed the mounted root filesystem.
    // Unlike the meter selector, the table keeps every reported mount
    // (including tmpfs/devtmpfs rows) to match the historical layout.
    const normalizedRows = (metrics?.fileSystemRows ?? [])
      .map((row) => ({
        path: row.mountPoint || row.name,
        usage: `${row.available}/${row.size}`
      }))
      .filter((row) => row.path && row.usage !== '/')

    if (normalizedRows.length > 0) {
      return normalizedRows
    }

    // Preserve compatibility with older/partial collectors that only provide
    // the compact block.
    return (metrics?.diskRows ?? []).filter((row) => Boolean(row?.path && row?.usage && row.usage !== '/'))
  }, [metrics?.fileSystemRows, metrics?.diskRows])
  const defaultFileSystem = useMemo(() => selectPrimaryFileSystem(availableFileSystems), [availableFileSystems])
  const [selectedDiskMountPoint, setSelectedDiskMountPoint] = useState('')
  const diskScrollRef = useRef<HTMLDivElement>(null)
  const systemMetricsScrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    setSelectedDiskMountPoint(defaultFileSystem?.mountPoint ?? '')
  }, [activeSession?.profileId, defaultFileSystem?.mountPoint])

  useEffect(() => {
    if (availableFileSystems.some((row) => row.mountPoint === selectedDiskMountPoint)) {
      return
    }
    setSelectedDiskMountPoint(defaultFileSystem?.mountPoint ?? '')
  }, [availableFileSystems, defaultFileSystem?.mountPoint, selectedDiskMountPoint])

  useEffect(() => {
    // Reconnects reuse the sidebar DOM node, so explicitly start the resource
    // viewport at the top when the remote session changes connection state.
    if (systemMetricsScrollRef.current) {
      systemMetricsScrollRef.current.scrollTop = 0
    }
  }, [activeSession?.connected, activeSession?.profileId])

  const selectedFileSystem =
    availableFileSystems.find((row) => row.mountPoint === selectedDiskMountPoint) ?? defaultFileSystem
  const sortedProcesses = useMemo(() => {
    const procs = [...(metrics?.topProcesses ?? [])]
    if (sortMode === 'command') {
      // 按命令名字典序，便于找同名进程；同命令按 CPU 降序
      return procs
        .sort((a, b) => a.command.localeCompare(b.command) || parseFloat(b.cpu) - parseFloat(a.cpu))
        .slice(0, 40)
    }
    return procs
      .sort((a, b) => {
        if (sortMode === 'cpu') {
          return parseFloat(b.cpu) - parseFloat(a.cpu)
        }
        if (sortMode === 'memory') {
          return parseMemory(b.memory) - parseMemory(a.memory)
        }
        return 0
      })
      .slice(0, 40)
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
            <button
              className="system-title"
              data-file-panel-snap-target="system-title"
              onClick={onOpenSystemInfo}
              type="button"
            >
              {t.systemInfo}
            </button>
            <ResourceMetricCards
              availableFileSystems={availableFileSystems}
              fileSystem={selectedFileSystem}
              metrics={metrics}
              onFileSystemChange={setSelectedDiskMountPoint}
              scrollRef={systemMetricsScrollRef}
              visibleMetrics={visibleMetrics}
            />
            {visibleMetrics.includes('processes') ? (
              <ProcessMetricPanel onSortModeChange={setSortMode} rows={sortedProcesses} sortMode={sortMode} />
            ) : null}
            {visibleMetrics.includes('network') ? <NetworkMetricPanel metrics={metrics} /> : null}
          </section>
          <section className="disk-table">
            <div className="disk-head" data-file-panel-snap-target="disk-header">
              <span>{t.path}</span>
              <span>{t.availableSize}</span>
            </div>
            <div className="disk-scroll-region">
              <div className="disk-body" ref={diskScrollRef}>
                {rows.length
                  ? rows.map((row) => (
                      <div className="disk-row" key={row.path}>
                        <span>{row.path}</span>
                        <span>{row.usage}</span>
                      </div>
                    ))
                  : Array.from({ length: 8 }).map((_, i) => (
                      <div className="disk-row" key={`empty-${i}`}>
                        <span></span>
                        <span></span>
                      </div>
                    ))}
              </div>
              <VerticalScrollbar ariaLabel={t.scrollDiskList} scrollRef={diskScrollRef} />
            </div>
          </section>
        </>
      ) : showResourceMeters ? (
        <CollapsedResourceMeters fileSystem={selectedFileSystem} metrics={metrics} visibleMetrics={visibleMetrics} />
      ) : null}
    </div>
  )
}

function ResourceMetricCards({
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

function ProcessMetricPanel({
  onSortModeChange,
  rows,
  sortMode
}: {
  onSortModeChange(mode: 'memory' | 'cpu' | 'command'): void
  rows: SystemMetrics['topProcesses']
  sortMode: 'memory' | 'cpu' | 'command'
}) {
  return (
    <div className="system-process-panel">
      <div className="mini-tabs" data-file-panel-snap-target="process-tabs">
        <span className={sortMode === 'memory' ? 'active' : ''} onClick={() => onSortModeChange('memory')}>
          {t.memory}
        </span>
        <span className={sortMode === 'cpu' ? 'active' : ''} onClick={() => onSortModeChange('cpu')}>
          {t.cpu}
        </span>
        <span className={sortMode === 'command' ? 'active' : ''} onClick={() => onSortModeChange('command')}>
          {t.command}
        </span>
      </div>
      <ProcessTable rows={rows} />
    </div>
  )
}

function NetworkMetricPanel({ metrics }: { metrics?: SystemMetrics }) {
  return (
    <div className="system-network-panel">
      <NetworkPanel metrics={metrics} />
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

function MetricHoverDetail({ value, className = 'metric-caption' }: { value: string; className?: string }) {
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

function CollapsedResourceMeters({
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

function selectPrimaryFileSystem(rows: SystemMetrics['fileSystemRows']) {
  return (
    rows.find((row) => row.mountPoint === '/') ??
    rows.find((row) => /^c:\\?$/i.test(row.mountPoint)) ??
    rows.find((row) => !isEphemeralFileSystem(row))
  )
}

function isEphemeralFileSystem(row: SystemMetrics['fileSystemRows'][number]) {
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

function ProcessTable({ rows }: { rows: SystemMetrics['topProcesses'] }) {
  const processScrollRef = useRef<HTMLDivElement>(null)
  const placeholderRows = Array.from({ length: Math.max(0, 4 - rows.length) }).map(() => ({
    pid: 0,
    user: '',
    memory: '',
    cpu: '',
    command: '',
    elapsedSeconds: 0
  }))
  const displayRows = rows.length
    ? [...rows, ...placeholderRows]
    : Array.from({ length: 4 }).map(() => ({
        pid: 0,
        user: '',
        memory: '',
        cpu: '',
        command: '',
        elapsedSeconds: 0
      }))
  return (
    <div className="process-scroll-region">
      <div className="process-table" ref={processScrollRef}>
        {displayRows.map((row, i) => (
          <div
            className="process-row"
            key={row.pid === 0 && !row.command ? `empty-${i}` : `${row.pid}-${row.command}-${row.cpu}-${i}`}
          >
            <span>{row.memory}</span>
            <span>{row.cpu ? `${row.cpu}%` : ''}</span>
            <span>
              <MetricHoverDetail className="process-hover-detail" value={row.command} />
            </span>
          </div>
        ))}
      </div>
      <VerticalScrollbar ariaLabel={t.scrollContent} scrollRef={processScrollRef} />
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
    const sampleAdvanced =
      previousSampleCountRef.current !== rawSamples.length ||
      previousLastSample?.rx !== latestSample?.rx ||
      previousLastSample?.tx !== latestSample?.tx

    previousLastSampleRef.current = latestSample
    previousSampleCountRef.current = rawSamples.length

    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current)
      animationFrameRef.current = null
    }

    if (interfaceChanged) {
      setDisplaySamples((current) => (areSampleWindowsEqual(current, samples) ? current : samples))
      setChartOffset(-chartStep)
      return
    }

    if (!sampleAdvanced) {
      setDisplaySamples((current) => (areSampleWindowsEqual(current, samples) ? current : samples))
      setChartOffset((current) => (current === -chartStep ? current : -chartStep))
      return
    }

    const startTime = performance.now()
    const duration = 420

    setDisplaySamples((current) => (areSampleWindowsEqual(current, samples) ? current : samples))
    setChartOffset((current) => (current === 0 ? current : 0))

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
      <div className="network-panel" data-file-panel-snap-target="network-panel">
        <div className="network-rates">
          <span className="network-rate up">
            <i>
              <AppIcon name="arrow-up" size={12} />
            </i>
            <strong>{currentRates?.tx ?? '0B'}</strong>
          </span>
          <span className="network-rate down">
            <i>
              <AppIcon name="arrow-down" size={12} />
            </i>
            <strong>{currentRates?.rx ?? '0B'}</strong>
          </span>
        </div>
        <DropdownSelect
          className="network-select"
          align="right"
          value={selectedInterface}
          options={interfaceOptions.map((name) => ({
            value: name,
            label: name === 'all' ? t.total : name
          }))}
          onChange={(value) => setSelectedInterface(value)}
        />
      </div>
      <div className="network-history">
        <div className="network-scale">
          {chartScale.map((value) => (
            <span key={value}>{formatTrafficLabel(value)}</span>
          ))}
        </div>
        <div className="grid-chart">
          <svg
            aria-label="Network history chart"
            className="network-chart-svg"
            preserveAspectRatio="none"
            viewBox="0 0 100 100"
          >
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
