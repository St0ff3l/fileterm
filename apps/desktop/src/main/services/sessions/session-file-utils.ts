import path from 'node:path'
import { FileInfo, FileType } from 'basic-ftp'
import type { FileEntry } from 'ssh2'
import type { RemoteFileItem, SystemMetrics } from '@termdock/core'

export function toRemoteFileItem(basePath: string, entry: FileEntry): RemoteFileItem {
  const fullPath = path.posix.join(basePath, entry.filename)
  const isDirectory = entry.longname.startsWith('d')
  return {
    path: fullPath,
    name: entry.filename,
    type: isDirectory ? 'folder' : 'file',
    modified: formatTimestamp(entry.attrs.mtime),
    size: isDirectory ? '-' : formatBytes(entry.attrs.size),
    permission: entry.longname.split(/\s+/)[0] ?? '',
    ownerGroup: `${entry.attrs.uid ?? 0}/${entry.attrs.gid ?? 0}`
  }
}

export function toFtpRemoteFileItem(basePath: string, entry: FileInfo): RemoteFileItem {
  const fullPath = path.posix.join(basePath, entry.name)
  const isDirectory = entry.type === FileType.Directory || entry.isDirectory
  return {
    path: fullPath,
    name: entry.name,
    type: isDirectory ? 'folder' : 'file',
    modified: entry.modifiedAt ? formatDate(entry.modifiedAt) : entry.rawModifiedAt,
    size: isDirectory ? '-' : formatBytes(entry.size),
    permission: formatFtpPermissions(entry.type, entry.permissions),
    ownerGroup: [entry.user, entry.group].filter(Boolean).join('/') || ''
  }
}

export function formatFtpPermissions(type: FileType, permissions?: FileInfo['permissions']) {
  if (!permissions) {
    return type === FileType.Directory ? 'd---------' : '----------'
  }

  return `${type === FileType.Directory ? 'd' : '-'}${formatPermissionGroup(permissions.user)}${formatPermissionGroup(permissions.group)}${formatPermissionGroup(permissions.world)}`
}

export function formatPermissionGroup(value = 0) {
  return `${value & FileInfo.UnixPermission.Read ? 'r' : '-'}${value & FileInfo.UnixPermission.Write ? 'w' : '-'}${value & FileInfo.UnixPermission.Execute ? 'x' : '-'}`
}

export function parentRemotePath(currentPath: string) {
  const normalized = currentPath.endsWith('/') && currentPath !== '/' ? currentPath.slice(0, -1) : currentPath
  const parent = path.posix.dirname(normalized)
  return parent === '.' ? '/' : parent
}

export function formatTimestamp(timestamp?: number) {
  if (!timestamp) {
    return ''
  }
  const date = new Date(timestamp * 1000)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

export function formatDate(date: Date) {
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

export function formatBytes(size = 0) {
  if (!size) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = size
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  const digits = value >= 10 || unitIndex === 0 ? 0 : 1
  return `${value.toFixed(digits)} ${units[unitIndex]}`
}

export function buildMetricsCommand() {
  return `sh <<'__TERMDOCK_METRICS__'
cd / >/dev/null 2>&1 || true
sleep_interval="0.15"
sleep "$sleep_interval" >/dev/null 2>&1 || sleep_interval="1"
read_cpu_stat() {
  awk '/^cpu / {print $2, $3, $4, $5, $6, $7, $8, $9; exit}' /proc/stat 2>/dev/null
}
set -- $(read_cpu_stat)
user=\${1:-0}
nice=\${2:-0}
system=\${3:-0}
idle=\${4:-0}
iowait=\${5:-0}
irq=\${6:-0}
softirq=\${7:-0}
steal=\${8:-0}
total1=$((user+nice+system+idle+iowait+irq+softirq+steal))
idle1=$((idle+iowait))
sleep "$sleep_interval"
set -- $(read_cpu_stat)
user2=\${1:-0}
nice2=\${2:-0}
system2=\${3:-0}
idle2=\${4:-0}
iowait2=\${5:-0}
irq2=\${6:-0}
softirq2=\${7:-0}
steal2=\${8:-0}
total2=$((user2+nice2+system2+idle2+iowait2+irq2+softirq2+steal2))
idle2sum=$((idle2+iowait2))
diff_total=$((total2-total1))
diff_idle=$((idle2sum-idle1))
if [ "$diff_total" -gt 0 ]; then cpu_pct=$((100*(diff_total-diff_idle)/diff_total)); else cpu_pct=0; fi
ip=$(hostname -I 2>/dev/null | awk '{print $1}')
[ -z "$ip" ] && ip=$(ip route get 1 2>/dev/null | awk 'NR==1 {for (i=1; i<=NF; i++) if ($i == "src") {print $(i+1); exit}}')
if [ -z "$ip" ]; then
  ip=$(ifconfig 2>/dev/null | awk '/inet / && $2 !~ /^127\\./ {print $2; exit}')
fi
if [ -z "$ip" ]; then
  ip=$(ifconfig 2>/dev/null | awk '/inet addr:/ && $2 !~ /127\\.0\\.0\\.1/ {sub("addr:", "", $2); print $2; exit}')
fi
uptime_days=$(awk '{print int($1/86400) " 天"}' /proc/uptime 2>/dev/null)
if [ -z "$uptime_days" ]; then
  uptime_days=$(uptime 2>/dev/null | awk -F'(up |, *[0-9]+ user)' 'NF>1 {gsub(/^ +| +$/, "", $2); print $2; exit}')
fi
load=$(awk '{printf "%s, %s, %s", $1, $2, $3}' /proc/loadavg 2>/dev/null)
if [ -z "$load" ]; then
  load=$(uptime 2>/dev/null | sed -n 's/.*load averages\\{0,1\\}: *//p; s/.*load average: *//p' | awk -F',' 'NF>=3 {gsub(/^ +| +$/, "", $1); gsub(/^ +| +$/, "", $2); gsub(/^ +| +$/, "", $3); printf "%s, %s, %s", $1, $2, $3; exit}')
fi
mem=$(awk 'BEGIN { total=available=memfree=buffers=cached=shmem=sreclaimable=slab=kernelstack=pagetables=active_anon=inactive_anon=unevictable=0 }
  /^MemTotal:/ { total=int($2/1024) }
  /^MemAvailable:/ { available=int($2/1024) }
  /^MemFree:/ { memfree=int($2/1024) }
  /^Buffers:/ { buffers=int($2/1024) }
  /^Cached:/ { cached=int($2/1024) }
  /^Shmem:/ { shmem=int($2/1024) }
  /^SReclaimable:/ { sreclaimable=int($2/1024) }
  /^Slab:/ { slab=int($2/1024) }
  /^KernelStack:/ { kernelstack=int($2/1024) }
  /^PageTables:/ { pagetables=int($2/1024) }
  /^Active\\(anon\\):/ { active_anon=int($2/1024) }
  /^Inactive\\(anon\\):/ { inactive_anon=int($2/1024) }
  /^Unevictable:/ { unevictable=int($2/1024) }
  END {
    if (available == 0) available=memfree+buffers+cached+sreclaimable-shmem
    if (available < 0) available=0
    if (total > 0) {
      used=total-available
      if (used < 0) used=0
      percent=int(used*100/total)
      app=active_anon+inactive_anon+unevictable
      cache=buffers+cached+sreclaimable-shmem
      if (cache < 0) cache=0
      kernel=slab-sreclaimable+kernelstack+pagetables
      if (kernel < 0) kernel=0
      printf "%d|%d|%d|%d|%d|%d", used, total, percent, app, cache, kernel
    }
  }' /proc/meminfo 2>/dev/null)
if [ -z "$mem" ]; then
  mem=$(free -m 2>/dev/null | awk '/^Mem:/ {
    total=$2
    used=$3
    available=$7
    if (available == "") available=total-used
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%d|%d|%d|0|0|0", used, total, percent
    exit
  }')
fi
swap=$(awk 'BEGIN { total=free=0 }
  /^SwapTotal:/ { total=int($2/1024) }
  /^SwapFree:/ { free=int($2/1024) }
  END {
    if (total >= 0) {
      used=total-free
      if (used < 0) used=0
      percent=(total>0 ? int(used*100/total) : 0)
      printf "%d|%d|%d", used, total, percent
    }
  }' /proc/meminfo 2>/dev/null)
if [ -z "$swap" ]; then
  swap=$(free -m 2>/dev/null | awk '/^Swap:/ {
    total=$2
    used=$3
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%d|%d|%d", used, total, percent
    exit
  }')
fi
ifaces=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); if (name != "lo") print name}' /proc/net/dev 2>/dev/null | paste -sd, -)
active_iface=$(awk '$2 == 00000000 {print $1; exit}' /proc/net/route 2>/dev/null)
[ -z "$active_iface" ] && active_iface=$(echo "$ifaces" | awk -F, '{print $1}')
rx1=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[2]} END {printf "%.0f", sum+0}' /proc/net/dev 2>/dev/null)
tx1=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[10]} END {printf "%.0f", sum+0}' /proc/net/dev 2>/dev/null)
before_file="/tmp/termdock-if-before-$$"
after_file="/tmp/termdock-if-after-$$"
awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") printf "%s|%.0f|%.0f\\n", name, values[2], values[10]}' /proc/net/dev 2>/dev/null > "$before_file"
sleep "$sleep_interval"
rx2=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[2]} END {printf "%.0f", sum+0}' /proc/net/dev 2>/dev/null)
tx2=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[10]} END {printf "%.0f", sum+0}' /proc/net/dev 2>/dev/null)
awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") printf "%s|%.0f|%.0f\\n", name, values[2], values[10]}' /proc/net/dev 2>/dev/null > "$after_file"
sample_ms=$(awk -v interval="$sleep_interval" 'BEGIN { printf "%d", interval * 1000 }')
[ -z "$sample_ms" ] && sample_ms=1000
rx_rate=$(awk -v before="$rx1" -v after="$rx2" -v ms="$sample_ms" 'BEGIN { if (ms > 0) printf "%d", (after-before) * 1000 / ms; else print 0 }')
tx_rate=$(awk -v before="$tx1" -v after="$tx2" -v ms="$sample_ms" 'BEGIN { if (ms > 0) printf "%d", (after-before) * 1000 / ms; else print 0 }')
disk=$(df -hP 2>/dev/null | awk 'NR>1 {printf "%s|%s/%s\\n", $6, $4, $2}' | head -n 12)
[ -z "$disk" ] && disk=$(df -h 2>/dev/null | awk 'NR>1 {printf "%s|%s/%s\\n", $NF, $(NF-2), $(NF-4)}' | head -n 12)
procs=$(ps -eo rss=,pcpu=,etimes=,comm= 2>/dev/null | awk 'NF >= 4 {printf "%.1fM|%s|%s|%s\\n", $1/1024, $2, $3, $4}')
[ -z "$procs" ] && procs=$(ps -eo rss=,pcpu=,comm= 2>/dev/null | awk 'NF >= 3 {printf "%.1fM|%s|0|%s\\n", $1/1024, $2, $3}')
[ -z "$procs" ] && procs=$(ps 2>/dev/null | awk 'NR>1 && NF >= 4 {printf "0.0M|0|0|%s\\n", $NF}')
echo "__IP__$ip"
echo "__UPTIME__$uptime_days"
echo "__LOAD__$load"
echo "__CPU__$cpu_pct"
echo "__MEM__$mem"
echo "__SWAP__$swap"
echo "__IFACES__$ifaces"
echo "__ACTIVE_IFACE__$active_iface"
echo "__RATES__$rx_rate|$tx_rate"
echo "__IFACE_RATES_START__"
awk -F'|' -v sample_ms="$sample_ms" '
  NR==FNR {rx[$1]=$2; tx[$1]=$3; next}
  NF >= 3 {
    prev_rx=rx[$1]
    prev_tx=tx[$1]
    curr_rx=$2
    curr_tx=$3
    rx_rate=(curr_rx-prev_rx) * 1000 / sample_ms
    tx_rate=(curr_tx-prev_tx) * 1000 / sample_ms
    printf "%s|%d|%d\\n", $1, rx_rate, tx_rate
  }
' "$before_file" "$after_file"
rm -f "$before_file" "$after_file"
echo "__IFACE_RATES_END__"
echo "__DISK_START__"
echo "$disk"
echo "__DISK_END__"
echo "__PROCS_START__"
echo "$procs"
echo "__PROCS_END__"
__TERMDOCK_METRICS__`
}

export function parseSystemMetrics(raw: string): SystemMetrics {
  const readLine = (key: string) => raw.split('\n').find((line) => line.startsWith(key))?.slice(key.length) ?? ''
  const readBlock = (start: string, end: string) => {
    const startIndex = raw.indexOf(start)
    const endIndex = raw.indexOf(end)
    if (startIndex === -1 || endIndex === -1 || endIndex <= startIndex) {
      return []
    }
    return raw
      .slice(startIndex + start.length, endIndex)
      .trim()
      .split('\n')
      .filter(Boolean)
  }

  const [memUsed, memTotal, memPercent, memApp, memCache, memKernel] = readLine('__MEM__').split('|')
  const [swapUsed, swapTotal, swapPercent] = readLine('__SWAP__').split('|')
  const [rxRate, txRate] = readLine('__RATES__').split('|')
  const interfaces = readLine('__IFACES__').split(',').filter(Boolean)
  const networkRatesByInterface = readBlock('__IFACE_RATES_START__', '__IFACE_RATES_END__').reduce<Record<string, { rx: string; tx: string }>>((acc, line) => {
    const [name, rx, tx] = line.split('|')
    if (!name) {
      return acc
    }
    acc[name] = {
      rx: formatRate(Number(rx) || 0),
      tx: formatRate(Number(tx) || 0)
    }
    return acc
  }, {})
  const networkSamplesByInterface = Object.fromEntries(
    Object.entries(networkRatesByInterface).map(([name, rates]) => [
      name,
      [{
        rx: parseRateLabelToNumber(rates.rx),
        tx: parseRateLabelToNumber(rates.tx)
      }]
    ])
  )
  const diskRows = readBlock('__DISK_START__', '__DISK_END__').map((line) => {
    const [diskPath, usage] = line.split('|')
    return { path: diskPath, usage }
  })
  const transientCollectorCommands = new Set(['ps', 'awk', 'bash', 'sleep', 'sh'])
  const groupedProcesses = new Map<string, {
    memoryMb: number
    cpu: number
    elapsedSeconds: number
  }>()
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

  return {
    ip: readLine('__IP__'),
    uptime: readLine('__UPTIME__') || '-',
    load: readLine('__LOAD__') || '-',
    cpuPercent: Number(readLine('__CPU__')) || 0,
    memoryPercent: Number(memPercent) || 0,
    memoryUsage: memTotal ? `${formatMegabytes(memUsed)}/${formatMegabytes(memTotal)}` : '0/0',
    memoryAppUsage: Number(memApp) > 0 ? formatMegabytes(memApp) : undefined,
    memoryCacheUsage: Number(memCache) > 0 ? formatMegabytes(memCache) : undefined,
    memoryKernelUsage: Number(memKernel) > 0 ? formatMegabytes(memKernel) : undefined,
    swapPercent: Number(swapPercent) || 0,
    swapUsage: swapTotal ? `${formatMegabytes(swapUsed)}/${formatMegabytes(swapTotal)}` : '0/0',
    diskRows,
    networkInterfaces: ['all', ...interfaces],
    activeNetworkInterface: 'all',
    networkRates: {
      rx: formatRate(Number(rxRate) || 0),
      tx: formatRate(Number(txRate) || 0)
    },
    networkSamples: [{
      rx: Number(rxRate) || 0,
      tx: Number(txRate) || 0
    }],
    networkRatesByInterface: {
      all: {
        rx: formatRate(Number(rxRate) || 0),
        tx: formatRate(Number(txRate) || 0)
      },
      ...networkRatesByInterface
    },
    networkSamplesByInterface: {
      all: [{
        rx: Number(rxRate) || 0,
        tx: Number(txRate) || 0
      }],
      ...networkSamplesByInterface
    },
    topProcesses
  }
}

function formatMegabytes(value?: string) {
  const numeric = Number(value) || 0
  if (numeric >= 1024) {
    return `${(numeric / 1024).toFixed(1)}G`
  }
  return `${numeric}M`
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

function formatProcessMegabytes(value: number) {
  if (value >= 1024) {
    return `${(value / 1024).toFixed(value >= 10 * 1024 ? 0 : 1)}G`
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)}M`
}

function parseRateLabelToNumber(value: string) {
  const numeric = Number.parseFloat(value) || 0
  const upper = value.toUpperCase()
  if (upper.endsWith('M')) {
    return Math.round(numeric * 1024 * 1024)
  }
  if (upper.endsWith('K')) {
    return Math.round(numeric * 1024)
  }
  return Math.round(numeric)
}
