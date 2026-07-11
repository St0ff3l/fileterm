import type { RawNetworkInterfaceMetrics, RemoteSystemPlatform, SystemMetrics } from '@fileterm/core'

export function parseSystemMetrics(raw: string, fallbackPlatform: RemoteSystemPlatform = 'unknown'): SystemMetrics {
  const normalizedRaw = raw.replace(/\r\n?/g, '\n')
  const readLine = (key: string) =>
    normalizedRaw
      .split('\n')
      .find((line) => line.startsWith(key))
      ?.slice(key.length)
      .trim() ?? ''
  const readBlock = (start: string, end: string) => {
    const startIndex = normalizedRaw.indexOf(start)
    const endIndex = normalizedRaw.indexOf(end, startIndex + start.length)
    if (startIndex === -1 || endIndex === -1 || endIndex <= startIndex) {
      return []
    }
    return normalizedRaw
      .slice(startIndex + start.length, endIndex)
      .trim()
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
  }

  const platform = normalizePlatform(readLine('__PLATFORM__'), fallbackPlatform)
  const loadUnit = readLine('__LOAD_UNIT__') === 'busy-logical-processors' ? 'busy-logical-processors' : undefined
  const [memUsed, memTotal, memPercent, memApp, memCache, memKernel] = readLine('__MEM__').split('|')
  const [memUsedBytes, memTotalBytes, memAvailableBytes, memRawPercent, memAppBytes, memCacheBytes, memKernelBytes] =
    readLine('__MEM_BYTES__').split('|')
  const [swapUsed, swapTotal, swapPercent] = readLine('__SWAP__').split('|')
  const [swapUsedBytes, swapTotalBytes, swapAvailableBytes, swapRawPercent] = readLine('__SWAP_BYTES__').split('|')
  const [cpuUser, cpuSystem, cpuNice, cpuIdle, cpuIoWait, cpuIrq, cpuSoftIrq, cpuSteal] =
    readLine('__CPU_USAGE__').split('|')
  const [rxRate, txRate] = readLine('__RATES__').split('|')
  const interfaces = readLine('__IFACES__').split(',').filter(Boolean)
  const networkRawByInterface: Record<string, RawNetworkInterfaceMetrics> = {}
  const networkInterfaceRows = readBlock('__IFACE_RATES_START__', '__IFACE_RATES_END__')
    .map((line) => {
      const [name, rxTotal, txTotal, rx, tx] = line.split('|')
      const rxBytes = Number(rxTotal) || 0
      const txBytes = Number(txTotal) || 0
      const rxBytesPerSecond = Number(rx) || 0
      const txBytesPerSecond = Number(tx) || 0
      if (name) {
        networkRawByInterface[name] = {
          name,
          rxBytes,
          txBytes,
          rxBytesPerSecond,
          txBytesPerSecond
        }
      }
      return {
        name,
        txTotal: formatNetworkBytes(txBytes),
        rxTotal: formatNetworkBytes(rxBytes),
        txRate: formatRate(txBytesPerSecond),
        rxRate: formatRate(rxBytesPerSecond)
      }
    })
    .filter((row) => row.name)
  const networkRatesByInterface = networkInterfaceRows.reduce<Record<string, { rx: string; tx: string }>>(
    (acc, row) => {
      const { name, rxRate: rowRxRate, txRate: rowTxRate } = row
      if (!name) {
        return acc
      }
      acc[name] = {
        rx: rowRxRate,
        tx: rowTxRate
      }
      return acc
    },
    {}
  )
  const networkSamplesByInterface = Object.fromEntries(
    Object.entries(networkRawByInterface).map(([name, row]) => [
      name,
      [
        {
          rx: row.rxBytesPerSecond,
          tx: row.txBytesPerSecond
        }
      ]
    ])
  )
  const diskRows = readBlock('__DISK_START__', '__DISK_END__').map((line) => {
    const [diskPath, usage] = line.split('|')
    return { path: diskPath, usage: formatStorageUsage(usage) }
  })
  const fileSystemRows = readBlock('__FILESYSTEMS_START__', '__FILESYSTEMS_END__').map((line) => {
    const [name, size, used, usagePercent, available, mountPoint] = line.split('|')
    return {
      name,
      size: formatStorageValue(size),
      used: formatStorageValue(used),
      usagePercent,
      available: formatStorageValue(available),
      mountPoint
    }
  })
  const cpuInfoRows = readBlock('__CPUINFO_START__', '__CPUINFO_END__')
    .map((line) => {
      const [model, cores, frequencyMHz, cache, bogomips] = line.split('|')
      return {
        model,
        cores: Number(cores) || 0,
        frequencyMHz,
        cache,
        bogomips
      }
    })
    .filter((row) => row.model)
  const gpuInfoRows = readBlock('__GPUINFO_START__', '__GPUINFO_END__')
    .map((line) => {
      const [model, vendor, driver, memory] = line.split('|')
      return {
        model,
        vendor: vendor || '-',
        driver: driver || '-',
        memory: memory || '-'
      }
    })
    .filter((row) => row.model)
  const transientCollectorCommands = new Set(['ps', 'awk', 'bash', 'sleep', 'sh', 'powershell', 'pwsh'])
  const groupedProcesses = new Map<
    string,
    {
      memoryMb: number
      cpu: number
      elapsedSeconds: number
    }
  >()
  readBlock('__PROCS_START__', '__PROCS_END__')
    .map((line) => {
      const [memory, cpu, elapsedSeconds, command] = line.split('|')
      return {
        memoryMb: Number(memory.replace(/M$/i, '')) || 0,
        cpuValue: Number(cpu) || 0,
        command,
        elapsedSeconds: Number(elapsedSeconds) || 0
      }
    })
    .filter((process) => process.command && !transientCollectorCommands.has(process.command))
    .forEach((process) => {
      const current = groupedProcesses.get(process.command) ?? {
        memoryMb: 0,
        cpu: 0,
        elapsedSeconds: 0
      }
      groupedProcesses.set(process.command, {
        memoryMb: current.memoryMb + process.memoryMb,
        cpu: current.cpu + process.cpuValue,
        elapsedSeconds: Math.max(current.elapsedSeconds, process.elapsedSeconds)
      })
    })
  const topProcesses = [...groupedProcesses.entries()]
    .map(([command, process]) => ({
      memory: formatProcessMegabytes(process.memoryMb),
      cpu: process.cpu.toFixed(1),
      command,
      elapsedSeconds: process.elapsedSeconds
    }))
    .sort((left, right) => parseFloat(right.memory) - parseFloat(left.memory))

  const memoryUsedBytes = readNumber(memUsedBytes, megabytesToBytes(memUsed))
  const memoryTotalBytes = readNumber(memTotalBytes, megabytesToBytes(memTotal))
  const memoryAvailableBytes = readNumber(memAvailableBytes, Math.max(memoryTotalBytes - memoryUsedBytes, 0))
  const memoryPercent = readNumber(memRawPercent, Number(memPercent) || 0)
  const swapUsedRawBytes = readNumber(swapUsedBytes, megabytesToBytes(swapUsed))
  const swapTotalRawBytes = readNumber(swapTotalBytes, megabytesToBytes(swapTotal))
  const swapAvailableRawBytes = readNumber(swapAvailableBytes, Math.max(swapTotalRawBytes - swapUsedRawBytes, 0))
  const swapRawUsagePercent = readNumber(swapRawPercent, Number(swapPercent) || 0)
  const aggregateNetworkRaw: RawNetworkInterfaceMetrics = {
    name: 'all',
    rxBytes: Object.values(networkRawByInterface).reduce((sum, row) => sum + row.rxBytes, 0),
    txBytes: Object.values(networkRawByInterface).reduce((sum, row) => sum + row.txBytes, 0),
    rxBytesPerSecond: Number(rxRate) || 0,
    txBytesPerSecond: Number(txRate) || 0
  }

  return {
    platform,
    ip: readLine('__IP__'),
    uptime: readLine('__UPTIME__') || '-',
    uptimeSeconds: Number(readLine('__UPTIME_SECONDS__')) || undefined,
    load: readLine('__LOAD__') || '-',
    loadUnit,
    identity: {
      osName: readLine('__OS__') || '-',
      kernelName: readLine('__KERNEL_NAME__') || '-',
      kernelVersion: readLine('__KERNEL_VERSION__') || '-',
      architecture: readLine('__ARCH__') || '-',
      hostname: readLine('__HOSTNAME__') || '-'
    },
    cpuPercent: Number(readLine('__CPU__')) || 0,
    cpuUsage: {
      user: Number(cpuUser) || 0,
      system: Number(cpuSystem) || 0,
      nice: Number(cpuNice) || 0,
      idle: Number(cpuIdle) || 0,
      ioWait: Number(cpuIoWait) || 0,
      irq: Number(cpuIrq) || 0,
      softIrq: Number(cpuSoftIrq) || 0,
      steal: Number(cpuSteal) || 0
    },
    cpuInfoRows,
    gpuInfoRows,
    memoryPercent,
    memoryUsage: memoryTotalBytes
      ? `${formatBytesAsMegabytes(memoryUsedBytes)}/${formatBytesAsMegabytes(memoryTotalBytes)}`
      : '0/0',
    memoryAppUsage:
      Number(memApp) > 0 || Number(memAppBytes) > 0
        ? formatBytesAsMegabytes(readNumber(memAppBytes, megabytesToBytes(memApp)))
        : undefined,
    memoryCacheUsage:
      Number(memCache) > 0 || Number(memCacheBytes) > 0
        ? formatBytesAsMegabytes(readNumber(memCacheBytes, megabytesToBytes(memCache)))
        : undefined,
    memoryKernelUsage:
      Number(memKernel) > 0 || Number(memKernelBytes) > 0
        ? formatBytesAsMegabytes(readNumber(memKernelBytes, megabytesToBytes(memKernel)))
        : undefined,
    memoryBreakdown: {
      total: formatBytesAsMegabytes(memoryTotalBytes),
      used: formatBytesAsMegabytes(memoryUsedBytes),
      available: formatBytesAsMegabytes(memoryAvailableBytes),
      percent: memoryPercent
    },
    memoryRaw: {
      totalBytes: memoryTotalBytes,
      usedBytes: memoryUsedBytes,
      availableBytes: memoryAvailableBytes,
      percent: memoryPercent,
      appBytes: readNumber(memAppBytes, megabytesToBytes(memApp)),
      cacheBytes: readNumber(memCacheBytes, megabytesToBytes(memCache)),
      kernelBytes: readNumber(memKernelBytes, megabytesToBytes(memKernel))
    },
    swapPercent: swapRawUsagePercent,
    swapUsage: swapTotalRawBytes
      ? `${formatBytesAsMegabytes(swapUsedRawBytes)}/${formatBytesAsMegabytes(swapTotalRawBytes)}`
      : '0/0',
    swapBreakdown: {
      total: formatBytesAsMegabytes(swapTotalRawBytes),
      used: formatBytesAsMegabytes(swapUsedRawBytes),
      available: formatBytesAsMegabytes(swapAvailableRawBytes),
      percent: swapRawUsagePercent
    },
    swapRaw: {
      totalBytes: swapTotalRawBytes,
      usedBytes: swapUsedRawBytes,
      availableBytes: swapAvailableRawBytes,
      percent: swapRawUsagePercent
    },
    diskRows,
    fileSystemRows,
    networkInterfaces: ['all', ...interfaces],
    activeNetworkInterface: 'all',
    networkRates: {
      rx: formatRate(Number(rxRate) || 0),
      tx: formatRate(Number(txRate) || 0)
    },
    networkSamples: [
      {
        rx: Number(rxRate) || 0,
        tx: Number(txRate) || 0
      }
    ],
    networkInterfaceRows,
    networkRatesByInterface: {
      all: {
        rx: formatRate(Number(rxRate) || 0),
        tx: formatRate(Number(txRate) || 0)
      },
      ...networkRatesByInterface
    },
    networkSamplesByInterface: {
      all: [
        {
          rx: Number(rxRate) || 0,
          tx: Number(txRate) || 0
        }
      ],
      ...networkSamplesByInterface
    },
    networkRawByInterface: {
      all: aggregateNetworkRaw,
      ...networkRawByInterface
    },
    topProcesses
  }
}

function normalizePlatform(value: string, fallback: RemoteSystemPlatform): RemoteSystemPlatform {
  const normalized = value.trim().toLowerCase()
  if (normalized === 'linux' || normalized === 'busybox' || normalized === 'windows' || normalized === 'unknown') {
    return normalized
  }
  return fallback
}

function readNumber(value: string | undefined, fallback = 0) {
  const numeric = Number(value)
  return Number.isFinite(numeric) ? numeric : fallback
}

function megabytesToBytes(value?: string | number) {
  return (Number(value) || 0) * 1024 * 1024
}

function formatBytesAsMegabytes(value?: string | number) {
  const numeric = Number(value) || 0
  const megabytes = numeric / 1024 / 1024
  if (megabytes >= 1024) {
    return `${(megabytes / 1024).toFixed(1)}G`
  }
  return `${Math.round(megabytes)}M`
}

function formatRate(bytesPerSecond: number) {
  if (bytesPerSecond >= 1024 * 1024) {
    return `${Math.round(bytesPerSecond / 1024 / 1024)}M`
  }
  if (bytesPerSecond >= 1024) {
    return `${Math.round(bytesPerSecond / 1024)}K`
  }
  return `${bytesPerSecond}B`
}

function formatNetworkBytes(bytes: number) {
  if (bytes >= 1024 * 1024 * 1024 * 1024) {
    return `${(bytes / 1024 / 1024 / 1024 / 1024).toFixed(1)} TB`
  }
  if (bytes >= 1024 * 1024 * 1024) {
    return `${(bytes / 1024 / 1024 / 1024).toFixed(bytes >= 10 * 1024 * 1024 * 1024 ? 0 : 1)} GB`
  }
  if (bytes >= 1024 * 1024) {
    return `${(bytes / 1024 / 1024).toFixed(bytes >= 10 * 1024 * 1024 ? 0 : 1)} MB`
  }
  if (bytes >= 1024) {
    return `${Math.round(bytes / 1024)} KB`
  }
  return `${bytes} B`
}

function formatStorageUsage(value?: string) {
  if (!value) {
    return '-'
  }
  const separatorIndex = value.indexOf('/')
  if (separatorIndex === -1) {
    return formatStorageValue(value)
  }
  return `${formatStorageValue(value.slice(0, separatorIndex))}/${formatStorageValue(value.slice(separatorIndex + 1))}`
}

function formatStorageValue(value?: string) {
  const trimmed = value?.trim() || '-'
  // Windows already emits localized, human-readable values such as "53.6 GB".
  // POSIX df -kP emits compact KiB values such as "8153252K" which need normalization.
  if (trimmed === '-' || /\s/.test(trimmed)) {
    return trimmed
  }
  const match = /^(\d+(?:\.\d+)?)([KMGT])(?:I?B)?$/i.exec(trimmed)
  if (!match) {
    return trimmed
  }
  const unitPowers = { K: 1, M: 2, G: 3, T: 4 } as const
  const unit = match[2].toUpperCase() as keyof typeof unitPowers
  const bytes = Number(match[1]) * 1024 ** unitPowers[unit]
  if (!Number.isFinite(bytes)) {
    return trimmed
  }
  const displayUnits = ['B', 'KB', 'MB', 'GB', 'TB'] as const
  let displayValue = bytes
  let displayUnitIndex = 0
  while (displayValue >= 1024 && displayUnitIndex < displayUnits.length - 1) {
    displayValue /= 1024
    displayUnitIndex += 1
  }
  return `${displayValue.toFixed(displayUnitIndex === 0 ? 0 : 1)} ${displayUnits[displayUnitIndex]}`
}

function formatProcessMegabytes(value: number) {
  if (value >= 1024) {
    return `${(value / 1024).toFixed(value >= 10 * 1024 ? 0 : 1)}G`
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)}M`
}
