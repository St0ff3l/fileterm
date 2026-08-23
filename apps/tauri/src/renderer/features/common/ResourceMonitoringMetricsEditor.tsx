import { useRef, useState } from 'react'
import { DEFAULT_RESOURCE_MONITORING_METRIC_ORDER, type ResourceMonitoringMetric } from '@fileterm/core'
import { usePointerSortFallback, type PointerSortTarget } from '../../hooks/usePointerSortFallback'
import { t, type LocaleMessages } from '../../i18n'
import { AppIcon } from './AppIcon'
import { managerDropClass, resolveManagerDropPosition, type ManagerDropPosition } from './manager-drag'
import { targetsNestedManagerControl } from './manager-interactions'

/** Meter cards rendered inside the sidebar metric scroll region. */
const RESOURCE_MONITORING_METER_OPTIONS: Array<{
  key: ResourceMonitoringMetric
  labelKey: keyof LocaleMessages
}> = [
  { key: 'load', labelKey: 'load' },
  { key: 'cpu', labelKey: 'cpu' },
  { key: 'memory', labelKey: 'memory' },
  { key: 'swap', labelKey: 'swap' },
  { key: 'disk', labelKey: 'disk' },
  { key: 'gpu', labelKey: 'gpu' },
  { key: 'gpuMemory', labelKey: 'gpuMemory' },
  { key: 'gpuTemperature', labelKey: 'gpuTemperature' },
  { key: 'gpuPower', labelKey: 'gpuPower' }
]

/** Standalone sidebar panels rendered as siblings of the metric cards. */
const RESOURCE_MONITORING_PANEL_OPTIONS: Array<{
  key: ResourceMonitoringMetric
  labelKey: keyof LocaleMessages
}> = [
  { key: 'processes', labelKey: 'resourceMonitoringProcesses' },
  { key: 'network', labelKey: 'resourceMonitoringNetwork' }
]

const METER_KEYS = new Set(RESOURCE_MONITORING_METER_OPTIONS.map((option) => option.key))

/**
 * Shared sidebar-metric card editor (checkbox list + pointer drag reorder).
 * Used by the settings connection-defaults section and the per-connection
 * resource monitoring section of the connection form.
 */
export function ResourceMonitoringMetricsEditor({
  metrics,
  order,
  disabled = false,
  onMetricsChange,
  onOrderChange
}: {
  metrics: ResourceMonitoringMetric[]
  order: ResourceMonitoringMetric[]
  disabled?: boolean
  onMetricsChange(next: ResourceMonitoringMetric[]): void
  onOrderChange(next: ResourceMonitoringMetric[]): void
}) {
  const [draggingMetric, setDraggingMetric] = useState<ResourceMonitoringMetric | null>(null)
  const [dragOverMetric, setDragOverMetric] = useState<ResourceMonitoringMetric | null>(null)
  const [dragPosition, setDragPosition] = useState<ManagerDropPosition | null>(null)
  const dragStateRef = useRef<{
    source: ResourceMonitoringMetric | null
    target: ResourceMonitoringMetric | null
    position: ManagerDropPosition | null
  }>({ source: null, target: null, position: null })
  const suppressCardClickRef = useRef(false)

  const clearDragState = () => {
    dragStateRef.current = { source: null, target: null, position: null }
    setDraggingMetric(null)
    setDragOverMetric(null)
    setDragPosition(null)
    window.setTimeout(() => {
      suppressCardClickRef.current = false
    }, 0)
  }

  const setDropTarget = (target: ResourceMonitoringMetric, position: ManagerDropPosition) => {
    if (dragStateRef.current.target === target && dragStateRef.current.position === position) {
      return
    }

    dragStateRef.current.target = target
    dragStateRef.current.position = position
    setDragOverMetric(target)
    setDragPosition(position)
  }

  const positionForTarget = (target: PointerSortTarget, clientY: number) => {
    if (target.kind === 'resource-monitoring-metric-top') {
      return 'top' as const
    }

    return resolveManagerDropPosition(target.element, clientY, false)
  }

  const applyDrop = (
    source: ResourceMonitoringMetric,
    target: ResourceMonitoringMetric,
    position: ManagerDropPosition
  ) => {
    if (source === target || position === 'inside' || disabled) {
      return
    }

    const previousOrder = order
    const nextOrder = previousOrder.filter((metric) => metric !== source)
    const targetIndex = nextOrder.indexOf(target)
    if (targetIndex === -1) {
      return
    }

    nextOrder.splice(position === 'bottom' ? targetIndex + 1 : targetIndex, 0, source)
    if (nextOrder.every((metric, index) => metric === previousOrder[index])) {
      return
    }

    onOrderChange(nextOrder)
  }

  const handlePointerDown = usePointerSortFallback<ResourceMonitoringMetric>({
    onStart: (metric) => {
      if (disabled) {
        return
      }
      suppressCardClickRef.current = true
      dragStateRef.current = { source: metric, target: null, position: null }
      setDraggingMetric(metric)
    },
    onTarget: (source, target, clientY) => {
      if (
        source === target.id ||
        (target.kind !== 'resource-monitoring-metric' && target.kind !== 'resource-monitoring-metric-top')
      ) {
        return
      }
      setDropTarget(target.id as ResourceMonitoringMetric, positionForTarget(target, clientY))
    },
    onDrop: (source, target, clientY) => {
      if (
        target &&
        (target.kind === 'resource-monitoring-metric' || target.kind === 'resource-monitoring-metric-top') &&
        source !== target.id
      ) {
        applyDrop(source, target.id as ResourceMonitoringMetric, positionForTarget(target, clientY))
      }
      clearDragState()
    },
    onCancel: clearDragState
  })

  const toggleMetric = (metric: ResourceMonitoringMetric, enabled: boolean) => {
    const next = enabled ? Array.from(new Set([...metrics, metric])) : metrics.filter((item) => item !== metric)
    if (JSON.stringify(metrics) === JSON.stringify(next)) {
      return
    }

    onMetricsChange(next)
  }

  const resetOrder = () => {
    const nextOrder = [...DEFAULT_RESOURCE_MONITORING_METRIC_ORDER]
    if (nextOrder.every((metric, index) => metric === order[index])) {
      return
    }

    onOrderChange(nextOrder)
  }

  const meterOrder = order.filter((key) => METER_KEYS.has(key))
  const metersEnabled = meterOrder.some((key) => metrics.includes(key))

  const toggleMeterSection = (enabled: boolean) => {
    const next = enabled
      ? Array.from(new Set([...metrics, ...meterOrder]))
      : metrics.filter((key) => !METER_KEYS.has(key))
    if (next.length === metrics.length && next.every((key) => metrics.includes(key))) {
      return
    }

    onMetricsChange(next)
  }

  return (
    <div className="resource-monitoring-items-section">
      <label className="resource-monitoring-panel-row">
        <span className="resource-monitoring-panel-name">{t.resourceMonitoringItems}</span>
        <span className="command-toggle resource-monitoring-panel-toggle">
          <input
            checked={metersEnabled}
            disabled={disabled}
            onChange={(event) => toggleMeterSection(event.target.checked)}
            type="checkbox"
          />
        </span>
      </label>
      {metersEnabled ? (
        <>
          <div className="resource-monitoring-items-header">
            <p>{t.resourceMonitoringItemsHint}</p>
            <button
              className="resource-monitoring-reset-button"
              disabled={disabled}
              onClick={resetOrder}
              title={t.resourceMonitoringResetOrder}
              type="button"
            >
              <AppIcon name="refresh" size={13} />
              <span>{t.resourceMonitoringResetOrder}</span>
            </button>
          </div>
          <div className="resource-monitoring-items">
            {draggingMetric && meterOrder[0] ? (
              <div
                aria-hidden="true"
                className="resource-monitoring-top-drop-zone"
                data-fileterm-sort-id={meterOrder[0]}
                data-fileterm-sort-kind="resource-monitoring-metric-top"
              />
            ) : null}
            {meterOrder.map((key) => {
              const item = RESOURCE_MONITORING_METER_OPTIONS.find((option) => option.key === key)
              if (!item) return null

              const isDragging = draggingMetric === key
              const isDragOver = dragOverMetric === key
              return (
                <div
                  className={`resource-monitoring-item-card ${isDragging ? 'dragging' : ''} ${managerDropClass(isDragOver, dragPosition)}`}
                  data-fileterm-sort-id={key}
                  data-fileterm-sort-kind="resource-monitoring-metric"
                  draggable={false}
                  key={key}
                  onClick={(event) => {
                    if (suppressCardClickRef.current) {
                      event.preventDefault()
                      event.stopPropagation()
                    }
                  }}
                  onPointerDown={(event) => {
                    if (!disabled && !targetsNestedManagerControl(event)) {
                      handlePointerDown(event, key)
                    }
                  }}
                >
                  <span
                    aria-label={t.resourceMonitoringDragToReorder}
                    className="resource-monitoring-item-drag-handle"
                    title={t.resourceMonitoringDragToReorder}
                  >
                    <AppIcon name="drag-handle" size={15} />
                  </span>
                  <span className="resource-monitoring-item-copy">
                    <strong>{t[item.labelKey]}</strong>
                  </span>
                  <label className="command-toggle resource-monitoring-item-toggle">
                    <input
                      checked={metrics.includes(key)}
                      disabled={disabled}
                      onChange={(event) => toggleMetric(key, event.target.checked)}
                      type="checkbox"
                    />
                  </label>
                </div>
              )
            })}
          </div>
        </>
      ) : null}
      {RESOURCE_MONITORING_PANEL_OPTIONS.map(({ key, labelKey }) => (
        <label className="resource-monitoring-panel-row" key={key}>
          <span className="resource-monitoring-panel-name">{t[labelKey]}</span>
          <span className="command-toggle resource-monitoring-panel-toggle">
            <input
              checked={metrics.includes(key)}
              disabled={disabled}
              onChange={(event) => toggleMetric(key, event.target.checked)}
              type="checkbox"
            />
          </span>
        </label>
      ))}
    </div>
  )
}
