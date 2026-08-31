import { useRef } from 'react'
import type { SystemMetrics } from '@fileterm/core'
import { t } from '../../i18n'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { MetricHoverDetail } from './system-resource-meters'

export function ProcessMetricPanel({
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
