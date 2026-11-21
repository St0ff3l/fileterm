import './system-network-panel.css'
import { buildLinePath, buildScrollingWindow } from './network-history'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { SystemMetrics } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { DropdownSelect } from '../common/dropdown-select'

const EMPTY_SAMPLES: SystemMetrics['networkSamples'] = []

export function NetworkMetricPanel({ metrics, onOpen }: { metrics?: SystemMetrics; onOpen?: () => void }) {
  return (
    <div className="system-network-panel">
      <NetworkPanel metrics={metrics} onOpen={onOpen} />
    </div>
  )
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

function NetworkPanel({ metrics, onOpen }: { metrics?: SystemMetrics; onOpen?: () => void }) {
  const visibleSampleCount = 64
  const chartStep = 100 / Math.max(1, visibleSampleCount - 1)
  const [selectedInterface, setSelectedInterface] = useState(metrics?.activeNetworkInterface ?? '')
  const interfaceOptions = metrics?.networkInterfaces.length ? metrics.networkInterfaces : ['-']
  const currentRates = metrics?.networkRatesByInterface?.[selectedInterface] ?? metrics?.networkRates
  const rawSamples = metrics?.networkSamplesByInterface?.[selectedInterface]?.length
    ? metrics.networkSamplesByInterface[selectedInterface]
    : metrics?.networkSamples.length
      ? metrics.networkSamples
      : EMPTY_SAMPLES
  const samples = useMemo(() => buildScrollingWindow(rawSamples, visibleSampleCount), [rawSamples])
  const chartRef = useRef<SVGGElement | null>(null)
  const animationFrameRef = useRef<number | null>(null)
  const previousInterfaceRef = useRef(selectedInterface)
  const previousLastSampleRef = useRef(rawSamples.at(-1))
  const previousSampleCountRef = useRef(rawSamples.length)

  const { maxValue, txPath, rxPath } = useMemo(() => {
    const maxValue = Math.max(...samples.map((sample) => Math.max(sample.rx, sample.tx)), 1)
    return { maxValue, txPath: buildLinePath(samples, 'tx', maxValue), rxPath: buildLinePath(samples, 'rx', maxValue) }
  }, [samples])
  const chartScale = [maxValue, maxValue * 0.66, maxValue * 0.33]

  useEffect(() => {
    if (!interfaceOptions.includes(selectedInterface)) {
      const nextInterface = metrics?.activeNetworkInterface ?? interfaceOptions[0] ?? ''
      if (nextInterface !== selectedInterface) setSelectedInterface(nextInterface)
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
      previousLastSample?.tx !== latestSample?.tx ||
      previousLastSample?.sampledAt !== latestSample?.sampledAt

    previousLastSampleRef.current = latestSample
    previousSampleCountRef.current = rawSamples.length

    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current)
      animationFrameRef.current = null
    }

    // Animation must not enqueue React updates behind every monitoring/input commit.
    const setChartOffset = (offset: number) => chartRef.current?.setAttribute('transform', `translate(${offset} 0)`)
    if (interfaceChanged || !sampleAdvanced) {
      setChartOffset(-chartStep)
      return
    }

    const startTime = performance.now()
    const duration = 420

    setChartOffset(0)

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
        <button
          type="button"
          className="network-rates network-details-trigger"
          onClick={onOpen}
          disabled={!onOpen}
          title={t.networkConnections}
          aria-label={t.networkConnections}
        >
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
        </button>
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
      <button
        type="button"
        className="network-history network-details-trigger"
        onClick={onOpen}
        disabled={!onOpen}
        title={t.networkConnections}
        aria-label={t.networkConnections}
      >
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
            <g ref={chartRef} transform={`translate(${-chartStep} 0)`}>
              <path className="network-path tx-path" d={txPath} />
              <path className="network-path rx-path" d={rxPath} />
            </g>
          </svg>
        </div>
      </button>
    </>
  )
}
