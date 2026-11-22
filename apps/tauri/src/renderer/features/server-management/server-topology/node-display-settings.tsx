import { Button } from '../../../components/common/button/button'
import { t } from '../../../i18n'
import {
  TOPOLOGY_NODE_METRICS,
  TOPOLOGY_NODE_SIZES,
  topologyNodeSizeForMetricCount,
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
  showMetrics,
  onSizeChange,
  onMetricsChange
}: {
  size: TopologyNodeSize
  metrics: TopologyNodeMetric[]
  showMetrics: boolean
  onSizeChange(size: TopologyNodeSize): void
  onMetricsChange(metrics: TopologyNodeMetric[]): void
}) {
  const toggleMetric = (metric: TopologyNodeMetric) => {
    onMetricsChange(metrics.includes(metric) ? metrics.filter((item) => item !== metric) : [...metrics, metric])
  }

  return (
    <section className="server-topology__display-settings">
      <h3>{t.serverTopologyDisplaySettings}</h3>
      <div className="server-topology__setting-group" role="group" aria-label={t.serverTopologyNodeSize}>
        <span>{t.serverTopologyNodeSize}</span>
        <div className="server-topology__setting-options">
          {TOPOLOGY_NODE_SIZES.map((option) => (
            <Button
              aria-pressed={size === option}
              disabled={topologyNodeSizeForMetricCount(option, metrics.length) !== option}
              key={option}
              onClick={() => onSizeChange(option)}
              size="sm"
              variant="ghost"
            >
              {sizeLabels[option]}
            </Button>
          ))}
        </div>
      </div>
      {showMetrics ? (
        <>
          <div className="server-topology__setting-group" role="group" aria-label={t.serverTopologyVisibleMetrics}>
            <span>{t.serverTopologyVisibleMetrics}</span>
            <div className="server-topology__setting-options">
              {TOPOLOGY_NODE_METRICS.map((metric) => (
                <Button
                  aria-pressed={metrics.includes(metric)}
                  key={metric}
                  onClick={() => toggleMetric(metric)}
                  size="sm"
                  variant="ghost"
                >
                  {metricLabels[metric]}
                </Button>
              ))}
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
