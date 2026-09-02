/// FreeBSD metrics collector.
///
/// FreeBSD does not expose the Linux /proc/stat and /proc/meminfo contract.
/// Use documented base-system interfaces instead: sysctl for kernel and VM
/// data, swapinfo for host swap, df for host filesystems, and ps for processes.
/// When available, rctl/quota provide the logged-in account's scoped usage and
/// limits. Hosted providers may add a provider-specific account command (for
/// example, Serv00's `devil info limits`). The emitted markers intentionally
/// match the POSIX parser so the renderer does not need a second protocol.
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

format_quota_bytes() {
  awk -v bytes="$1" 'BEGIN {
    if (bytes >= 1099511627776) printf "%.1fT", bytes/1099511627776
    else if (bytes >= 1073741824) printf "%.1fG", bytes/1073741824
    else if (bytes >= 1048576) printf "%.1fM", bytes/1048576
    else if (bytes >= 1024) printf "%.1fK", bytes/1024
    else printf "%.0fB", bytes
  }'
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

# FreeBSD's standard account-level interfaces are split by responsibility:
# `rctl -u` reports current resource usage, `rctl -l` reports applicable rules,
# and `quota -v -f` reports per-user filesystem quota in 1024-byte blocks.
# These commands are optional (RACCT/RCTL and filesystem quotas are not enabled
# everywhere), so every result is used only when a complete used/limit pair is
# available. This prevents a scoped value from being paired with host capacity.
current_user=$(id -un 2>/dev/null)
rctl_usage=""
rctl_rules=""
quota_output=""
if [ -n "$current_user" ] && command -v rctl >/dev/null 2>&1; then
  rctl_usage=$(rctl -u "user:$current_user" 2>/dev/null)
  rctl_rules=$(rctl -l "user:$current_user" 2>/dev/null)
fi
if command -v quota >/dev/null 2>&1; then
  quota_output=$(quota -v -f "${HOME:-/}" 2>/dev/null)
fi

rctl_usage_values=$(printf '%s\n' "$rctl_usage" | awk '
function metric_amount(raw, resource, value, unit, suffix, factor) {
  value=raw
  gsub(/[[:space:]]/, "", value)
  sub(/\/.*$/, "", value)
  gsub(/,/, ".", value)
  gsub(/%$/, "", value)
  if (value == "" || value == "-") return -1
  if (resource == "pcpu" || resource == "maxproc") {
    if (value !~ /^[0-9]+([.][0-9]+)?$/) return -1
    return value + 0
  }
  unit=toupper(value)
  factor=1
  if (unit ~ /KILOBYTES?$/) { factor=1024; sub(/KILOBYTES?$/, "", unit) }
  else if (unit ~ /MEGABYTES?$/) { factor=1048576; sub(/MEGABYTES?$/, "", unit) }
  else if (unit ~ /GIGABYTES?$/) { factor=1073741824; sub(/GIGABYTES?$/, "", unit) }
  else if (unit ~ /TERABYTES?$/) { factor=1099511627776; sub(/TERABYTES?$/, "", unit) }
  else if (unit ~ /PETABYTES?$/) { factor=1125899906842624; sub(/PETABYTES?$/, "", unit) }
  else if (unit ~ /BYTES?$/) { sub(/BYTES?$/, "", unit) }
  else {
    suffix=substr(unit, length(unit), 1)
    if (suffix == "B") {
      unit=substr(unit, 1, length(unit)-1)
      suffix=substr(unit, length(unit), 1)
    }
    if (suffix == "I") {
      unit=substr(unit, 1, length(unit)-1)
      suffix=substr(unit, length(unit), 1)
    }
    if (suffix == "K") { factor=1024; unit=substr(unit, 1, length(unit)-1) }
    else if (suffix == "M") { factor=1048576; unit=substr(unit, 1, length(unit)-1) }
    else if (suffix == "G") { factor=1073741824; unit=substr(unit, 1, length(unit)-1) }
    else if (suffix == "T") { factor=1099511627776; unit=substr(unit, 1, length(unit)-1) }
    else if (suffix == "P") { factor=1125899906842624; unit=substr(unit, 1, length(unit)-1) }
  }
  if (unit !~ /^[0-9]+([.][0-9]+)?$/) return -1
  return (unit + 0) * factor
}
function print_metric(resource, value) {
  if (resource == "pcpu") printf "%s=%.6f\n", resource, value
  else printf "%s=%.0f\n", resource, value
}
{
  equals=index($0, "=")
  if (equals == 0) next
  prefix=substr($0, 1, equals-1)
  count=split(prefix, parts, ":")
  resource=parts[count]
  if (resource != "memoryuse" && resource != "pcpu" && resource != "swapuse" && resource != "maxproc") next
  value=metric_amount(substr($0, equals+1), resource)
  if (value >= 0) print_metric(resource, value)
}')
rctl_limit_values=$(printf '%s\n' "$rctl_rules" | awk '
function metric_amount(raw, resource, value, unit, suffix, factor) {
  value=raw
  gsub(/[[:space:]]/, "", value)
  sub(/\/.*$/, "", value)
  gsub(/,/, ".", value)
  gsub(/%$/, "", value)
  if (value == "" || value == "-") return -1
  if (resource == "pcpu" || resource == "maxproc") {
    if (value !~ /^[0-9]+([.][0-9]+)?$/) return -1
    return value + 0
  }
  unit=toupper(value)
  factor=1
  if (unit ~ /KILOBYTES?$/) { factor=1024; sub(/KILOBYTES?$/, "", unit) }
  else if (unit ~ /MEGABYTES?$/) { factor=1048576; sub(/MEGABYTES?$/, "", unit) }
  else if (unit ~ /GIGABYTES?$/) { factor=1073741824; sub(/GIGABYTES?$/, "", unit) }
  else if (unit ~ /TERABYTES?$/) { factor=1099511627776; sub(/TERABYTES?$/, "", unit) }
  else if (unit ~ /PETABYTES?$/) { factor=1125899906842624; sub(/PETABYTES?$/, "", unit) }
  else if (unit ~ /BYTES?$/) { sub(/BYTES?$/, "", unit) }
  else {
    suffix=substr(unit, length(unit), 1)
    if (suffix == "B") {
      unit=substr(unit, 1, length(unit)-1)
      suffix=substr(unit, length(unit), 1)
    }
    if (suffix == "I") {
      unit=substr(unit, 1, length(unit)-1)
      suffix=substr(unit, length(unit), 1)
    }
    if (suffix == "K") { factor=1024; unit=substr(unit, 1, length(unit)-1) }
    else if (suffix == "M") { factor=1048576; unit=substr(unit, 1, length(unit)-1) }
    else if (suffix == "G") { factor=1073741824; unit=substr(unit, 1, length(unit)-1) }
    else if (suffix == "T") { factor=1099511627776; unit=substr(unit, 1, length(unit)-1) }
    else if (suffix == "P") { factor=1125899906842624; unit=substr(unit, 1, length(unit)-1) }
  }
  if (unit !~ /^[0-9]+([.][0-9]+)?$/) return -1
  return (unit + 0) * factor
}
function save_limit(resource, value) {
  if (value <= 0) return
  if (!(resource in limits) || value < limits[resource]) limits[resource]=value
}
{
  equals=index($0, "=")
  if (equals == 0) next
  prefix=substr($0, 1, equals-1)
  count=split(prefix, parts, ":")
  if (count < 2 || parts[count] != "deny") next
  resource=parts[count-1]
  if (resource != "memoryuse" && resource != "pcpu" && resource != "swapuse" && resource != "maxproc") next
  raw_amount=substr($0, equals+1)
  slash=index(raw_amount, "/")
  if (slash > 0) per=substr(raw_amount, slash+1)
  else per=parts[1]
  # Pair user usage only with a rule accounted per user. A process, jail, or
  # login-class aggregate limit has a different accounting scope.
  if (per != "user") next
  value=metric_amount(raw_amount, resource)
  if (value >= 0) save_limit(resource, value)
}
END {
  if ("memoryuse" in limits) printf "memoryuse=%.0f\n", limits["memoryuse"]
  if ("pcpu" in limits) printf "pcpu=%.6f\n", limits["pcpu"]
  if ("swapuse" in limits) printf "swapuse=%.0f\n", limits["swapuse"]
  if ("maxproc" in limits) printf "maxproc=%.0f\n", limits["maxproc"]
}')
rctl_memory_used=$(printf '%s\n' "$rctl_usage_values" | awk -F= '$1 == "memoryuse" { print $2; exit }')
rctl_memory_limit=$(printf '%s\n' "$rctl_limit_values" | awk -F= '$1 == "memoryuse" { print $2; exit }')
rctl_cpu_used=$(printf '%s\n' "$rctl_usage_values" | awk -F= '$1 == "pcpu" { print $2; exit }')
rctl_cpu_limit=$(printf '%s\n' "$rctl_limit_values" | awk -F= '$1 == "pcpu" { print $2; exit }')
rctl_swap_used=$(printf '%s\n' "$rctl_usage_values" | awk -F= '$1 == "swapuse" { print $2; exit }')
rctl_swap_limit=$(printf '%s\n' "$rctl_limit_values" | awk -F= '$1 == "swapuse" { print $2; exit }')
rctl_process_limit=$(printf '%s\n' "$rctl_limit_values" | awk -F= '$1 == "maxproc" { print $2; exit }')

standard_memory=""
if [ -n "$rctl_memory_used" ] && [ -n "$rctl_memory_limit" ] && awk -v value="$rctl_memory_limit" 'BEGIN { exit !(value > 0) }'; then
  standard_memory="$rctl_memory_used|$rctl_memory_limit"
fi
standard_cpu=""
if [ -n "$rctl_cpu_used" ] && [ -n "$rctl_cpu_limit" ] && awk -v value="$rctl_cpu_limit" 'BEGIN { exit !(value > 0) }'; then
  standard_cpu="$rctl_cpu_used|$rctl_cpu_limit"
fi
standard_swap=""
if [ -n "$rctl_swap_used" ] && [ -n "$rctl_swap_limit" ] && awk -v value="$rctl_swap_limit" 'BEGIN { exit !(value > 0) }'; then
  standard_swap="$rctl_swap_used|$rctl_swap_limit"
fi
standard_process_scope=0
if [ -n "$rctl_process_limit" ] && awk -v value="$rctl_process_limit" 'BEGIN { exit !(value > 0) }'; then
  standard_process_scope=1
fi

quota_disk=$(printf '%s\n' "$quota_output" | awk '
function clean(value) {
  gsub(/[*+]/, "", value)
  gsub(/,/, "", value)
  return value
}
function valid(value) { return value ~ /^[0-9]+([.][0-9]+)?$/ }
!found {
  used=clean($2)
  soft=clean($3)
  hard=clean($4)
  if (!valid(used) || (!valid(soft) && !valid(hard))) next
  limit=valid(hard) ? hard + 0 : 0
  if (limit <= 0 && valid(soft)) limit=soft + 0
  if (limit <= 0) next
  mount=$1
  gsub(/\|/, " ", mount)
  printf "%.0f|%.0f|%s\n", (used + 0) * 1024, limit * 1024, mount
  found=1
}')

# Serv00 and similar hosted FreeBSD environments may expose an additional
# account view. It is provider-specific rather than a FreeBSD primitive, so it
# only fills gaps left by the documented rctl/quota sources below.
devil_limits=""
if command -v devil >/dev/null 2>&1; then
  devil_limits=$(devil info limits 2>/dev/null)
fi
account_limits=$(printf '%s\n' "$devil_limits" | awk '
function human_bytes(value, suffix, factor) {
  gsub(/[[:space:]]/, "", value)
  if (value == "" || value == "-") return -1
  if (value ~ /,/ && value !~ /\./) gsub(/,/, ".", value)
  suffix=toupper(substr(value, length(value), 1))
  if (suffix == "B") {
    value=substr(value, 1, length(value)-1)
    suffix=toupper(substr(value, length(value), 1))
  }
  if (suffix == "I") {
    value=substr(value, 1, length(value)-1)
    suffix=toupper(substr(value, length(value), 1))
  }
  factor=1
  if (suffix == "K") { factor=1024; value=substr(value, 1, length(value)-1) }
  else if (suffix == "M") { factor=1048576; value=substr(value, 1, length(value)-1) }
  else if (suffix == "G") { factor=1073741824; value=substr(value, 1, length(value)-1) }
  else if (suffix == "T") { factor=1099511627776; value=substr(value, 1, length(value)-1) }
  else if (suffix == "P") { factor=1125899906842624; value=substr(value, 1, length(value)-1) }
  else if (suffix ~ /[A-Z]/) return -1
  if (value !~ /^[0-9]+([.][0-9]+)?$/) return -1
  return (value + 0) * factor
}
function limit_pair(line, kind, inner, count, parts, used, total) {
  inner=line
  if (index(inner, "(") == 0) return ""
  sub(/^.*\(/, "", inner)
  sub(/\).*$/, "", inner)
  gsub(/[[:space:]]/, "", inner)
  count=split(inner, parts, "/")
  if (count != 2) return ""
  used=human_bytes(parts[1])
  total=human_bytes(parts[2])
  if (used < 0 || total <= 0) return ""
  if (kind == "cpu") return sprintf("%.6f|%.6f", used, total)
  return sprintf("%.0f|%.0f", used, total)
}
{
  lower=tolower($0)
  pair=""
  if (index(lower, "disk quota") > 0 || index(lower, "diskquota") > 0) {
    pair=limit_pair($0, "disk")
    if (pair != "") disk=pair
  } else if (index(lower, "ram memory") > 0 || index(lower, "memory") > 0) {
    pair=limit_pair($0, "memory")
    if (pair != "") memory=pair
  } else if (index(lower, "cpu") > 0) {
    pair=limit_pair($0, "cpu")
    if (pair != "") cpu=pair
  }
}
END {
  print "disk=" disk
  print "memory=" memory
  print "cpu=" cpu
}')
account_disk=$(printf '%s\n' "$account_limits" | awk -F= '$1 == "disk" { print $2; exit }')
account_memory=$(printf '%s\n' "$account_limits" | awk -F= '$1 == "memory" { print $2; exit }')
account_cpu=$(printf '%s\n' "$account_limits" | awk -F= '$1 == "cpu" { print $2; exit }')
if [ -n "$standard_memory" ]; then
  account_memory="$standard_memory"
fi
account_cpu_source="none"
if [ -n "$standard_cpu" ]; then
  account_cpu="$standard_cpu"
  account_cpu_source="rctl"
fi
account_swap="$standard_swap"
if [ -n "$quota_disk" ]; then
  account_disk="$quota_disk"
fi
account_scope=0
if [ -n "$account_disk" ] || [ -n "$account_memory" ] || [ -n "$account_cpu" ] || [ -n "$account_swap" ] || [ "$standard_process_scope" -gt 0 ]; then
  account_scope=1
fi

if [ -n "$account_cpu" ]; then
  account_cpu_used=$(printf '%s' "$account_cpu" | awk -F'|' '{ print $1 }')
  account_cpu_total=$(printf '%s' "$account_cpu" | awk -F'|' '{ print $2 }')
  if [ "$account_cpu_source" = "rctl" ]; then
    # rctl's pcpu is already a percentage of one CPU core; do not turn it
    # into a quota-consumption ratio a second time.
    cpu_pct=$(awk -v value="$account_cpu_used" 'BEGIN {
      if (value < 0) value=0
      if (value > 100) value=100
      printf "%.1f", value
    }')
  else
    # Provider output uses the explicit used/limit pair shown to the user.
    cpu_pct=$(awk -v used="$account_cpu_used" -v total="$account_cpu_total" 'BEGIN {
      if (total <= 0) { print "0.0"; exit }
      value=used*100/total
      if (value < 0) value=0
      if (value > 100) value=100
      printf "%.1f", value
    }')
  fi
  cpu_usage=$(awk -v value="$cpu_pct" 'BEGIN {
    if (value < 0) value=0
    if (value > 100) value=100
    idle=100-value
    if (idle < 0) idle=0
    printf "%.1f|0.0|0.0|%.1f|0.0|0.0|0.0|0.0", value, idle
  }')
fi

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

if [ -n "$account_memory" ]; then
  account_memory_used_bytes=$(printf '%s' "$account_memory" | awk -F'|' '{ print $1 }')
  account_memory_total_bytes=$(printf '%s' "$account_memory" | awk -F'|' '{ print $2 }')
  account_memory_used_bytes=$(read_number "$account_memory_used_bytes")
  account_memory_total_bytes=$(read_number "$account_memory_total_bytes")
  if [ "$account_memory_total_bytes" -gt 0 ]; then
    if [ "$account_memory_used_bytes" -gt "$account_memory_total_bytes" ]; then
      account_memory_available_bytes=0
    else
      account_memory_available_bytes=$((account_memory_total_bytes-account_memory_used_bytes))
    fi
    mem_percent=$(awk -v used="$account_memory_used_bytes" -v total="$account_memory_total_bytes" 'BEGIN {
      value=used*100/total
      if (value < 0) value=0
      if (value > 100) value=100
      printf "%.2f", value
    }')
    mem=$(awk -v used="$account_memory_used_bytes" -v total="$account_memory_total_bytes" -v percent="$mem_percent" 'BEGIN {
      printf "%.1f|%.1f|%s|0|0|0", used/1048576, total/1048576, percent
    }')
    mem_bytes="$account_memory_used_bytes|$account_memory_total_bytes|$account_memory_available_bytes|$mem_percent|0|0|0"
  fi
fi

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
if [ -n "$account_swap" ]; then
  account_swap_used_bytes=$(printf '%s' "$account_swap" | awk -F'|' '{ print $1 }')
  account_swap_total_bytes=$(printf '%s' "$account_swap" | awk -F'|' '{ print $2 }')
  account_swap_used_bytes=$(read_number "$account_swap_used_bytes")
  account_swap_total_bytes=$(read_number "$account_swap_total_bytes")
  if [ "$account_swap_total_bytes" -gt 0 ]; then
    if [ "$account_swap_used_bytes" -gt "$account_swap_total_bytes" ]; then
      account_swap_available_bytes=0
    else
      account_swap_available_bytes=$((account_swap_total_bytes-account_swap_used_bytes))
    fi
    account_swap_percent=$(awk -v used="$account_swap_used_bytes" -v total="$account_swap_total_bytes" 'BEGIN {
      value=used*100/total
      if (value < 0) value=0
      if (value > 100) value=100
      printf "%.1f", value
    }')
    swap_used_bytes=$account_swap_used_bytes
    swap_total_bytes=$account_swap_total_bytes
    swap_available_bytes=$account_swap_available_bytes
    swap_percent=$account_swap_percent
    swap_bytes="$swap_used_bytes|$swap_total_bytes|$swap_available_bytes|$swap_percent"
    swap=$(awk -v used="$swap_used_bytes" -v total="$swap_total_bytes" -v percent="$swap_percent" 'BEGIN {
      printf "%.1f|%.1f|%s", used/1048576, total/1048576, percent
    }')
  fi
fi

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

if [ -n "$account_disk" ]; then
  account_disk_used_bytes=$(printf '%s' "$account_disk" | awk -F'|' '{ print $1 }')
  account_disk_total_bytes=$(printf '%s' "$account_disk" | awk -F'|' '{ print $2 }')
  account_disk_used_bytes=$(read_number "$account_disk_used_bytes")
  account_disk_total_bytes=$(read_number "$account_disk_total_bytes")
  if [ "$account_disk_total_bytes" -gt 0 ]; then
    if [ "$account_disk_used_bytes" -gt "$account_disk_total_bytes" ]; then
      account_disk_available_bytes=0
    else
      account_disk_available_bytes=$((account_disk_total_bytes-account_disk_used_bytes))
    fi
    account_disk_percent=$(awk -v used="$account_disk_used_bytes" -v total="$account_disk_total_bytes" 'BEGIN {
      value=used*100/total
      if (value < 0) value=0
      if (value > 100) value=100
      printf "%.2f", value
    }')
    account_disk_used_display=$(format_quota_bytes "$account_disk_used_bytes")
    account_disk_total_display=$(format_quota_bytes "$account_disk_total_bytes")
    account_disk_available_display=$(format_quota_bytes "$account_disk_available_bytes")
    account_disk_mount=$(printf '%s' "$account_disk" | awk -F'|' '{ print $3; exit }')
    [ -z "$account_disk_mount" ] && account_disk_mount=${HOME:-/}
    disk="$account_disk_mount|$account_disk_used_display/$account_disk_total_display"
    filesystems="account quota|$account_disk_total_display|$account_disk_used_display|$account_disk_percent%|$account_disk_available_display|$account_disk_mount"
  fi
fi

procs=$(ps -axo pid=,user=,rss=,pcpu=,pmem=,command= 2>/dev/null | awk -v logical_cpu_count="$cpu_cores" -v account_scope="$account_scope" -v current_user="$current_user" '
NF >= 6 {
  pid=$1
  user=$2
  rss=$3+0
  cpu=$4+0
  mem=$5+0
  args=$6
  for (i=7; i<=NF; i++) args=args " " $i
  if (account_scope + 0 > 0 && current_user != "" && user != current_user) next
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
