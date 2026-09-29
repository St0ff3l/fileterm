import { buildLinePath, buildScrollingWindow, areSampleWindowsEqual } from './network-history'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { SystemMetrics } from '@fileterm/core'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { DropdownSelect } from '../common/dropdown-select'

export function NetworkMetricPanel({ metrics }: { metrics?: SystemMetrics }) {
  return (
    <div className="system-network-panel">
      <NetworkPanel metrics={metrics} />
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
      previousLastSample?.tx !== latestSample?.tx ||
      previousLastSample?.sampledAt !== latestSample?.sampledAt

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
