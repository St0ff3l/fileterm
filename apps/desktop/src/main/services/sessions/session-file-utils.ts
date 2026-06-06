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
  return toResolvedFtpRemoteFileItem(basePath, entry, entry.type === FileType.Directory || entry.isDirectory)
}

export function toResolvedFtpRemoteFileItem(basePath: string, entry: FileInfo, isDirectory: boolean): RemoteFileItem {
  const fullPath = path.posix.join(basePath, entry.name)
  const resolvedType = isDirectory ? FileType.Directory : entry.type
  return {
    path: fullPath,
    name: entry.name,
    type: isDirectory ? 'folder' : 'file',
    modified: entry.modifiedAt ? formatDate(entry.modifiedAt) : entry.rawModifiedAt,
    size: isDirectory ? '-' : formatBytes(entry.size),
    permission: formatFtpPermissions(resolvedType, entry.permissions),
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
  return `cd / >/dev/null 2>&1 || true
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
cpu_user_pct=$(awk -v diff_total="$diff_total" -v before="$user" -v after="$user2" 'BEGIN { if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }')
cpu_system_pct=$(awk -v diff_total="$diff_total" -v before="$system" -v after="$system2" 'BEGIN { if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }')
cpu_nice_pct=$(awk -v diff_total="$diff_total" -v before="$nice" -v after="$nice2" 'BEGIN { if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }')
cpu_idle_pct=$(awk -v diff_total="$diff_total" -v before="$idle1" -v after="$idle2sum" 'BEGIN { if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }')
cpu_iowait_pct=$(awk -v diff_total="$diff_total" -v before="$iowait" -v after="$iowait2" 'BEGIN { if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }')
cpu_irq_pct=$(awk -v diff_total="$diff_total" -v before="$irq" -v after="$irq2" 'BEGIN { if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }')
cpu_softirq_pct=$(awk -v diff_total="$diff_total" -v before="$softirq" -v after="$softirq2" 'BEGIN { if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }')
cpu_steal_pct=$(awk -v diff_total="$diff_total" -v before="$steal" -v after="$steal2" 'BEGIN { if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }')
os_name=$( ( . /etc/os-release >/dev/null 2>&1 && printf "%s" "$PRETTY_NAME" ) 2>/dev/null )
[ -z "$os_name" ] && os_name=$(uname -s 2>/dev/null)
kernel_name=$(uname -s 2>/dev/null)
kernel_version=$(uname -r 2>/dev/null)
architecture=$(uname -m 2>/dev/null)
hostname_value=$(hostname 2>/dev/null)
best_ip=""
best_ip_rank=99
rank_ip() {
  case "$1" in
    10.*|192.168.*|172.1[6-9].*|172.2[0-9].*|172.3[0-1].*)
      echo 1
      ;;
    fc*:*|fd*:*)
      echo 2
      ;;
    100.6[4-9].*|100.[7-9][0-9].*|100.1[0-1][0-9].*|100.12[0-7].*)
      echo 3
      ;;
    *:*)
      echo 5
      ;;
    *)
      echo 4
      ;;
  esac
}
consider_ip() {
  candidate="$1"
  [ -z "$candidate" ] && return
  candidate=\${candidate%%/*}
  case "$candidate" in
    127.*|169.254.*|::1|fe80:*)
      return
      ;;
  esac
  rank=$(rank_ip "$candidate")
  if [ "$rank" -lt "$best_ip_rank" ]; then
    best_ip="$candidate"
    best_ip_rank="$rank"
  fi
}
for candidate in $(ip route get 1 2>/dev/null | awk 'NR==1 {for (i=1; i<=NF; i++) if ($i == "src") {print $(i+1)}}'); do
  consider_ip "$candidate"
done
for candidate in $(hostname -I 2>/dev/null); do
  consider_ip "$candidate"
done
for candidate in $(ip -o addr show up scope global 2>/dev/null | awk '{print $4}'); do
  consider_ip "$candidate"
done
for candidate in $(ifconfig 2>/dev/null | awk '/inet / && $2 !~ /^127\\./ {print $2}'); do
  consider_ip "$candidate"
done
for candidate in $(ifconfig 2>/dev/null | awk '/inet addr:/ && $2 !~ /127\\.0\\.0\\.1/ {sub("addr:", "", $2); print $2}'); do
  consider_ip "$candidate"
done
ip="$best_ip"
uptime_seconds=$(awk '{print int($1)}' /proc/uptime 2>/dev/null)
uptime_days=$(awk '{print int($1/86400) " 天"}' /proc/uptime 2>/dev/null)
if [ -z "$uptime_days" ]; then
  uptime_days=$(uptime 2>/dev/null | awk -F'(up |, *[0-9]+ user)' 'NF>1 {gsub(/^ +| +$/, "", $2); print $2; exit}')
fi
load=$(awk '{printf "%s, %s, %s", $1, $2, $3}' /proc/loadavg 2>/dev/null)
if [ -z "$load" ]; then
  load=$(uptime 2>/dev/null | sed -n 's/.*load averages\\{0,1\\}: *//p; s/.*load average: *//p' | awk -F',' 'NF>=3 {gsub(/^ +| +$/, "", $1); gsub(/^ +| +$/, "", $2); gsub(/^ +| +$/, "", $3); printf "%s, %s, %s", $1, $2, $3; exit}')
fi
mem=$(awk 'BEGIN { total=available=memfree=buffers=cached=shmem=sreclaimable=slab=kernelstack=pagetables=0 }
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
  END {
    if (available == 0) available=memfree+buffers+cached+sreclaimable-shmem
    if (available < 0) available=0
    if (total > 0) {
      used=total-available
      if (used < 0) used=0
      percent=int(used*100/total)
      cache_total=buffers+cached+sreclaimable-shmem
      if (cache_total < 0) cache_total=0
      kernel_total=slab-sreclaimable+kernelstack+pagetables
      if (kernel_total < 0) kernel_total=0
      kernel=kernel_total
      if (kernel > used) kernel=used
      remaining=used-kernel
      cache=cache_total
      if (cache > remaining) cache=remaining
      if (cache < 0) cache=0
      app=remaining-cache
      if (app < 0) app=0
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
cpu_info=$(awk -F: '
  /^model name[[:space:]]*:/ {
    current=$2
    sub(/^[[:space:]]+/, "", current)
    model_order[++model_count]=current
    seen[current]=1
  }
  /^cpu cores[[:space:]]*:/ {
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (cores[current] == "") cores[current]=value
  }
  /^cpu MHz[[:space:]]*:/ {
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (mhz[current] == "") mhz[current]=sprintf("%.3f", value + 0)
  }
  /^cache size[[:space:]]*:/ {
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (cache[current] == "") cache[current]=value
  }
  /^bogomips[[:space:]]*:/ {
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (bogomips[current] == "") bogomips[current]=value
  }
  END {
    for (index = 1; index <= model_count; index++) {
      model=model_order[index]
      if (printed[model]) continue
      printed[model]=1
      printf "%s|%s|%s|%s|%s\\n", model, (cores[model] == "" ? "0" : cores[model]), (mhz[model] == "" ? "-" : mhz[model]), (cache[model] == "" ? "-" : cache[model]), (bogomips[model] == "" ? "-" : bogomips[model])
    }
  }
' /proc/cpuinfo 2>/dev/null)
if [ -z "$cpu_info" ]; then
  cpu_model=$(sed -n 's/^model name[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo 2>/dev/null | head -n 1)
  cpu_cores=$(sed -n 's/^cpu cores[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo 2>/dev/null | head -n 1)
  cpu_mhz=$(sed -n 's/^cpu MHz[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo 2>/dev/null | head -n 1)
  cpu_cache=$(sed -n 's/^cache size[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo 2>/dev/null | head -n 1)
  cpu_bogomips=$(sed -n 's/^bogomips[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo 2>/dev/null | head -n 1)
  if [ -n "$cpu_model" ]; then
    cpu_info="$cpu_model|\${cpu_cores:-0}|\${cpu_mhz:--}|\${cpu_cache:--}|\${cpu_bogomips:--}"
  fi
fi
if [ -z "$cpu_info" ]; then
  cpu_info=$(LC_ALL=C lscpu 2>/dev/null | awk -F: '
    function trim(value) {
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      return value
    }
    /^Model name:/ { model=trim($2) }
    /^Socket\\(s\\):/ { sockets=trim($2) + 0 }
    /^Core\\(s\\) per socket:/ { cores_per_socket=trim($2) + 0 }
    /^CPU\\(s\\):/ && total_cores == 0 { total_cores=trim($2) + 0 }
    /^CPU max MHz:/ { frequency=trim($2) }
    /^CPU MHz:/ && frequency == "" { frequency=trim($2) }
    /^L3 cache:/ { cache=trim($2) }
    /^L2 cache:/ && cache == "" { cache=trim($2) }
    /^BogoMIPS:/ { bogomips=trim($2) }
    END {
      if (total_cores == 0 && sockets > 0 && cores_per_socket > 0) total_cores=sockets * cores_per_socket
      if (model != "") printf "%s|%s|%s|%s|%s\\n", model, (total_cores > 0 ? total_cores : 0), (frequency == "" ? "-" : sprintf("%.3f", frequency + 0)), (cache == "" ? "-" : cache), (bogomips == "" ? "-" : bogomips)
    }
  ')
fi
gpu_info=$(nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader,nounits 2>/dev/null | awk -F',' '
  function trim(value) {
    sub(/^[[:space:]]+/, "", value)
    sub(/[[:space:]]+$/, "", value)
    return value
  }
  NF >= 3 {
    model=trim($1)
    driver=trim($2)
    memory=trim($3)
    printf "%s|NVIDIA|%s|%s MiB\\n", model, (driver == "" ? "-" : driver), (memory == "" ? "-" : memory)
  }
')
if [ -z "$gpu_info" ]; then
  gpu_info=$(lspci 2>/dev/null | awk '
    BEGIN { IGNORECASE=1 }
    /VGA compatible controller|3D controller|Display controller/ {
      line=$0
      sub(/^[[:xdigit:]:.]+[[:space:]]+[^:]+: /, "", line)
      vendor=line
      sub(/[[:space:]].*$/, "", vendor)
      printf "%s|%s|-|-\\n", line, (vendor == "" ? "-" : vendor)
    }
  ')
fi
if [ -z "$gpu_info" ]; then
  gpu_info=$(lshw -C display 2>/dev/null | awk -F: '
    function trim(value) {
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      return value
    }
    /^  \\*-display/ {
      if (product != "" || vendor != "" || driver != "" || memory != "") {
        printf "%s|%s|%s|%s\\n", (product == "" ? "-" : product), (vendor == "" ? "-" : vendor), (driver == "" ? "-" : driver), (memory == "" ? "-" : memory)
      }
      product=""
      vendor=""
      driver=""
      memory=""
      next
    }
    /^       product:/ { product=trim($2) }
    /^       vendor:/ { vendor=trim($2) }
    /^       size:/ { memory=trim($2) }
    /^       configuration:/ {
      if (match($0, /driver=[^[:space:]]+/)) {
        driver=substr($0, RSTART + 7, RLENGTH - 7)
      }
    }
    END {
      if (product != "" || vendor != "" || driver != "" || memory != "") {
        printf "%s|%s|%s|%s\\n", (product == "" ? "-" : product), (vendor == "" ? "-" : vendor), (driver == "" ? "-" : driver), (memory == "" ? "-" : memory)
      }
    }
  ')
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
filesystems=$(df -hP 2>/dev/null | awk 'NR>1 {printf "%s|%s|%s|%s|%s|%s\\n", $1, $2, $3, $5, $4, $6}' | head -n 20)
procs=$(ps -eo rss=,pcpu=,etimes=,comm= 2>/dev/null | awk 'NF >= 4 {printf "%.1fM|%s|%s|%s\\n", $1/1024, $2, $3, $4}')
[ -z "$procs" ] && procs=$(ps -eo rss=,pcpu=,comm= 2>/dev/null | awk 'NF >= 3 {printf "%.1fM|%s|0|%s\\n", $1/1024, $2, $3}')
[ -z "$procs" ] && procs=$(ps 2>/dev/null | awk 'NR>1 && NF >= 4 {printf "0.0M|0|0|%s\\n", $NF}')
echo "__OS__$os_name"
echo "__KERNEL_NAME__$kernel_name"
echo "__KERNEL_VERSION__$kernel_version"
echo "__ARCH__$architecture"
echo "__HOSTNAME__$hostname_value"
echo "__IP__$ip"
echo "__UPTIME__$uptime_days"
echo "__UPTIME_SECONDS__$uptime_seconds"
echo "__LOAD__$load"
echo "__CPU__$cpu_pct"
echo "__CPU_USAGE__$cpu_user_pct|$cpu_system_pct|$cpu_nice_pct|$cpu_idle_pct|$cpu_iowait_pct|$cpu_irq_pct|$cpu_softirq_pct|$cpu_steal_pct"
echo "__MEM__$mem"
echo "__SWAP__$swap"
echo "__CPUINFO_START__"
echo "$cpu_info"
echo "__CPUINFO_END__"
echo "__GPUINFO_START__"
echo "$gpu_info"
echo "__GPUINFO_END__"
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
    printf "%s|%.0f|%.0f|%d|%d\\n", $1, curr_rx, curr_tx, rx_rate, tx_rate
  }
' "$before_file" "$after_file"
rm -f "$before_file" "$after_file"
echo "__IFACE_RATES_END__"
echo "__DISK_START__"
echo "$disk"
echo "__DISK_END__"
echo "__FILESYSTEMS_START__"
echo "$filesystems"
echo "__FILESYSTEMS_END__"
echo "__PROCS_START__"
echo "$procs"
echo "__PROCS_END__"
`
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
  const [cpuUser, cpuSystem, cpuNice, cpuIdle, cpuIoWait, cpuIrq, cpuSoftIrq, cpuSteal] = readLine('__CPU_USAGE__').split('|')
  const [rxRate, txRate] = readLine('__RATES__').split('|')
  const interfaces = readLine('__IFACES__').split(',').filter(Boolean)
  const networkInterfaceRows = readBlock('__IFACE_RATES_START__', '__IFACE_RATES_END__').map((line) => {
    const [name, rxTotal, txTotal, rx, tx] = line.split('|')
    return {
      name,
      txTotal: formatNetworkBytes(Number(txTotal) || 0),
      rxTotal: formatNetworkBytes(Number(rxTotal) || 0),
      txRate: formatRate(Number(tx) || 0),
      rxRate: formatRate(Number(rx) || 0)
    }
  }).filter((row) => row.name)
  const networkRatesByInterface = networkInterfaceRows.reduce<Record<string, { rx: string; tx: string }>>((acc, row) => {
    const { name, rxRate: rowRxRate, txRate: rowTxRate } = row
    if (!name) {
      return acc
    }
    acc[name] = {
      rx: rowRxRate,
      tx: rowTxRate
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
  const fileSystemRows = readBlock('__FILESYSTEMS_START__', '__FILESYSTEMS_END__').map((line) => {
    const [name, size, used, usagePercent, available, mountPoint] = line.split('|')
    return { name, size, used, usagePercent, available, mountPoint }
  })
  const cpuInfoRows = readBlock('__CPUINFO_START__', '__CPUINFO_END__').map((line) => {
    const [model, cores, frequencyMHz, cache, bogomips] = line.split('|')
    return {
      model,
      cores: Number(cores) || 0,
      frequencyMHz,
      cache,
      bogomips
    }
  }).filter((row) => row.model)
  const gpuInfoRows = readBlock('__GPUINFO_START__', '__GPUINFO_END__').map((line) => {
    const [model, vendor, driver, memory] = line.split('|')
    return {
      model,
      vendor: vendor || '-',
      driver: driver || '-',
      memory: memory || '-'
    }
  }).filter((row) => row.model)
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
    uptimeSeconds: Number(readLine('__UPTIME_SECONDS__')) || undefined,
    load: readLine('__LOAD__') || '-',
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
    memoryPercent: Number(memPercent) || 0,
    memoryUsage: memTotal ? `${formatMegabytes(memUsed)}/${formatMegabytes(memTotal)}` : '0/0',
    memoryAppUsage: Number(memApp) > 0 ? formatMegabytes(memApp) : undefined,
    memoryCacheUsage: Number(memCache) > 0 ? formatMegabytes(memCache) : undefined,
    memoryKernelUsage: Number(memKernel) > 0 ? formatMegabytes(memKernel) : undefined,
    memoryBreakdown: {
      total: formatMegabytes(memTotal),
      used: formatMegabytes(memUsed),
      available: formatMegabytes(Math.max((Number(memTotal) || 0) - (Number(memUsed) || 0), 0)),
      percent: Number(memPercent) || 0
    },
    swapPercent: Number(swapPercent) || 0,
    swapUsage: swapTotal ? `${formatMegabytes(swapUsed)}/${formatMegabytes(swapTotal)}` : '0/0',
    swapBreakdown: {
      total: formatMegabytes(swapTotal),
      used: formatMegabytes(swapUsed),
      available: formatMegabytes(Math.max((Number(swapTotal) || 0) - (Number(swapUsed) || 0), 0)),
      percent: Number(swapPercent) || 0
    },
    diskRows,
    fileSystemRows,
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
    networkInterfaceRows,
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

function formatMegabytes(value?: string | number) {
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
