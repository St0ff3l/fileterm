import { useRef, useState, type KeyboardEvent } from 'react'
import { SegmentedControl } from '../../../components/common/segmented-control/segmented-control'
import { SelectionControl } from '../../../features/common/selection-control'
import { usePointerSortFallback, type PointerSortTarget } from '../../../hooks/use-pointer-sort-fallback'
import { t } from '../../../i18n'
import {
  TOPOLOGY_NODE_SIZES,
  topologyNodeSizeForContent,
  type TopologyNodeMetric,
  type TopologyNodeSize
} from '../server-topology-model'

const metricLabels: Record<TopologyNodeMetric, string> = {
  ip: t.privateIp,
  cpu: t.cpu,
  memory: t.memory,
  disk: t.disk,
  gpu: t.gpu,
  network: t.resourceMonitoringNetwork,
  processes: t.resourceMonitoringProcesses
}

const sizeLabels: Record<TopologyNodeSize, string> = {
  compact: t.serverTopologySizeCompact,
  standard: t.serverTopologySizeStandard,
  expanded: t.serverTopologySizeExpanded
}

export function NodeDisplaySettings({
  size,
  metrics,
  metricOrder,
  serviceCount,
  showMetrics,
  onSizeChange,
  onMetricsChange,
  onMetricOrderChange
}: {
  size: TopologyNodeSize
  metrics: TopologyNodeMetric[]
  metricOrder: TopologyNodeMetric[]
  serviceCount: number
  showMetrics: boolean
  onSizeChange(size: TopologyNodeSize): void
  onMetricsChange(metrics: TopologyNodeMetric[]): void
  onMetricOrderChange(metricOrder: TopologyNodeMetric[]): void
}) {
  const [draggingMetric, setDraggingMetric] = useState<TopologyNodeMetric | null>(null)
  const [dragOverMetric, setDragOverMetric] = useState<TopologyNodeMetric | null>(null)
  const [metricDropPosition, setMetricDropPosition] = useState<'before' | 'after' | null>(null)
  const suppressMetricClickRef = useRef(false)

  const clearMetricDragState = () => {
    setDraggingMetric(null)
    setDragOverMetric(null)
    setMetricDropPosition(null)
    window.setTimeout(() => {
      suppressMetricClickRef.current = false
    }, 0)
  }

  const positionForMetricTarget = (target: PointerSortTarget, clientX: number) => {
    const { left, width } = target.element.getBoundingClientRect()
    return clientX < left + width / 2 ? 'before' : 'after'
  }

  const handleMetricPointerDown = usePointerSortFallback<TopologyNodeMetric>({
    onStart: (metric) => {
      suppressMetricClickRef.current = true
      setDraggingMetric(metric)
    },
    onTarget: (source, target, _clientY, clientX) => {
      if (
        target.kind !== 'topology-node-metric' ||
        !metricOrder.some((metric) => metric === target.id) ||
        source === target.id
      ) {
        setDragOverMetric(null)
        setMetricDropPosition(null)
        return
      }
      setDragOverMetric(target.id as TopologyNodeMetric)
      setMetricDropPosition(positionForMetricTarget(target, clientX))
    },
    onDrop: (source, target, _clientY, clientX) => {
      if (
        target?.kind === 'topology-node-metric' &&
        metricOrder.some((metric) => metric === target.id) &&
        source !== target.id
      ) {
        const next = metricOrder.filter((metric) => metric !== source)
        const targetMetric = target.id as TopologyNodeMetric
        const targetIndex = next.indexOf(targetMetric)
        if (targetIndex !== -1) {
          next.splice(targetIndex + (positionForMetricTarget(target, clientX) === 'after' ? 1 : 0), 0, source)
          onMetricOrderChange(next)
        }
      }
      clearMetricDragState()
    },
    onCancel: clearMetricDragState
  })

  const handleMetricReorderKeyDown = (event: KeyboardEvent<HTMLInputElement>, metric: TopologyNodeMetric) => {
    const index = metricOrder.indexOf(metric)
    const targetIndex = event.key === 'ArrowLeft' ? index - 1 : event.key === 'ArrowRight' ? index + 1 : -1
    if (targetIndex < 0 || targetIndex >= metricOrder.length) return
    event.preventDefault()
    const next = [...metricOrder]
    ;[next[index], next[targetIndex]] = [next[targetIndex], next[index]]
    onMetricOrderChange(next)
  }

  const toggleMetric = (metric: TopologyNodeMetric) => {
    onMetricsChange(metrics.includes(metric) ? metrics.filter((item) => item !== metric) : [...metrics, metric])
  }
  return (
    <section className="server-topology__display-settings">
      <h3>{t.serverTopologyDisplaySettings}</h3>
      <div className="server-topology__setting-group">
        <span>{t.serverTopologyNodeSize}</span>
        <SegmentedControl<TopologyNodeSize>
          ariaLabel={t.serverTopologyNodeSize}
          className="server-topology__size-options"
          variant="neutral"
          value={size}
          options={TOPOLOGY_NODE_SIZES.map((option) => ({
            value: option,
            label: sizeLabels[option],
            disabled: topologyNodeSizeForContent(option, metrics.length, serviceCount) !== option
          }))}
          onChange={onSizeChange}
        />
      </div>
      {showMetrics ? (
        <>
          <div className="server-topology__setting-group">
            <span>{t.serverTopologyVisibleMetrics}</span>
            <div className="server-topology__metric-options" role="group" aria-label={t.serverTopologyVisibleMetrics}>
              {metricOrder.map((metric) => {
                const selected = metrics.includes(metric)
                const isDragging = draggingMetric === metric
                const isDragOver = dragOverMetric === metric
                return (
                  <label
                    className={`server-topology__metric-option${selected ? ' is-selected' : ''}${isDragging ? ' is-dragging' : ''}${isDragOver && metricDropPosition ? ` drop-${metricDropPosition}` : ''}`}
                    data-fileterm-sort-id={metric}
                    data-fileterm-sort-kind="topology-node-metric"
                    draggable={false}
                    key={metric}
                    onClickCapture={(event) => {
                      if (suppressMetricClickRef.current) {
                        event.preventDefault()
                        event.stopPropagation()
                      }
                    }}
                    onPointerDown={(event) => handleMetricPointerDown(event, metric)}
                    title={t.resourceMonitoringDragToReorder}
                  >
                    <SelectionControl
                      checked={selected}
                      onKeyDown={(event) => handleMetricReorderKeyDown(event, metric)}
                      onChange={() => toggleMetric(metric)}
                      type="checkbox"
                      value={metric}
                    />
                    <span>{metricLabels[metric]}</span>
                  </label>
                )
              })}
            </div>
          </div>
          <p className="server-topology__hint">{t.serverTopologyMetricHint}</p>
        </>
      ) : (
        <p className="server-topology__hint">{t.serverTopologyMetricsOnlyForServers}</p>
      )}
    </section>
  )
}
