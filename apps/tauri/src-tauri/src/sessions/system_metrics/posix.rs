pub fn build_posix_metrics_command(platform: &str) -> String {
    let complete_marker = "__FILETERM_METRICS_COMPLETE__";
    format!(
        r#"cd / >/dev/null 2>&1 || true
sleep_interval="0.15"
sleep "$sleep_interval" >/dev/null 2>&1 || sleep_interval="1"
run_bounded() {{
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
}}
has_bounded_runner() {{
  if command -v timeout >/dev/null 2>&1 && timeout 1 true >/dev/null 2>&1; then
    return 0
  fi
  if command -v busybox >/dev/null 2>&1 && busybox timeout 1 true >/dev/null 2>&1; then
    return 0
  fi
  return 1
}}
read_cpu_stat() {{
  awk '/^cpu / {{print $2, $3, $4, $5, $6, $7, $8, $9; exit}}' /proc/stat 2>/dev/null
}}
read_process_ticks() {{
  awk '
    {{
      path=FILENAME
      sub(/^\/proc\//, "", path)
      sub(/\/stat$/, "", path)
      line=$0
      sub(/^[0-9]+ \(.+\) /, "", line)
      count=split(line, fields, /[[:space:]]+/)
      if (count >= 13) printf "%s|%s\n", path, fields[12] + fields[13]
    }}
  ' /proc/[0-9]*/stat 2>/dev/null
}}
set -- $(read_cpu_stat)
user=${{1:-0}}
nice=${{2:-0}}
system=${{3:-0}}
idle=${{4:-0}}
iowait=${{5:-0}}
irq=${{6:-0}}
softirq=${{7:-0}}
steal=${{8:-0}}
total1=$((user+nice+system+idle+iowait+irq+softirq+steal))
idle1=$((idle+iowait))
process_ticks_before_file="/tmp/fileterm-procs-before-$$"
process_ticks_after_file="/tmp/fileterm-procs-after-$$"
process_cpu_file="/tmp/fileterm-procs-cpu-$$"
process_cpu_tmp_file="/tmp/fileterm-procs-cpu-tmp-$$"
gpu_info_file="/tmp/fileterm-gpu-info-$$"
trap 'rm -f "$before_file" "$after_file" "$process_ticks_before_file" "$process_ticks_after_file" "$process_cpu_file" "$process_cpu_tmp_file" "$gpu_info_file"' 0 1 2 15
read_process_ticks > "$process_ticks_before_file"
sleep "$sleep_interval"
read_process_ticks > "$process_ticks_after_file"
set -- $(read_cpu_stat)
user2=${{1:-0}}
nice2=${{2:-0}}
system2=${{3:-0}}
idle2=${{4:-0}}
iowait2=${{5:-0}}
irq2=${{6:-0}}
softirq2=${{7:-0}}
steal2=${{8:-0}}
total2=$((user2+nice2+system2+idle2+iowait2+irq2+softirq2+steal2))
idle2sum=$((idle2+iowait2))
diff_total=$((total2-total1))
diff_idle=$((idle2sum-idle1))
if [ "$diff_total" -gt 0 ]; then cpu_pct=$((100*(diff_total-diff_idle)/diff_total)); else cpu_pct=0; fi
cpu_user_pct=$(awk -v diff_total="$diff_total" -v before="$user" -v after="$user2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_system_pct=$(awk -v diff_total="$diff_total" -v before="$system" -v after="$system2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_nice_pct=$(awk -v diff_total="$diff_total" -v before="$nice" -v after="$nice2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_idle_pct=$(awk -v diff_total="$diff_total" -v before="$idle1" -v after="$idle2sum" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_iowait_pct=$(awk -v diff_total="$diff_total" -v before="$iowait" -v after="$iowait2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_irq_pct=$(awk -v diff_total="$diff_total" -v before="$irq" -v after="$irq2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_softirq_pct=$(awk -v diff_total="$diff_total" -v before="$softirq" -v after="$softirq2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_steal_pct=$(awk -v diff_total="$diff_total" -v before="$steal" -v after="$steal2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
os_name=$( ( . /etc/os-release >/dev/null 2>&1 && printf "%s" "$PRETTY_NAME" ) 2>/dev/null )
[ -z "$os_name" ] && os_name=$(sed -n 's/^DISTRIB_DESCRIPTION=['"'"'"]\\{{0,1\\}}\\(.*\\)['"'"'"]\\{{0,1\\}}$/\\1/p' /etc/openwrt_release 2>/dev/null | head -n 1)
[ -z "$os_name" ] && os_name=$(uname -s 2>/dev/null)
kernel_name=$(uname -s 2>/dev/null)
kernel_version=$(uname -r 2>/dev/null)
architecture=$(uname -m 2>/dev/null)
hostname_value=$(hostname 2>/dev/null)
best_ip=""
best_ip_rank=99
rank_ip() {{
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
}}
consider_ip() {{
  candidate="$1"
  [ -z "$candidate" ] && return
  candidate=${{candidate%%/*}}
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
}}
is_virtual_iface() {{
  case "$1" in
    tailscale*|zt*|zerotier*|docker*|veth*|virbr*|br-*|cni*|flannel*|tun*|tap*|wg*|vethernet*)
      return 0
      ;;
  esac
  return 1
}}
default_ifaces=$(
  {{
    ip route show default 2>/dev/null | awk '{{for (i=1; i<=NF; i++) if ($i == "dev") print $(i+1)}}'
    awk '$2 == "00000000" {{print $1}}' /proc/net/route 2>/dev/null
  }} | awk 'NF && !seen[$0]++'
)
for iface in $default_ifaces; do
  is_virtual_iface "$iface" && continue
  for candidate in $(ip -o -4 addr show dev "$iface" scope global 2>/dev/null | awk '{{print $4}}'); do
    consider_ip "$candidate"
  done
  for candidate in $(ifconfig "$iface" 2>/dev/null | awk '/inet / && $2 !~ /^127\\./ {{print $2}} /inet addr:/ && $2 !~ /127\\.0\\.0\\.1/ {{sub("addr:", "", $2); print $2}}'); do
    consider_ip "$candidate"
  done
done
for candidate in $(ip route get 1 2>/dev/null | awk 'NR==1 {{for (i=1; i<=NF; i++) if ($i == "src") {{print $(i+1)}}}}'); do
  consider_ip "$candidate"
done
for candidate in $(hostname -I 2>/dev/null); do
  consider_ip "$candidate"
done
for candidate in $(ip -o addr show up scope global 2>/dev/null | awk '{{print $4}}'); do
  consider_ip "$candidate"
done
for candidate in $(ifconfig 2>/dev/null | awk '/inet / && $2 !~ /^127\\./ {{print $2}}'); do
  consider_ip "$candidate"
done
for candidate in $(ifconfig 2>/dev/null | awk '/inet addr:/ && $2 !~ /127\\.0\\.0\\.1/ {{sub("addr:", "", $2); print $2}}'); do
  consider_ip "$candidate"
done
ip="$best_ip"
uptime_seconds=$(awk '{{print int($1)}}' /proc/uptime 2>/dev/null)
if [ -z "$uptime_seconds" ]; then
  uptime_seconds=$(uptime 2>/dev/null | awk '
    /day/ {{
      for (i=1; i<=NF; i++) {{
        if ($i ~ /day/) days=$(i-1)
      }}
    }}
    {{
      if (match($0, /[0-9]+:[0-9]+/)) {{
        split(substr($0, RSTART, RLENGTH), time_parts, ":")
        hours=time_parts[1]
        minutes=time_parts[2]
      }}
      printf "%d", (days * 86400) + (hours * 3600) + (minutes * 60)
      exit
    }}
  ')
fi
load=$(awk '{{printf "%s, %s, %s", $1, $2, $3}}' /proc/loadavg 2>/dev/null)
if [ -z "$load" ]; then
  load=$(uptime 2>/dev/null | sed -n 's/.*load averages\\{{0,1\\}}: *//p; s/.*load average: *//p' | awk -F',' 'NF>=3 {{gsub(/^ +| +$/, "", $1); gsub(/^ +| +$/, "", $2); gsub(/^ +| +$/, "", $3); printf "%s, %s, %s", $1, $2, $3; exit}}')
fi
mem_bytes=$(awk 'BEGIN {{ total=available=memfree=buffers=cached=shmem=anonpages=sreclaimable=slab=kernelstack=pagetables=0 }}
  /^MemTotal:/ {{ total=$2 * 1024 }}
  /^MemAvailable:/ {{ available=$2 * 1024 }}
  /^MemFree:/ {{ memfree=$2 * 1024 }}
  /^Buffers:/ {{ buffers=$2 * 1024 }}
  /^Cached:/ {{ cached=$2 * 1024 }}
  /^Shmem:/ {{ shmem=$2 * 1024 }}
  /^AnonPages:/ {{ anonpages=$2 * 1024 }}
  /^SReclaimable:/ {{ sreclaimable=$2 * 1024 }}
  /^Slab:/ {{ slab=$2 * 1024 }}
  /^KernelStack:/ {{ kernelstack=$2 * 1024 }}
  /^PageTables:/ {{ pagetables=$2 * 1024 }}
  END {{
    if (available == 0) available=memfree+buffers+cached+sreclaimable-shmem
    if (available < 0) available=0
    if (total > 0) {{
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
    }}
  }}' /proc/meminfo 2>/dev/null)
if [ -z "$mem_bytes" ]; then
  mem_bytes=$(free 2>/dev/null | awk '/^Mem:/ {{
    total=$2 * 1024
    used=$3 * 1024
    available=$7 * 1024
    if (available == 0) available=total-used
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%.0f|%.0f|%.0f|%d|0|0|0", used, total, available, percent
    exit
  }}')
fi
mem=$(printf "%s" "$mem_bytes" | awk -F'|' 'NF >= 4 {{printf "%d|%d|%d|%d|%d|%d", $1/1024/1024, $2/1024/1024, $4, $5/1024/1024, $6/1024/1024, $7/1024/1024}}')
swap_bytes=$(awk 'BEGIN {{ total=free=0 }}
  /^SwapTotal:/ {{ total=$2 * 1024 }}
  /^SwapFree:/ {{ free=$2 * 1024 }}
  END {{
    used=total-free
    if (used < 0) used=0
    available=free
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%.0f|%.0f|%.0f|%d", used, total, available, percent
  }}' /proc/meminfo 2>/dev/null)
if [ -z "$swap_bytes" ]; then
  swap_bytes=$(free 2>/dev/null | awk '/^Swap:/ {{
    total=$2 * 1024
    used=$3 * 1024
    available=total-used
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%.0f|%.0f|%.0f|%d", used, total, available, percent
    exit
  }}')
fi
swap=$(printf "%s" "$swap_bytes" | awk -F'|' 'NF >= 4 {{printf "%d|%d|%d", $1/1024/1024, $2/1024/1024, $4}}')
logical_cpu_count=$(getconf _NPROCESSORS_ONLN 2>/dev/null)
case "$logical_cpu_count" in
  ''|*[!0-9]*|0) logical_cpu_count=$(nproc 2>/dev/null) ;;
esac
case "$logical_cpu_count" in
  ''|*[!0-9]*|0) logical_cpu_count=$(awk '/^processor[[:space:]]*:/ {{ count++ }} END {{ print count + 0 }}' /proc/cpuinfo 2>/dev/null) ;;
esac
cpu_info=$(awk -F: -v logical_cpu_count="$logical_cpu_count" '
  /^model name[[:space:]]*:/ || /^Hardware[[:space:]]*:/ || /^Processor[[:space:]]*:/ {{
    current=$2
    sub(/^[[:space:]]+/, "", current)
    if (current != "") {{
      model_order[++model_count]=current
      model_occurrences[current]++
      if (!seen[current]++) unique_model_count++
    }}
  }}
  /^cpu cores[[:space:]]*:/ {{
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (cores[current] == "") cores[current]=value
  }}
  /^cpu MHz[[:space:]]*:/ || /^BogoMIPS[[:space:]]*:/ {{
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (mhz[current] == "") mhz[current]=sprintf("%.3f", value + 0)
  }}
  /^cache size[[:space:]]*:/ {{
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (cache[current] == "") cache[current]=value
  }}
  /^bogomips[[:space:]]*:/ || /^BogoMIPS[[:space:]]*:/ {{
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (bogomips[current] == "") bogomips[current]=value
  }}
  END {{
    for (row_index = 1; row_index <= model_count; row_index++) {{
      model=model_order[row_index]
      if (printed[model]) continue
      printed[model]=1
      resolved_cores=model_occurrences[model] + 0
      if (unique_model_count == 1 && logical_cpu_count + 0 > resolved_cores) resolved_cores=logical_cpu_count + 0
      if (resolved_cores == 0 && cores[model] != "") resolved_cores=cores[model] + 0
      printf "%s|%s|%s|%s|%s\n", model, resolved_cores, (mhz[model] == "" ? "-" : mhz[model]), (cache[model] == "" ? "-" : cache[model]), (bogomips[model] == "" ? "-" : bogomips[model])
    }}
  }}
' /proc/cpuinfo 2>/dev/null)
if [ -z "$cpu_info" ]; then
  cpu_info=$(LC_ALL=C lscpu 2>/dev/null | awk -F: '
    function trim(value) {{
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      return value
    }}
    /^Model name:/ {{ model=trim($2) }}
    /^Socket\\(s\\):/ {{ sockets=trim($2) + 0 }}
    /^Core\\(s\\) per socket:/ {{ cores_per_socket=trim($2) + 0 }}
    /^CPU\\(s\\):/ && total_cores == 0 {{ total_cores=trim($2) + 0 }}
    /^CPU max MHz:/ {{ frequency=trim($2) }}
    /^CPU MHz:/ && frequency == "" {{ frequency=trim($2) }}
    /^L3 cache:/ {{ cache=trim($2) }}
    /^L2 cache:/ && cache == "" {{ cache=trim($2) }}
    /^BogoMIPS:/ {{ bogomips=trim($2) }}
    END {{
      if (total_cores == 0 && sockets > 0 && cores_per_socket > 0) total_cores=sockets * cores_per_socket
      if (model != "") printf "%s|%s|%s|%s|%s\n", model, (total_cores > 0 ? total_cores : 0), (frequency == "" ? "-" : sprintf("%.3f", frequency + 0)), (cache == "" ? "-" : cache), (bogomips == "" ? "-" : bogomips)
    }}
  ')
fi
: > "$gpu_info_file"
nvidia_gpu_info=$(run_bounded 1 nvidia-smi --query-gpu=name,driver_version,memory.total,utilization.gpu,memory.used,temperature.gpu,power.draw,power.limit --format=csv,noheader,nounits 2>/dev/null | awk -F',' '
  function trim(value) {{
    sub(/^[[:space:]]+/, "", value)
    sub(/[[:space:]]+$/, "", value)
    return value
  }}
  function with_unit(value, unit) {{
    value=trim(value)
    return (value == "" || value == "-") ? "-" : value " " unit
  }}
  NF >= 3 {{
    model=trim($1)
    driver=trim($2)
    memory_total=trim($3)
    gpu_usage=trim($4)
    memory_used=trim($5)
    temperature=trim($6)
    power_usage=trim($7)
    power_limit=trim($8)
    printf "%s|NVIDIA|%s|%s|%s|%s|%s|%s|%s\n", model, (driver == "" ? "-" : driver), with_unit(memory_total, "MiB"), with_unit(gpu_usage, "%"), with_unit(memory_used, "MiB"), with_unit(temperature, "C"), with_unit(power_usage, "W"), with_unit(power_limit, "W")
  }}
')
if [ -n "$nvidia_gpu_info" ]; then
  printf "%s\n" "$nvidia_gpu_info" >> "$gpu_info_file"
fi

format_gpu_sysfs_bytes() {{
  value="$1"
  case "$value" in
    ''|*[!0-9]*) printf "%s" "-" ;;
    *) awk -v bytes="$value" 'BEGIN {{ if (bytes > 0) printf "%.0f MiB", bytes / 1024 / 1024; else print "-" }}' ;;
  esac
}}

format_gpu_microwatts() {{
  value="$1"
  case "$value" in
    ''|*[!0-9]*) printf "%s" "-" ;;
    *) awk -v microwatts="$value" 'BEGIN {{ if (microwatts > 0) printf "%.1f W", microwatts / 1000000; else print "-" }}' ;;
  esac
}}

read_gpu_memory_value() {{
  card="$1"
  field="$2"
  for candidate in \
    "$card/device/$field" \
    "$card/device/tile0/$field" \
    "$card/device/gt/gt0/$field"; do
    [ -r "$candidate" ] || continue
    raw=$(cat "$candidate" 2>/dev/null)
    case "$raw" in
      ''|*[!0-9]*) continue ;;
    esac
    formatted=$(format_gpu_sysfs_bytes "$raw")
    [ "$formatted" != "-" ] && {{
      printf "%s" "$formatted"
      return
    }}
  done
  printf "%s" "-"
}}

read_gpu_temperature_value() {{
  card="$1"
  for hwmon in "$card"/device/hwmon/hwmon*; do
    [ -r "$hwmon/temp1_input" ] || continue
    raw=$(cat "$hwmon/temp1_input" 2>/dev/null)
    case "$raw" in
      ''|*[!0-9-]*) continue ;;
    esac
    formatted=$(awk -v millidegrees="$raw" 'BEGIN {{ c=millidegrees / 1000; if (c > -50 && c < 150) printf "%.1f C", c }}')
    [ -n "$formatted" ] && {{
      printf "%s" "$formatted"
      return
    }}
  done
  printf "%s" "-"
}}

read_gpu_power_value() {{
  card="$1"
  mode="$2"
  for hwmon in "$card"/device/hwmon/hwmon*; do
    [ -d "$hwmon" ] || continue
    if [ "$mode" = "limit" ]; then
      power_files="power1_cap power1_max"
    else
      power_files="power1_average power1_input"
    fi
    for power_name in $power_files; do
      power_file="$hwmon/$power_name"
      [ -r "$power_file" ] || continue
      raw=$(cat "$power_file" 2>/dev/null)
      formatted=$(format_gpu_microwatts "$raw")
      [ "$formatted" != "-" ] && {{
        printf "%s" "$formatted"
        return
      }}
    done
  done
  printf "%s" "-"
}}

read_intel_gpu_usage() {{
  card="$1"
  card_number=$(printf "%s\n" "$card" | sed 's#.*/card##')
  if ! command -v intel_gpu_top >/dev/null 2>&1; then
    printf "%s" "-"
    return
  fi
  usage=$(run_bounded 2 intel_gpu_top -J -s 1000 -o - -d "drm:/dev/dri/card$card_number" 2>/dev/null | awk '
    {{
      line=$0
      while (match(line, /"busy"[[:space:]]*:[[:space:]]*[0-9]+([.][0-9]+)?/)) {{
        token=substr(line, RSTART, RLENGTH)
        sub(/^.*:[[:space:]]*/, "", token)
        value=token + 0
        if (value > maximum) maximum=value
        seen=1
        line=substr(line, RSTART + RLENGTH)
      }}
    }}
    END {{
      if (seen) {{
        if (maximum < 0) maximum=0
        if (maximum > 100) maximum=100
        printf "%.1f", maximum
      }}
    }}
  ')
  [ -n "$usage" ] && printf "%s%%" "$usage" || printf "%s" "-"
}}

# Linux DRM exposes vendor-independent card directories. AMD's amdgpu and
# Intel's i915/xe drivers expose busy, VRAM, hwmon temperature and power
# values there when the kernel/driver supports them. Keep NVIDIA rows from
# nvidia-smi and only use this path for NVIDIA when that query was unavailable.
for card in /sys/class/drm/card*; do
  [ -d "$card/device" ] || continue
  card_name=$(printf "%s\n" "$card" | sed 's#.*/card##')
  card_number="$card_name"
  case "$card_number" in
    ''|*[!0-9]*) continue ;;
  esac
  [ -r "$card/device/vendor" ] || continue
  vendor_id=$(cat "$card/device/vendor" 2>/dev/null)
  case "$vendor_id" in
    0x1002|0X1002) vendor="AMD" ;;
    0x8086|0X8086) vendor="Intel" ;;
    0x10de|0X10DE) vendor="NVIDIA" ;;
    *) vendor="-" ;;
  esac
  if [ "$vendor" = "NVIDIA" ] && [ -n "$nvidia_gpu_info" ]; then
    continue
  fi
  slot=$(sed -n 's/^PCI_SLOT_NAME=//p' "$card/device/uevent" 2>/dev/null | head -n 1)
  gpu_line=$(lspci -s "$slot" 2>/dev/null | head -n 1)
  model=$(printf "%s\n" "$gpu_line" | awk '
    {{
      line=$0
      sub(/^[[:xdigit:]:.]+[[:space:]]+[^:]+:[[:space:]]*/, "", line)
      sub(/[[:space:]]+\[[[:xdigit:]:]+\]$/, "", line)
      print line
      exit
    }}
  ')
  [ -n "$model" ] || model="$vendor GPU"
  driver=$(readlink "$card/device/driver" 2>/dev/null | sed 's#.*/##')
  [ -n "$driver" ] || driver="-"

  gpu_usage="-"
  if [ -r "$card/device/gpu_busy_percent" ]; then
    raw_usage=$(cat "$card/device/gpu_busy_percent" 2>/dev/null)
    case "$raw_usage" in
      ''|*[!0-9.]*) ;;
      *) gpu_usage=$(awk -v value="$raw_usage" 'BEGIN {{ if (value < 0) value=0; if (value > 100) value=100; printf "%.1f%%", value }}') ;;
    esac
  elif [ "$vendor" = "Intel" ]; then
    gpu_usage=$(read_intel_gpu_usage "$card")
  fi
  gpu_memory=$(read_gpu_memory_value "$card" "mem_info_vram_total")
  gpu_memory_used=$(read_gpu_memory_value "$card" "mem_info_vram_used")
  gpu_temperature=$(read_gpu_temperature_value "$card")
  gpu_power=$(read_gpu_power_value "$card" "current")
  gpu_power_limit=$(read_gpu_power_value "$card" "limit")
  printf "%s|%s|%s|%s|%s|%s|%s|%s|%s\n" \
    "$model" "$vendor" "$driver" "$gpu_memory" "$gpu_usage" "$gpu_memory_used" \
    "$gpu_temperature" "$gpu_power" "$gpu_power_limit" >> "$gpu_info_file"
done

if [ ! -s "$gpu_info_file" ]; then
  # Last-resort hardware discovery for systems without DRM sysfs.
  run_bounded 1 lspci 2>/dev/null | awk '
    BEGIN {{ IGNORECASE=1 }}
    /VGA compatible controller|3D controller|Display controller/ {{
      line=$0
      sub(/^[[:xdigit:]:.]+[[:space:]]+[^:]+: /, "", line)
      vendor=line
      sub(/[[:space:]].*$/, "", vendor)
      printf "%s|%s|-|-|-|-|-|-|-\n", line, (vendor == "" ? "-" : vendor)
    }}
  ' >> "$gpu_info_file"
fi
gpu_info=$(cat "$gpu_info_file" 2>/dev/null)
ifaces=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); if (name != "lo") {{ if (out != "") out=out ","; out=out name }}}} END {{print out}}' /proc/net/dev 2>/dev/null)
active_iface=$(awk '$2 == 00000000 {{print $1; exit}}' /proc/net/route 2>/dev/null)
[ -z "$active_iface" ] && active_iface=$(echo "$ifaces" | awk -F, '{{print $1}}')
rx1=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[2]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
tx1=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[10]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
before_file="/tmp/fileterm-if-before-$$"
after_file="/tmp/fileterm-if-after-$$"
awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") printf "%s|%.0f|%.0f\n", name, values[2], values[10]}}' /proc/net/dev 2>/dev/null > "$before_file"
sleep "$sleep_interval"
rx2=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[2]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
tx2=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[10]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") printf "%s|%.0f|%.0f\n", name, values[2], values[10]}}' /proc/net/dev 2>/dev/null > "$after_file"
sample_ms=$(awk -v interval="$sleep_interval" 'BEGIN {{ printf "%d", interval * 1000 }}')
[ -z "$sample_ms" ] && sample_ms=1000
rx_rate=$(awk -v before="$rx1" -v after="$rx2" -v ms="$sample_ms" 'BEGIN {{ if (ms > 0) printf "%d", (after-before) * 1000 / ms; else print 0 }}')
tx_rate=$(awk -v before="$tx1" -v after="$tx2" -v ms="$sample_ms" 'BEGIN {{ if (ms > 0) printf "%d", (after-before) * 1000 / ms; else print 0 }}')
df_flags="-kP"
df -kPl / >/dev/null 2>&1 && df_flags="-kPl"
if has_bounded_runner; then
  df_output=$(run_bounded 2 df "$df_flags" 2>/dev/null)
else
  local_mounts=$(awk '
    $3 ~ /^(overlay|squashfs|tmpfs|ramfs|ext[234]|xfs|btrfs|f2fs|vfat|ubifs|jffs2|zfs)$/ && !seen[$2]++ {{ print $2 }}
  ' /proc/mounts 2>/dev/null | head -n 20)
  [ -z "$local_mounts" ] && local_mounts="/"
  df_output=$(df "$df_flags" $local_mounts 2>/dev/null)
fi
disk=$(printf "%s\n" "$df_output" | awk 'NR>1 {{printf "%s|%sK/%sK\n", $6, $4, $2}}' | head -n 12)
filesystems=$(printf "%s\n" "$df_output" | awk 'NR>1 {{printf "%s|%sK|%sK|%s|%sK|%s\n", $1, $2, $3, $5, $4, $6}}' | head -n 20)
# 进程采集：按两次 /proc/<pid>/stat 采样计算窗口内的瞬时 CPU 占用。
# ps 的 %CPU 是进程生命周期平均值，长时间运行的进程在突然升高时会严重漏报；
# 同时它按单核百分比返回，多核机器还会出现与总 CPU 仪表不一致的问题。
# 这里用进程 tick 增量 / 全局 CPU tick 增量，直接得到 0-100 的整机占比。
if [ -s "$process_ticks_before_file" ] && [ -s "$process_ticks_after_file" ] && [ "$diff_total" -gt 0 ]; then
  # A process delta larger than the global delta means the two /proc snapshots
  # are not comparable (PID reuse, a broken proc reader, or a too-short sample).
  # Reject the whole tick sample and use ps below rather than emitting values
  # such as thousands of percent.
  if awk -F'|' -v diff_total="$diff_total" '
    NR==FNR {{ before[$1]=$2; next }}
    {{
      if (!($1 in before)) next
      delta=$2-before[$1]
      if (delta < 0 || delta > diff_total) {{ invalid=1; next }}
      # Keep processes that consumed no CPU during this short sample too.
      # Filtering them out makes the top-process list randomly shrink to two
      # or three rows whenever fewer processes receive a tick in the window.
      printf "%s|%.4f\n", $1, delta * 100 / diff_total
      matched++
    }}
    END {{ if (matched == 0 || invalid) exit 1 }}
  ' "$process_ticks_before_file" "$process_ticks_after_file" > "$process_cpu_tmp_file"; then
    mv "$process_cpu_tmp_file" "$process_cpu_file"
  else
    rm -f "$process_cpu_tmp_file" "$process_cpu_file"
  fi
fi

if [ -s "$process_cpu_file" ]; then
  procs=$(ps -eo pid=,user=,rss=,pmem=,args= 2>/dev/null | awk -v cpu_file="$process_cpu_file" '
    BEGIN {{
      while ((getline line < cpu_file) > 0) {{
        split(line, values, "|")
        cpu[values[1]]=values[2] + 0
      }}
      close(cpu_file)
    }}
    NF >= 5 {{
      pid=$1
      if (!(pid in cpu)) next
      args=$5
      for (i=6; i<=NF; i++) args=args" "$i
      command_name=args
      sub(/^[[:space:]]*/, "", command_name)
      split(command_name, command_parts, /[[:space:]]+/)
      comm=command_parts[1]
      sub(/^.*\//, "", comm)
      if (comm == "ps" || comm == "awk" || comm == "bash" || comm == "sleep" || comm == "sh" || comm == "powershell" || comm == "pwsh") next
      if (cpu[pid] < 0 || cpu[pid] > 100) next
      row_count++
      scores[row_count]=cpu[pid]
      rows[row_count]=sprintf("%s|%s|%.1fM|%.1f|%s|%s", pid, $2, $3/1024, cpu[pid], $4, substr(args, 1, 200))
    }}
    END {{
      for (rank=1; rank<=40 && rank<=row_count; rank++) {{
        best=0
        best_score=-1
        for (i=1; i<=row_count; i++) {{
          if (!used[i] && scores[i] > best_score) {{
            best=i
            best_score=scores[i]
          }}
        }}
        if (best == 0) break
        print rows[best]
        used[best]=1
      }}
    }}
  ')
else
  # fallback：无法读取 /proc 进程 tick 或快照校验失败时，使用 ps 的
  # 生命周期平均值。ps 的 %CPU 按单核百分比返回，这里归一化到整机 0-100。
  if has_bounded_runner; then
    procs=$(run_bounded 1 ps -eo pid=,user=,rss=,pcpu=,pmem=,args= --sort=-pcpu 2>/dev/null | head -n 40 | awk -v logical_cpu_count="$logical_cpu_count" 'NF >= 6 {{rss=$3/1024; args=$6; for(i=7;i<=NF;i++) args=args" "$i; cpu=$4+0; if (logical_cpu_count + 0 > 0) cpu=cpu/logical_cpu_count; if (cpu < 0) cpu=0; if (cpu > 100) cpu=100; printf "%s|%s|%.1fM|%.1f|%s|%s\n", $1, $2, rss, cpu, $5, substr(args,1,200)}}')
  else
    procs=$(ps -eo pid=,user=,rss=,pcpu=,pmem=,args= --sort=-pcpu 2>/dev/null | head -n 40 | awk -v logical_cpu_count="$logical_cpu_count" 'NF >= 6 {{rss=$3/1024; args=$6; for(i=7;i<=NF;i++) args=args" "$i; cpu=$4+0; if (logical_cpu_count + 0 > 0) cpu=cpu/logical_cpu_count; if (cpu < 0) cpu=0; if (cpu > 100) cpu=100; printf "%s|%s|%.1fM|%.1f|%s|%s\n", $1, $2, rss, cpu, $5, substr(args,1,200)}}')
  fi
fi
if [ -z "$procs" ]; then
  # fallback：极简 ps（如某些 BusyBox 不支持 --sort 或 -o args=）
  if has_bounded_runner; then
    procs=$(run_bounded 1 ps 2>/dev/null | head -n 40 | awk 'NR>1 && NF >= 5 {{printf "0|-|%.1fM|0|0|%s\n", $3/1024, $5}}')
  else
    procs=$(ps 2>/dev/null | head -n 40 | awk 'NR>1 && NF >= 5 {{printf "0|-|%.1fM|0|0|%s\n", $3/1024, $5}}')
  fi
fi
echo "__PLATFORM__{}"
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
  NR==FNR {{rx[$1]=$2; tx[$1]=$3; next}}
  NF >= 3 {{
    prev_rx=rx[$1]
    prev_tx=tx[$1]
    curr_rx=$2
    curr_tx=$3
    rx_rate=(curr_rx-prev_rx) * 1000 / sample_ms
    tx_rate=(curr_tx-prev_tx) * 1000 / sample_ms
    printf "%s|%.0f|%.0f|%d|%d\n", $1, curr_rx, curr_tx, rx_rate, tx_rate
  }}
' "$before_file" "$after_file"
rm -f "$before_file" "$after_file" "$process_ticks_before_file" "$process_ticks_after_file" "$process_cpu_file" "$process_cpu_tmp_file" "$gpu_info_file"
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
echo "{}"
"#,
        platform, complete_marker
    )
}
