import type { RemoteSystemPlatform } from './types.js'

export const POSIX_METRICS_COMPLETE_MARKER = '__FILETERM_METRICS_COMPLETE__'

export function assertPosixMetricsComplete(raw: string, platform: 'Linux' | 'BusyBox') {
  if (!raw.includes(POSIX_METRICS_COMPLETE_MARKER)) {
    throw new Error(`${platform} metrics script did not emit ${POSIX_METRICS_COMPLETE_MARKER}`)
  }
}

export function buildPosixMetricsCommand(platform: Extract<RemoteSystemPlatform, 'linux' | 'busybox'>) {
  return `cd / >/dev/null 2>&1 || true
sleep_interval="0.15"
sleep "$sleep_interval" >/dev/null 2>&1 || sleep_interval="1"
run_bounded() {
  limit="$1"
  shift
  if command -v timeout >/dev/null 2>&1; then
    if timeout -k 1 1 true >/dev/null 2>&1; then
      timeout -k 1 "$limit" "$@"
    else
      timeout "$limit" "$@"
    fi
    return $?
  fi
  if command -v busybox >/dev/null 2>&1 && busybox timeout 1 true >/dev/null 2>&1; then
    if busybox timeout -k 1 1 true >/dev/null 2>&1; then
      busybox timeout -k 1 "$limit" "$@"
    else
      busybox timeout "$limit" "$@"
    fi
    return $?
  fi
  return 124
}
has_bounded_runner() {
  if command -v timeout >/dev/null 2>&1 && timeout 1 true >/dev/null 2>&1; then
    return 0
  fi
  if command -v busybox >/dev/null 2>&1 && busybox timeout 1 true >/dev/null 2>&1; then
    return 0
  fi
  return 1
}
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
[ -z "$os_name" ] && os_name=$(sed -n 's/^DISTRIB_DESCRIPTION=['"'"'"]\\{0,1\\}\\(.*\\)['"'"'"]\\{0,1\\}$/\\1/p' /etc/openwrt_release 2>/dev/null | head -n 1)
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
is_virtual_iface() {
  case "$1" in
    tailscale*|zt*|zerotier*|docker*|veth*|virbr*|br-*|cni*|flannel*|tun*|tap*|wg*|vethernet*)
      return 0
      ;;
  esac
  return 1
}
default_ifaces=$(
  {
    ip route show default 2>/dev/null | awk '{for (i=1; i<=NF; i++) if ($i == "dev") print $(i+1)}'
    awk '$2 == "00000000" {print $1}' /proc/net/route 2>/dev/null
  } | awk 'NF && !seen[$0]++'
)
for iface in $default_ifaces; do
  is_virtual_iface "$iface" && continue
  for candidate in $(ip -o -4 addr show dev "$iface" scope global 2>/dev/null | awk '{print $4}'); do
    consider_ip "$candidate"
  done
  for candidate in $(ifconfig "$iface" 2>/dev/null | awk '/inet / && $2 !~ /^127\\./ {print $2} /inet addr:/ && $2 !~ /127\\.0\\.0\\.1/ {sub("addr:", "", $2); print $2}'); do
    consider_ip "$candidate"
  done
done
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
if [ -z "$uptime_seconds" ]; then
  uptime_seconds=$(uptime 2>/dev/null | awk '
    /day/ {
      for (i=1; i<=NF; i++) {
        if ($i ~ /day/) days=$(i-1)
      }
    }
    {
      if (match($0, /[0-9]+:[0-9]+/)) {
        split(substr($0, RSTART, RLENGTH), time_parts, ":")
        hours=time_parts[1]
        minutes=time_parts[2]
      }
      printf "%d", (days * 86400) + (hours * 3600) + (minutes * 60)
      exit
    }
  ')
fi
load=$(awk '{printf "%s, %s, %s", $1, $2, $3}' /proc/loadavg 2>/dev/null)
if [ -z "$load" ]; then
  load=$(uptime 2>/dev/null | sed -n 's/.*load averages\\{0,1\\}: *//p; s/.*load average: *//p' | awk -F',' 'NF>=3 {gsub(/^ +| +$/, "", $1); gsub(/^ +| +$/, "", $2); gsub(/^ +| +$/, "", $3); printf "%s, %s, %s", $1, $2, $3; exit}')
fi
mem_bytes=$(awk 'BEGIN { total=available=memfree=buffers=cached=shmem=anonpages=sreclaimable=slab=kernelstack=pagetables=0 }
  /^MemTotal:/ { total=$2 * 1024 }
  /^MemAvailable:/ { available=$2 * 1024 }
  /^MemFree:/ { memfree=$2 * 1024 }
  /^Buffers:/ { buffers=$2 * 1024 }
  /^Cached:/ { cached=$2 * 1024 }
  /^Shmem:/ { shmem=$2 * 1024 }
  /^AnonPages:/ { anonpages=$2 * 1024 }
  /^SReclaimable:/ { sreclaimable=$2 * 1024 }
  /^Slab:/ { slab=$2 * 1024 }
  /^KernelStack:/ { kernelstack=$2 * 1024 }
  /^PageTables:/ { pagetables=$2 * 1024 }
  END {
    if (available == 0) available=memfree+buffers+cached+sreclaimable-shmem
    if (available < 0) available=0
    if (total > 0) {
      used=total-available
      if (used < 0) used=0
      percent=int(used*100/total)
      kernel_total=slab-sreclaimable+kernelstack+pagetables
      if (kernel_total < 0) kernel_total=0
      kernel=kernel_total
      if (kernel > used) kernel=used
      remaining=used-kernel
      app=anonpages+shmem
      if (app > remaining) app=remaining
      if (app < 0) app=0
      cache=remaining-app
      if (cache < 0) cache=0
      printf "%.0f|%.0f|%.0f|%d|%.0f|%.0f|%.0f", used, total, available, percent, app, cache, kernel
    }
  }' /proc/meminfo 2>/dev/null)
if [ -z "$mem_bytes" ]; then
  mem_bytes=$(free 2>/dev/null | awk '/^Mem:/ {
    total=$2 * 1024
    used=$3 * 1024
    available=$7 * 1024
    if (available == 0) available=total-used
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%.0f|%.0f|%.0f|%d|0|0|0", used, total, available, percent
    exit
  }')
fi
mem=$(printf "%s" "$mem_bytes" | awk -F'|' 'NF >= 4 {printf "%d|%d|%d|%d|%d|%d", $1/1024/1024, $2/1024/1024, $4, $5/1024/1024, $6/1024/1024, $7/1024/1024}')
swap_bytes=$(awk 'BEGIN { total=free=0 }
  /^SwapTotal:/ { total=$2 * 1024 }
  /^SwapFree:/ { free=$2 * 1024 }
  END {
    used=total-free
    if (used < 0) used=0
    available=free
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%.0f|%.0f|%.0f|%d", used, total, available, percent
  }' /proc/meminfo 2>/dev/null)
if [ -z "$swap_bytes" ]; then
  swap_bytes=$(free 2>/dev/null | awk '/^Swap:/ {
    total=$2 * 1024
    used=$3 * 1024
    available=total-used
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%.0f|%.0f|%.0f|%d", used, total, available, percent
    exit
  }')
fi
swap=$(printf "%s" "$swap_bytes" | awk -F'|' 'NF >= 4 {printf "%d|%d|%d", $1/1024/1024, $2/1024/1024, $4}')
cpu_info=$(awk -F: '
  /^model name[[:space:]]*:/ || /^Hardware[[:space:]]*:/ || /^Processor[[:space:]]*:/ {
    current=$2
    sub(/^[[:space:]]+/, "", current)
    if (current != "") {
      model_order[++model_count]=current
      seen[current]=1
    }
  }
  /^cpu cores[[:space:]]*:/ {
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (cores[current] == "") cores[current]=value
  }
  /^cpu MHz[[:space:]]*:/ || /^BogoMIPS[[:space:]]*:/ {
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (mhz[current] == "") mhz[current]=sprintf("%.3f", value + 0)
  }
  /^cache size[[:space:]]*:/ {
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (cache[current] == "") cache[current]=value
  }
  /^bogomips[[:space:]]*:/ || /^BogoMIPS[[:space:]]*:/ {
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
gpu_info=$(run_bounded 1 nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader,nounits 2>/dev/null | awk -F',' '
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
  gpu_info=$(run_bounded 1 lspci 2>/dev/null | awk '
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
ifaces=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); if (name != "lo") { if (out != "") out=out ","; out=out name }} END {print out}' /proc/net/dev 2>/dev/null)
active_iface=$(awk '$2 == 00000000 {print $1; exit}' /proc/net/route 2>/dev/null)
[ -z "$active_iface" ] && active_iface=$(echo "$ifaces" | awk -F, '{print $1}')
rx1=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[2]} END {printf "%.0f", sum+0}' /proc/net/dev 2>/dev/null)
tx1=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[10]} END {printf "%.0f", sum+0}' /proc/net/dev 2>/dev/null)
before_file="/tmp/fileterm-if-before-$$"
after_file="/tmp/fileterm-if-after-$$"
trap 'rm -f "$before_file" "$after_file"' 0 1 2 15
awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") printf "%s|%.0f|%.0f\\n", name, values[2], values[10]}' /proc/net/dev 2>/dev/null > "$before_file"
sleep "$sleep_interval"
rx2=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[2]} END {printf "%.0f", sum+0}' /proc/net/dev 2>/dev/null)
tx2=$(awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[10]} END {printf "%.0f", sum+0}' /proc/net/dev 2>/dev/null)
awk -F: 'NR>2 {name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") printf "%s|%.0f|%.0f\\n", name, values[2], values[10]}' /proc/net/dev 2>/dev/null > "$after_file"
sample_ms=$(awk -v interval="$sleep_interval" 'BEGIN { printf "%d", interval * 1000 }')
[ -z "$sample_ms" ] && sample_ms=1000
rx_rate=$(awk -v before="$rx1" -v after="$rx2" -v ms="$sample_ms" 'BEGIN { if (ms > 0) printf "%d", (after-before) * 1000 / ms; else print 0 }')
tx_rate=$(awk -v before="$tx1" -v after="$tx2" -v ms="$sample_ms" 'BEGIN { if (ms > 0) printf "%d", (after-before) * 1000 / ms; else print 0 }')
df_flags="-kP"
df -kPl / >/dev/null 2>&1 && df_flags="-kPl"
if has_bounded_runner; then
  df_output=$(run_bounded 2 df "$df_flags" 2>/dev/null)
else
  local_mounts=$(awk '
    $3 ~ /^(overlay|squashfs|tmpfs|ramfs|ext[234]|xfs|btrfs|f2fs|vfat|ubifs|jffs2|zfs)$/ && !seen[$2]++ { print $2 }
  ' /proc/mounts 2>/dev/null | head -n 20)
  [ -z "$local_mounts" ] && local_mounts="/"
  df_output=$(df "$df_flags" $local_mounts 2>/dev/null)
fi
disk=$(printf "%s\\n" "$df_output" | awk 'NR>1 {printf "%s|%sK/%sK\\n", $6, $4, $2}' | head -n 12)
filesystems=$(printf "%s\\n" "$df_output" | awk 'NR>1 {printf "%s|%sK|%sK|%s|%sK|%s\\n", $1, $2, $3, $5, $4, $6}' | head -n 20)
if has_bounded_runner; then
  procs=$(run_bounded 1 ps -eo rss=,pcpu=,etimes=,comm= 2>/dev/null | awk 'NF >= 4 {printf "%.1fM|%s|%s|%s\\n", $1/1024, $2, $3, $4}')
  [ -z "$procs" ] && procs=$(run_bounded 1 ps 2>/dev/null | awk 'NR>1 && NF >= 5 {proc_name=$5; sub(/^.*\\//, "", proc_name); printf "%.1fM|0|0|%s\\n", $3/1024, proc_name}')
else
  procs=$(ps -eo rss=,pcpu=,etimes=,comm= 2>/dev/null | awk 'NF >= 4 {printf "%.1fM|%s|%s|%s\\n", $1/1024, $2, $3, $4}')
  [ -z "$procs" ] && procs=$(ps 2>/dev/null | awk 'NR>1 && NF >= 5 {proc_name=$5; sub(/^.*\\//, "", proc_name); printf "%.1fM|0|0|%s\\n", $3/1024, proc_name}')
fi
echo "__PLATFORM__${platform}"
echo "__OS__$os_name"
echo "__KERNEL_NAME__$kernel_name"
echo "__KERNEL_VERSION__$kernel_version"
echo "__ARCH__$architecture"
echo "__HOSTNAME__$hostname_value"
echo "__IP__$ip"
echo "__UPTIME__"
echo "__UPTIME_SECONDS__$uptime_seconds"
echo "__LOAD__$load"
echo "__CPU__$cpu_pct"
echo "__CPU_USAGE__$cpu_user_pct|$cpu_system_pct|$cpu_nice_pct|$cpu_idle_pct|$cpu_iowait_pct|$cpu_irq_pct|$cpu_softirq_pct|$cpu_steal_pct"
echo "__MEM__$mem"
echo "__MEM_BYTES__$mem_bytes"
echo "__SWAP__$swap"
echo "__SWAP_BYTES__$swap_bytes"
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
echo "${POSIX_METRICS_COMPLETE_MARKER}"
`
}
