/// FreeBSD metrics collector.
///
/// FreeBSD does not expose the Linux /proc/stat and /proc/meminfo contract.
/// Use documented base-system interfaces instead: sysctl for kernel and VM
/// data, swapinfo for swap, df for filesystems, and ps for processes. The
/// emitted markers intentionally match the POSIX parser so the renderer does
/// not need a second metrics protocol.
pub fn build_freebsd_metrics_command() -> String {
    r#"cd / >/dev/null 2>&1 || true
sleep_interval="0.15"

read_number() {
  case "$1" in
    ''|*[!0-9]*) printf '0' ;;
    *) printf '%s' "$1" ;;
  esac
}

read_sysctl() {
  sysctl -n "$1" 2>/dev/null | head -n 1
}

# `sysctl -n` may surround array values with braces on some FreeBSD builds.
# Keep only the five numeric CPU-state counters before positional expansion.
set -- $(sysctl -n kern.cp_time 2>/dev/null | tr -cd '0-9 \n')
cpu_user_before=$(read_number "$1")
cpu_nice_before=$(read_number "$2")
cpu_system_before=$(read_number "$3")
cpu_irq_before=$(read_number "$4")
cpu_idle_before=$(read_number "$5")
cpu_total_before=$((cpu_user_before+cpu_nice_before+cpu_system_before+cpu_irq_before+cpu_idle_before))

sleep "$sleep_interval" >/dev/null 2>&1 || sleep 1

set -- $(sysctl -n kern.cp_time 2>/dev/null | tr -cd '0-9 \n')
cpu_user_after=$(read_number "$1")
cpu_nice_after=$(read_number "$2")
cpu_system_after=$(read_number "$3")
cpu_irq_after=$(read_number "$4")
cpu_idle_after=$(read_number "$5")
cpu_total_after=$((cpu_user_after+cpu_nice_after+cpu_system_after+cpu_irq_after+cpu_idle_after))
cpu_diff_total=$((cpu_total_after-cpu_total_before))
cpu_diff_idle=$((cpu_idle_after-cpu_idle_before))

cpu_pct=$(awk -v total="$cpu_diff_total" -v idle="$cpu_diff_idle" 'BEGIN {
  if (total <= 0) { print "0.0"; exit }
  value=(total-idle)*100/total
  if (value < 0) value=0
  if (value > 100) value=100
  printf "%.1f", value
}')
cpu_usage=$(awk -v total="$cpu_diff_total" \
  -v user_before="$cpu_user_before" -v user_after="$cpu_user_after" \
  -v system_before="$cpu_system_before" -v system_after="$cpu_system_after" \
  -v nice_before="$cpu_nice_before" -v nice_after="$cpu_nice_after" \
  -v idle_before="$cpu_idle_before" -v idle_after="$cpu_idle_after" \
  -v irq_before="$cpu_irq_before" -v irq_after="$cpu_irq_after" 'function pct(before, after) {
  value=(after-before)*100/total
  if (value < 0) value=0
  if (value > 100) value=100
  return value
}
BEGIN {
  if (total <= 0) { print "0.0|0.0|0.0|0.0|0.0|0.0|0.0|0.0"; exit }
  printf "%.1f|%.1f|%.1f|%.1f|0.0|%.1f|0.0|0.0",
    pct(user_before, user_after),
    pct(system_before, system_after),
    pct(nice_before, nice_after),
    pct(idle_before, idle_after),
    pct(irq_before, irq_after)
}')

freebsd_version=$(freebsd-version 2>/dev/null | head -n 1)
[ -z "$freebsd_version" ] && freebsd_version=$(uname -r 2>/dev/null)
os_name="FreeBSD"
[ -n "$freebsd_version" ] && os_name="$os_name $freebsd_version"
kernel_name=$(uname -s 2>/dev/null)
kernel_version=$(uname -r 2>/dev/null)
architecture=$(uname -m 2>/dev/null)
hostname_value=$(hostname 2>/dev/null | head -n 1)

boot_seconds=$(sysctl -n kern.boottime 2>/dev/null | awk -F '[ =,{}]+' '{
  for (i=1; i<=NF; i++) if ($i == "sec") { print $(i+1); exit }
}')
boot_seconds=$(read_number "$boot_seconds")
now_seconds=$(date +%s 2>/dev/null)
now_seconds=$(read_number "$now_seconds")
uptime_seconds=0
if [ "$boot_seconds" -gt 0 ] && [ "$now_seconds" -ge "$boot_seconds" ]; then
  uptime_seconds=$((now_seconds-boot_seconds))
fi
load=$(sysctl -n vm.loadavg 2>/dev/null | tr -d '{}(),' | awk 'NF >= 3 { printf "%s, %s, %s", $1, $2, $3 }')

page_size=$(read_number "$(read_sysctl vm.stats.vm.v_page_size)")
page_count=$(read_number "$(read_sysctl vm.stats.vm.v_page_count)")
free_pages=$(read_number "$(read_sysctl vm.stats.vm.v_free_count)")
cache_pages=$(read_number "$(read_sysctl vm.stats.vm.v_cache_count)")
inactive_pages=$(read_number "$(read_sysctl vm.stats.vm.v_inactive_count)")
total_bytes=$(read_number "$(read_sysctl hw.physmem)")
available_bytes=$(((free_pages+cache_pages+inactive_pages)*page_size))
if [ "$total_bytes" -le 0 ] && [ "$page_size" -gt 0 ] && [ "$page_count" -gt 0 ]; then
  total_bytes=$((page_size*page_count))
fi
if [ "$total_bytes" -le 0 ]; then
  total_bytes=$(read_number "$(read_sysctl hw.usermem)")
fi
if [ "$available_bytes" -le 0 ]; then
  available_bytes=$(read_number "$(read_sysctl hw.usermem)")
fi
if [ "$total_bytes" -gt 0 ] && [ "$available_bytes" -gt "$total_bytes" ]; then
  available_bytes=$total_bytes
fi
used_bytes=$((total_bytes-available_bytes))
if [ "$used_bytes" -lt 0 ]; then used_bytes=0; fi
mem_percent=$(awk -v used="$used_bytes" -v total="$total_bytes" 'BEGIN {
  if (total > 0) printf "%.0f", used*100/total; else print "0"
}')
mem=$(awk -v used="$used_bytes" -v total="$total_bytes" -v percent="$mem_percent" 'BEGIN {
  printf "%.1f|%.1f|%s|0|0|0", used/1048576, total/1048576, percent
}')
mem_bytes="$used_bytes|$total_bytes|$available_bytes|$mem_percent|0|0|0"

swap_bytes=$(swapinfo -k 2>/dev/null | awk 'NR > 1 && $2 ~ /^[0-9]+$/ && $3 ~ /^[0-9]+$/ && $4 ~ /^[0-9]+$/ {
  total += $2*1024
  used += $3*1024
  available += $4*1024
}
END {
  if (total > 0) printf "%.0f|%.0f|%.0f|%.1f", used, total, available, used*100/total
  else print "0|0|0|0"
}')
swap_used_bytes=$(printf '%s' "$swap_bytes" | cut -d'|' -f1)
swap_total_bytes=$(printf '%s' "$swap_bytes" | cut -d'|' -f2)
swap_available_bytes=$(printf '%s' "$swap_bytes" | cut -d'|' -f3)
swap_percent=$(printf '%s' "$swap_bytes" | cut -d'|' -f4)
swap=$(awk -v used="$swap_used_bytes" -v total="$swap_total_bytes" -v percent="$swap_percent" 'BEGIN {
  printf "%.1f|%.1f|%s", used/1048576, total/1048576, percent
}')

cpu_model=$(read_sysctl hw.model)
[ -z "$cpu_model" ] && cpu_model="FreeBSD CPU"
cpu_cores=$(read_number "$(read_sysctl hw.ncpu)")
cpu_frequency=$(read_number "$(read_sysctl hw.clockrate)")
cpu_info="$cpu_model|$cpu_cores|$cpu_frequency|-|-"

ifaces=$(ifconfig -l 2>/dev/null | awk '{
  for (i=1; i<=NF; i++) if ($i != "lo0") {
    if (out != "") out=out ","
    out=out $i
  }
} END { print out }')
active_iface=$(printf '%s\n' "$ifaces" | awk -F, '{ print $1 }')
ip=""
if [ -n "$active_iface" ]; then
  ip=$(ifconfig "$active_iface" 2>/dev/null | awk '$1 == "inet" && $2 !~ /^127\./ { print $2; exit }')
fi

df_output=$(df -kP 2>/dev/null)
disk=$(printf '%s\n' "$df_output" | awk 'NR > 1 && $2 ~ /^[0-9]+$/ {
  mount=$6
  for (i=7; i<=NF; i++) mount=mount " " $i
  printf "%s|%sK/%sK\n", mount, $4, $2
}')
filesystems=$(printf '%s\n' "$df_output" | awk 'NR > 1 && $2 ~ /^[0-9]+$/ {
  mount=$6
  for (i=7; i<=NF; i++) mount=mount " " $i
  printf "%s|%sK|%sK|%s|%sK|%s\n", $1, $2, $3, $5, $4, mount
}')

procs=$(ps -axo pid=,user=,rss=,pcpu=,pmem=,command= 2>/dev/null | awk -v logical_cpu_count="$cpu_cores" '
NF >= 6 {
  pid=$1
  user=$2
  rss=$3+0
  cpu=$4+0
  mem=$5+0
  args=$6
  for (i=7; i<=NF; i++) args=args " " $i
  gsub(/\|/, " ", args)
  gsub(/\r/, "", args)
  comm=args
  sub(/^.*\//, "", comm)
  sub(/[[:space:]].*$/, "", comm)
  if (comm == "ps" || comm == "awk" || comm == "sh" || comm == "sleep" || comm == "head") next
  if (logical_cpu_count + 0 > 0) cpu=cpu/logical_cpu_count
  if (cpu < 0) cpu=0
  if (cpu > 100) cpu=100
  printf "%s|%s|%.1fM|%.1f|%.1f|%s\n", pid, user, rss/1024, cpu, mem, substr(args, 1, 200)
}' | head -n 40)

printf '%s\n' "__PLATFORM__freebsd"
printf '%s\n' "__OS__$os_name"
printf '%s\n' "__KERNEL_NAME__$kernel_name"
printf '%s\n' "__KERNEL_VERSION__$kernel_version"
printf '%s\n' "__ARCH__$architecture"
printf '%s\n' "__HOSTNAME__$hostname_value"
printf '%s\n' "__IP__$ip"
printf '%s\n' "__UPTIME__"
printf '%s\n' "__UPTIME_SECONDS__$uptime_seconds"
printf '%s\n' "__LOAD__$load"
printf '%s\n' "__CPU__$cpu_pct"
printf '%s\n' "__CPU_USAGE__$cpu_usage"
printf '%s\n' "__MEM__$mem"
printf '%s\n' "__MEM_BYTES__$mem_bytes"
printf '%s\n' "__SWAP__$swap"
printf '%s\n' "__SWAP_BYTES__$swap_bytes"
printf '%s\n' "__CPUINFO_START__"
printf '%s\n' "$cpu_info"
printf '%s\n' "__CPUINFO_END__"
printf '%s\n' "__GPUINFO_START__"
printf '%s\n' "__GPUINFO_END__"
printf '%s\n' "__IFACES__$ifaces"
printf '%s\n' "__ACTIVE_IFACE__$active_iface"
printf '%s\n' "__RATES__0|0"
printf '%s\n' "__IFACE_RATES_START__"
printf '%s\n' "__IFACE_RATES_END__"
printf '%s\n' "__DISK_START__"
printf '%s\n' "$disk"
printf '%s\n' "__DISK_END__"
printf '%s\n' "__FILESYSTEMS_START__"
printf '%s\n' "$filesystems"
printf '%s\n' "__FILESYSTEMS_END__"
printf '%s\n' "__PROCS_START__"
printf '%s\n' "$procs"
printf '%s\n' "__PROCS_END__"
printf '%s\n' "__FILETERM_METRICS_COMPLETE__"


"#.to_string()
}
