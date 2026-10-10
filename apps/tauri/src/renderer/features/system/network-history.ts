import type { NetworkSamplePoint } from '@fileterm/core'

export function buildLinePath(samples: NetworkSamplePoint[], key: 'rx' | 'tx', maxValue: number) {
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
    if (samples[index + 1].breakBefore) {
      path += ` M ${next.x.toFixed(2)} ${next.y.toFixed(2)}`
      continue
    }
    const controlX = (next.x - current.x) / 2

    path += ` C ${(current.x + controlX).toFixed(2)} ${current.y.toFixed(2)}, ${(next.x - controlX).toFixed(2)} ${next.y.toFixed(2)}, ${next.x.toFixed(2)} ${next.y.toFixed(2)}`
  }

  return path
}

export function buildScrollingWindow(samples: NetworkSamplePoint[], visibleCount: number) {
  const windowSize = visibleCount + 1
  const padded = Array.from({ length: Math.max(0, windowSize - samples.length) }, () => ({ rx: 0, tx: 0 }))
  return [...padded, ...samples].slice(-windowSize)
}

export function areSampleWindowsEqual(left: NetworkSamplePoint[], right: NetworkSamplePoint[]) {
  if (left === right) {
    return true
  }
  if (left.length !== right.length) {
    return false
  }

  for (let index = 0; index < left.length; index += 1) {
    if (
      left[index]?.rx !== right[index]?.rx ||
      left[index]?.tx !== right[index]?.tx ||
      left[index]?.breakBefore !== right[index]?.breakBefore ||
      left[index]?.sampledAt !== right[index]?.sampledAt
    ) {
      return false
    }
  }

  return true
}
