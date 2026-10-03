fn build_posix_monitor_fallback_script() -> &'static str {
    r#"monitor_fallback=""
if [ "$total1" -le 0 ] || [ "$total2" -le 0 ] || [ -z "$mem_bytes" ]; then
  if has_bounded_runner; then
    # htop is useful on restricted appliances that expose a monitor command
    # but do not expose the usual /proc counters to the SSH user. Its batch
    # output is bounded before parsing so a verbose implementation cannot
    # grow the collector channel without limit.
    monitor_fallback=$(run_bounded 2 htop -b -n 1 2>/dev/null | head -n 80)
    [ -z "$monitor_fallback" ] && monitor_fallback=$(run_bounded 2 top -b -n 1 2>/dev/null | head -n 80)
  fi
fi
if [ -n "$monitor_fallback" ]; then
  fallback_cpu=$(printf "%s\n" "$monitor_fallback" | awk '
    function number(value) { gsub(/[^0-9.]/, "", value); return value + 0 }
    function clamp(value) { if (value < 0) return 0; if (value > 100) return 100; return value }
    {
      line=tolower($0)
      if (line ~ /cpu\(s\):/ || line ~ /^cpu:/) {
        for (i=1; i<=NF; i++) {
          token=tolower($i)
          next_label=tolower($(i+1))
          if (token ~ /%us/) { user=number(token); seen=1 }
          if (token ~ /%sy/) { sys_pct=number(token); seen=1 }
          if (token ~ /%ni/) { nice=number(token); seen=1 }
          if (token ~ /%id/) { idle=number(token); idle_seen=1; seen=1 }
          if (token ~ /%wa/) { iowait=number(token); seen=1 }
          if (token ~ /%hi/) { irq=number(token); seen=1 }
          if (token ~ /%si/) { softirq=number(token); seen=1 }
          if (token ~ /%st/) { steal=number(token); seen=1 }
          if (token ~ /%$/ && next_label ~ /^usr/) { user=number(token); seen=1 }
          if (token ~ /%$/ && next_label ~ /^sys/) { sys_pct=number(token); seen=1 }
          if (token ~ /%$/ && next_label ~ /^(nic|nice)/) { nice=number(token); seen=1 }
          if (token ~ /%$/ && next_label ~ /^idle/) { idle=number(token); idle_seen=1; seen=1 }
          if (token ~ /%$/ && next_label ~ /^(wa|iowait)/) { iowait=number(token); seen=1 }
          if (token ~ /%$/ && next_label ~ /^(hi|irq)/) { irq=number(token); seen=1 }
          if (token ~ /%$/ && next_label ~ /^(si|sirq|softirq)/) { softirq=number(token); seen=1 }
          if (token ~ /%$/ && next_label ~ /^(st|steal)/) { steal=number(token); seen=1 }
        }
      }
      if (line ~ /^cpu[[:space:]]*\[/) {
        for (i=1; i<=NF; i++) {
          token=tolower($i)
          if (token ~ /%/) { overall_sum += number(token); overall_count++; break }
        }
      }
      # htop batch mode prints one bar per logical CPU, for example
      # `0[||||] 12.5%`. Average the bars when no top-style header exists.
      if (line ~ /^[[:space:]]*[0-9]+[[:space:]]*\[/) {
        for (i=1; i<=NF; i++) {
          token=tolower($i)
          if (token ~ /%/) { overall_sum += number(token); overall_count++; break }
        }
      }
    }
    END {
      if (overall_count > 0) { overall=overall_sum / overall_count; overall_seen=1 }
      if (!seen && !overall_seen) exit
      if (!overall_seen) {
        if (idle_seen) overall=100-idle
        else overall=user+sys_pct+nice+iowait+irq+softirq+steal
      }
      if (!idle_seen && overall > 0) idle=100-overall
      printf "%.1f|%.1f|%.1f|%.1f|%.1f|%.1f|%.1f|%.1f|%.1f\n", clamp(overall), clamp(user), clamp(sys_pct), clamp(nice), clamp(idle), clamp(iowait), clamp(irq), clamp(softirq), clamp(steal)
    }
  ')
  if [ "$total1" -le 0 ] || [ "$total2" -le 0 ]; then
    if [ -n "$fallback_cpu" ]; then
      fallback_ifs="$IFS"
      IFS='|'
      set -- $fallback_cpu
      IFS="$fallback_ifs"
      cpu_pct=${1:-0}
      cpu_user_pct=${2:-0}
      cpu_system_pct=${3:-0}
      cpu_nice_pct=${4:-0}
      cpu_idle_pct=${5:-0}
      cpu_iowait_pct=${6:-0}
      cpu_irq_pct=${7:-0}
      cpu_softirq_pct=${8:-0}
      cpu_steal_pct=${9:-0}
    fi
  fi

  fallback_mem=$(printf "%s\n" "$monitor_fallback" | awk '
    BEGIN { unit_factor=1 }
    function bytes(value, normalized, number, scale) {
      normalized=tolower(value)
      gsub(/[\[\],]/, "", normalized)
      number=normalized + 0
      scale=unit_factor
      if (normalized ~ /t(i?b)?$/) scale=1024*1024*1024*1024
      else if (normalized ~ /g(i?b)?$/) scale=1024*1024*1024
      else if (normalized ~ /m(i?b)?$/) scale=1024*1024
      else if (normalized ~ /k(i?b)?$/) scale=1024
      return number * scale
    }
    {
      line=tolower($0)
      if (line ~ /gib[[:space:]]+mem/) unit_factor=1024*1024*1024
      else if (line ~ /mib[[:space:]]+mem/) unit_factor=1024*1024
      else if (line ~ /kib[[:space:]]+mem/) unit_factor=1024
      if (line ~ /mem/) {
        for (i=1; i<=NF; i++) {
          label=tolower($(i+1))
          if (label ~ /^total,?$/) total=bytes($i)
          else if (label ~ /^used,?$/) used=bytes($i)
          else if (label ~ /^free,?$/) free=bytes($i)
          else if (label ~ /^avail/) available=bytes($i)
        }
        # htop commonly reports memory as `used/total` next to the Mem bar.
        for (i=1; i<=NF; i++) {
          pair=$i
          if (pair !~ /\//) continue
          gsub(/[\[\]]/, "", pair)
          count=split(pair, values, "/")
          if (count == 2 && values[1] ~ /[0-9]/ && values[2] ~ /[0-9]/) {
            used=bytes(values[1])
            total=bytes(values[2])
          }
        }
      }
    }
    END {
      if (total <= 0) exit
      if (used <= 0 && free > 0) used=total-free
      if (available <= 0) available=free > 0 ? free : total-used
      if (used < 0) used=0
      if (available < 0) available=0
      percent=total > 0 ? used*100/total : 0
      printf "%.0f|%.0f|%.0f|%.1f|0|0|0\n", used, total, available, percent
    }
  ')
  if [ -z "$mem_bytes" ] && [ -n "$fallback_mem" ]; then
    mem_bytes="$fallback_mem"
  fi
fi
"#
}
