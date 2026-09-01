fn megabytes_to_bytes(val: &str) -> f64 {
    val.parse::<f64>().unwrap_or(0.0) * 1024.0 * 1024.0
}

fn format_bytes_as_megabytes(val: f64) -> String {
    let megabytes = val / 1024.0 / 1024.0;
    if megabytes >= 1024.0 {
        format!("{:.1}G", megabytes / 1024.0)
    } else {
        format!("{}M", megabytes.round() as i64)
    }
}

fn format_rate(bytes_per_sec: f64) -> String {
    let bps = bytes_per_sec.max(0.0);
    if bps >= 1024.0 * 1024.0 {
        format!("{}M", (bps / 1024.0 / 1024.0).round() as i64)
    } else if bps >= 1024.0 {
        format!("{}K", (bps / 1024.0).round() as i64)
    } else {
        format!("{}B", bps as i64)
    }
}

fn format_network_bytes(bytes: f64) -> String {
    if bytes >= 1024.0 * 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} TB", bytes / 1024.0 / 1024.0 / 1024.0 / 1024.0)
    } else if bytes >= 1024.0 * 1024.0 * 1024.0 {
        let decimals = if bytes >= 10.0 * 1024.0 * 1024.0 * 1024.0 {
            0
        } else {
            1
        };
        format!("{:.*} GB", decimals, bytes / 1024.0 / 1024.0 / 1024.0)
    } else if bytes >= 1024.0 * 1024.0 {
        let decimals = if bytes >= 10.0 * 1024.0 * 1024.0 {
            0
        } else {
            1
        };
        format!("{:.*} MB", decimals, bytes / 1024.0 / 1024.0)
    } else if bytes >= 1024.0 {
        format!("{} KB", (bytes / 1024.0).round() as i64)
    } else {
        format!("{} B", bytes as i64)
    }
}

fn format_storage_usage(value: &str) -> String {
    if value.is_empty() {
        return "-".to_string();
    }
    if let Some(idx) = value.find('/') {
        format!(
            "{}/{}",
            format_storage_value(&value[..idx]),
            format_storage_value(&value[idx + 1..])
        )
    } else {
        format_storage_value(value)
    }
}

fn format_storage_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed.contains(' ') {
        return trimmed.to_string();
    }
    let re = regex::Regex::new(r"(?i)^(\d+(?:\.\d+)?)([KMGT])(?:I?B)?$").unwrap();
    if let Some(caps) = re.captures(trimmed) {
        let val_num: f64 = caps[1].parse().unwrap_or(0.0);
        let unit = caps[2].to_uppercase();
        let power = match unit.as_str() {
            "K" => 1,
            "M" => 2,
            "G" => 3,
            "T" => 4,
            _ => 0,
        };
        let mut bytes = val_num * 1024_f64.powi(power);
        let display_units = ["B", "KB", "MB", "GB", "TB"];
        let mut idx = 0;
        while bytes >= 1024.0 && idx < display_units.len() - 1 {
            bytes /= 1024.0;
            idx += 1;
        }
        let decimals = if idx == 0 { 0 } else { 1 };
        return format!("{:.*} {}", decimals, bytes, display_units[idx]);
    }
    trimmed.to_string()
}

fn parse_gpu_memory_bytes(value: &str) -> Option<f64> {
    let normalized = value.trim().replace(' ', "");
    if normalized.is_empty() || normalized == "-" {
        return None;
    }

    let re = regex::Regex::new(r"(?i)^([0-9]+(?:\.[0-9]+)?)([KMGT]?)(?:I?B)?$").unwrap();
    let caps = re.captures(&normalized)?;
    let amount = caps.get(1)?.as_str().parse::<f64>().ok()?;
    let unit = caps.get(2).map(|m| m.as_str().to_ascii_uppercase());
    let power = match unit.as_deref() {
        Some("K") => 1,
        Some("M") => 2,
        Some("G") => 3,
        Some("T") => 4,
        // nvidia-smi is called with `nounits`, and its memory fields are MiB.
        Some("") | None => 2,
        _ => return None,
    };

    Some(amount * 1024_f64.powi(power))
}

fn format_gpu_memory(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return "-".to_string();
    }

    let normalized = trimmed.replace(' ', "");
    if normalized == "-" {
        return "-".to_string();
    }
    format_storage_value(&normalized)
}

fn parse_gpu_percent(value: &str) -> Option<f64> {
    // PowerShell's Windows emitter formats nvidia-smi values as `49 %`.
    // Trim again after removing the suffix so the whitespace between the
    // number and unit does not turn an otherwise valid sample into `None`.
    let normalized = value.trim().trim_end_matches('%').trim();
    let parsed = normalized.parse::<f64>().ok()?;
    Some(parsed.clamp(0.0, 100.0))
}

fn parse_gpu_temperature(value: &str) -> Option<f64> {
    value
        .trim()
        .trim_end_matches('C')
        .trim_end_matches('c')
        .trim_end_matches('°')
        .trim()
        .parse::<f64>()
        .ok()
}

fn format_gpu_optional(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value == "-" {
        None
    } else {
        Some(value.to_string())
    }
}

fn format_process_megabytes(value: f64) -> String {
    if value >= 1024.0 {
        let decimals = if value >= 10.0 * 1024.0 { 0 } else { 1 };
        format!("{:.*}G", decimals, value / 1024.0)
    } else {
        let decimals = if value >= 100.0 { 0 } else { 1 };
        format!("{:.*}M", decimals, value)
    }
}

pub fn parse_system_metrics(raw: &str, fallback_platform: &str) -> serde_json::Value {
    let normalized_raw = raw.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized_raw.split('\n').collect();

    let read_line = |key: &str| -> String {
        for line in &lines {
            if let Some(stripped) = line.strip_prefix(key) {
                return stripped.trim().to_string();
            }
        }
        "".to_string()
    };

    let read_block = |start: &str, end: &str| -> Vec<String> {
        let start_index = match normalized_raw.find(start) {
            Some(idx) => idx,
            None => return Vec::new(),
        };
        let body_start = start_index + start.len();
        // 起始标记存在但结束标记缺失时，远端脚本可能被截断；
        // 取到字符串结尾作为容错，避免静默丢弃已采集到的数据。
        let body = match normalized_raw[body_start..].find(end) {
            Some(idx) => &normalized_raw[body_start..body_start + idx],
            None => &normalized_raw[body_start..],
        };
        body.trim()
            .split('\n')
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    };

    let platform = read_line("__PLATFORM__");
    let platform = if platform.is_empty() {
        fallback_platform.to_string()
    } else {
        platform
    };
    let load_unit = read_line("__LOAD_UNIT__");
    let load_unit = if load_unit == "busy-logical-processors" {
        Some("busy-logical-processors")
    } else {
        None
    };

    let mem_line = read_line("__MEM__");
    let mem_parts: Vec<&str> = mem_line.split('|').collect();
    let mem_used = mem_parts.first().copied().unwrap_or("0");
    let mem_total = mem_parts.get(1).copied().unwrap_or("0");
    let mem_percent = mem_parts.get(2).copied().unwrap_or("0");
    let mem_app = mem_parts.get(3).copied().unwrap_or("0");
    let mem_cache = mem_parts.get(4).copied().unwrap_or("0");
    let mem_kernel = mem_parts.get(5).copied().unwrap_or("0");

    let mem_bytes_line = read_line("__MEM_BYTES__");
    let mem_bytes_parts: Vec<&str> = mem_bytes_line.split('|').collect();
    let mem_used_bytes = mem_bytes_parts.first().copied().unwrap_or("");
    let mem_total_bytes = mem_bytes_parts.get(1).copied().unwrap_or("");
    let mem_available_bytes = mem_bytes_parts.get(2).copied().unwrap_or("");
    let mem_raw_percent = mem_bytes_parts.get(3).copied().unwrap_or("");
    let mem_app_bytes = mem_bytes_parts.get(4).copied().unwrap_or("");
    let mem_cache_bytes = mem_bytes_parts.get(5).copied().unwrap_or("");
    let mem_kernel_bytes = mem_bytes_parts.get(6).copied().unwrap_or("");

    let swap_line = read_line("__SWAP__");
    let swap_parts: Vec<&str> = swap_line.split('|').collect();
    let swap_used = swap_parts.first().copied().unwrap_or("0");
    let swap_total = swap_parts.get(1).copied().unwrap_or("0");
    let swap_percent = swap_parts.get(2).copied().unwrap_or("0");

    let swap_bytes_line = read_line("__SWAP_BYTES__");
    let swap_bytes_parts: Vec<&str> = swap_bytes_line.split('|').collect();
    let swap_used_bytes = swap_bytes_parts.first().copied().unwrap_or("");
    let swap_total_bytes = swap_bytes_parts.get(1).copied().unwrap_or("");
    let swap_available_bytes = swap_bytes_parts.get(2).copied().unwrap_or("");
    let swap_raw_percent = swap_bytes_parts.get(3).copied().unwrap_or("");

    let cpu_line = read_line("__CPU_USAGE__");
    let cpu_parts: Vec<&str> = cpu_line.split('|').collect();
    let cpu_user = cpu_parts
        .first()
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_system = cpu_parts
        .get(1)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_nice = cpu_parts
        .get(2)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_idle = cpu_parts
        .get(3)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_iowait = cpu_parts
        .get(4)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_irq = cpu_parts
        .get(5)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_softirq = cpu_parts
        .get(6)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_steal = cpu_parts
        .get(7)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);

    let rates_line = read_line("__RATES__");
    let rates_parts: Vec<&str> = rates_line.split('|').collect();
    let rx_rate = rates_parts
        .first()
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0)
        .max(0.0);
    let tx_rate = rates_parts
        .get(1)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0)
        .max(0.0);

    let interfaces: Vec<String> = read_line("__IFACES__")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // parse network interface rates
    let mut network_interface_rows = Vec::new();
    let mut network_rates_by_interface = serde_json::Map::new();
    let mut network_samples_by_interface = serde_json::Map::new();
    let mut network_raw_by_interface = serde_json::Map::new();

    let mut aggregate_rx_bytes = 0.0;
    let mut aggregate_tx_bytes = 0.0;

    for line in read_block("__IFACE_RATES_START__", "__IFACE_RATES_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 5 {
            let name = parts[0].to_string();
            let rx_total = parts[1].parse::<f64>().unwrap_or(0.0);
            let tx_total = parts[2].parse::<f64>().unwrap_or(0.0);
            let rx = parts[3].parse::<f64>().unwrap_or(0.0).max(0.0);
            let tx = parts[4].parse::<f64>().unwrap_or(0.0).max(0.0);

            aggregate_rx_bytes += rx_total;
            aggregate_tx_bytes += tx_total;

            network_interface_rows.push(serde_json::json!({
                "name": name,
                "txTotal": format_network_bytes(tx_total),
                "rxTotal": format_network_bytes(rx_total),
                "txRate": format_rate(tx),
                "rxRate": format_rate(rx),
            }));

            network_rates_by_interface.insert(
                name.clone(),
                serde_json::json!({
                    "rx": format_rate(rx),
                    "tx": format_rate(tx),
                }),
            );

            network_samples_by_interface.insert(
                name.clone(),
                serde_json::json!([
                    { "rx": rx, "tx": tx }
                ]),
            );

            network_raw_by_interface.insert(
                name.clone(),
                serde_json::json!({
                    "name": name,
                    "rxBytes": rx_total,
                    "txBytes": tx_total,
                    "rxBytesPerSecond": rx,
                    "txBytesPerSecond": tx,
                }),
            );
        }
    }

    let mut disk_rows = Vec::new();
    for line in read_block("__DISK_START__", "__DISK_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 2 {
            disk_rows.push(serde_json::json!({
                "path": parts[0],
                "usage": format_storage_usage(parts[1]),
            }));
        }
    }

    let mut file_system_rows = Vec::new();
    for line in read_block("__FILESYSTEMS_START__", "__FILESYSTEMS_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 6 {
            file_system_rows.push(serde_json::json!({
                "name": parts[0],
                "size": format_storage_value(parts[1]),
                "used": format_storage_value(parts[2]),
                "usagePercent": parts[3],
                "available": format_storage_value(parts[4]),
                "mountPoint": parts[5],
            }));
        }
    }

    // Newer collectors already provide the richer filesystem rows, while the
    // compact sidebar table still consumes the legacy diskRows shape. Keep the
    // compact table populated when a platform/collector emits only the richer
    // block (which is what caused the sidebar to show an empty body).
    if disk_rows.is_empty() {
        for row in &file_system_rows {
            let path = row
                .get("mountPoint")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .or_else(|| row.get("name").and_then(serde_json::Value::as_str));
            let available = row.get("available").and_then(serde_json::Value::as_str);
            let size = row.get("size").and_then(serde_json::Value::as_str);

            if let (Some(path), Some(available), Some(size)) = (path, available, size) {
                disk_rows.push(serde_json::json!({
                    "path": path,
                    "usage": format!("{available}/{size}"),
                }));
            }
        }
    }

    let mut cpu_info_rows = Vec::new();
    for line in read_block("__CPUINFO_START__", "__CPUINFO_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 5 {
            cpu_info_rows.push(serde_json::json!({
                "model": parts[0],
                "cores": parts[1].parse::<i64>().unwrap_or(0),
                "frequencyMHz": parts[2],
                "cache": parts[3],
                "bogomips": parts[4],
            }));
        }
    }

    let mut gpu_info_rows = Vec::new();
    for line in read_block("__GPUINFO_START__", "__GPUINFO_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 4 {
            let total_memory_bytes = parse_gpu_memory_bytes(parts[3]);
            let used_memory_bytes = parts.get(5).and_then(|value| parse_gpu_memory_bytes(value));
            let memory_percent = match (used_memory_bytes, total_memory_bytes) {
                (Some(used), Some(total)) if total > 0.0 => {
                    Some((used * 100.0 / total).clamp(0.0, 100.0))
                }
                _ => None,
            };
            gpu_info_rows.push(serde_json::json!({
                "model": parts[0],
                "vendor": if parts[1].is_empty() { "-" } else { parts[1] },
                "driver": if parts[2].is_empty() { "-" } else { parts[2] },
                "memory": format_gpu_memory(parts[3]),
                "usagePercent": parts.get(4).and_then(|value| parse_gpu_percent(value)),
                "memoryUsed": parts
                    .get(5)
                    .map(|value| format_gpu_memory(value))
                    .filter(|value| value != "-"),
                "memoryPercent": memory_percent,
                "temperatureCelsius": parts.get(6).and_then(|value| parse_gpu_temperature(value)),
                "powerUsage": format_gpu_optional(parts.get(7).copied()),
                "powerLimit": format_gpu_optional(parts.get(8).copied()),
            }));
        }
    }

    // Top processes: shell 端按瞬时 CPU 占用降序取 top 40，
    // 这里按到达顺序逐行解析，不做 comm 分组。每行一个 PID，保留 pid/user
    // 字段供排查使用，command 用 args（完整命令行）而非 comm。
    // 格式：pid|user|rss(M)|pcpu|pmem|args（args 内部空格保留）
    let transient_collector_commands: std::collections::HashSet<&str> =
        ["ps", "awk", "bash", "sleep", "sh", "powershell", "pwsh"]
            .iter()
            .cloned()
            .collect();
    let mut top_processes: Vec<serde_json::Value> = Vec::new();
    for line in read_block("__PROCS_START__", "__PROCS_END__") {
        // splitn(6) 保留 args 内部所有字符（含 |），避免误切
        let parts: Vec<&str> = line.splitn(6, '|').collect();
        if parts.len() < 6 {
            continue;
        }
        let pid: u32 = parts[0].parse().unwrap_or(0);
        let user = parts[1].to_string();
        let memory_str = parts[2].to_lowercase();
        let memory_mb: f64 = memory_str.replace('m', "").parse().unwrap_or(0.0);
        let cpu_val = match parts[3].parse::<f64>() {
            Ok(value) if value.is_finite() && (0.0..=100.0).contains(&value) => value,
            // The collector is expected to emit a system-wide 0-100 value.
            // Do not let malformed or unbounded samples reach the renderer.
            _ => continue,
        };
        let _mem_percent: f64 = parts[4].parse().unwrap_or(0.0);
        let command = parts[5].to_string();

        // 过滤采集器自身（ps/awk/sh 等），按 args 首字段匹配
        let comm = command.split_whitespace().next().unwrap_or("");
        let comm_basename = comm.rsplit('/').next().unwrap_or(comm);
        if transient_collector_commands.contains(comm_basename) {
            continue;
        }

        top_processes.push(serde_json::json!({
            "pid": pid,
            "user": user,
            "memory": format_process_megabytes(memory_mb),
            "cpu": format!("{:.1}", cpu_val),
            "command": command,
            "elapsedSeconds": 0_i64,
        }));
    }

    let mem_used_bytes_num = mem_used_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_used));
    let mem_total_bytes_num = mem_total_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_total));
    let mem_available_bytes_num = mem_available_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| (mem_total_bytes_num - mem_used_bytes_num).max(0.0));
    let mem_percent_num = mem_raw_percent
        .parse::<f64>()
        .unwrap_or_else(|_| mem_percent.parse::<f64>().unwrap_or(0.0));

    let swap_used_bytes_num = swap_used_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(swap_used));
    let swap_total_bytes_num = swap_total_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(swap_total));
    let swap_available_bytes_num = swap_available_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| (swap_total_bytes_num - swap_used_bytes_num).max(0.0));
    let swap_percent_num = swap_raw_percent
        .parse::<f64>()
        .unwrap_or_else(|_| swap_percent.parse::<f64>().unwrap_or(0.0));

    let mem_app_bytes_num = mem_app_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_app));
    let mem_cache_bytes_num = mem_cache_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_cache));
    let mem_kernel_bytes_num = mem_kernel_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_kernel));

    let aggregate_network_raw = serde_json::json!({
        "name": "all",
        "rxBytes": aggregate_rx_bytes,
        "txBytes": aggregate_tx_bytes,
        "rxBytesPerSecond": rx_rate,
        "txBytesPerSecond": tx_rate,
    });

    let has_mem_app = mem_app.parse::<f64>().unwrap_or(0.0) > 0.0 || mem_app_bytes_num > 0.0;
    let has_mem_cache = mem_cache.parse::<f64>().unwrap_or(0.0) > 0.0 || mem_cache_bytes_num > 0.0;
    let has_mem_kernel =
        mem_kernel.parse::<f64>().unwrap_or(0.0) > 0.0 || mem_kernel_bytes_num > 0.0;

    let mut network_rates_all = serde_json::Map::new();
    network_rates_all.insert(
        "all".to_string(),
        serde_json::json!({
            "rx": format_rate(rx_rate),
            "tx": format_rate(tx_rate),
        }),
    );
    for (k, v) in network_rates_by_interface.iter() {
        network_rates_all.insert(k.clone(), v.clone());
    }

    let mut network_samples_all = serde_json::Map::new();
    network_samples_all.insert(
        "all".to_string(),
        serde_json::json!([
            { "rx": rx_rate, "tx": tx_rate }
        ]),
    );
    for (k, v) in network_samples_by_interface.iter() {
        network_samples_all.insert(k.clone(), v.clone());
    }

    let mut network_raw_all = serde_json::Map::new();
    network_raw_all.insert("all".to_string(), aggregate_network_raw);
    for (k, v) in network_raw_by_interface.iter() {
        network_raw_all.insert(k.clone(), v.clone());
    }

    let mut network_interfaces_val = vec![serde_json::Value::String("all".to_string())];
    for iface in interfaces {
        network_interfaces_val.push(serde_json::Value::String(iface));
    }

    serde_json::json!({
        "platform": platform,
        "ip": read_line("__IP__"),
        "uptime": if read_line("__UPTIME__").is_empty() { "-".to_string() } else { read_line("__UPTIME__") },
        "uptimeSeconds": read_line("__UPTIME_SECONDS__").parse::<i64>().ok(),
        "load": if read_line("__LOAD__").is_empty() { "-".to_string() } else { read_line("__LOAD__") },
        "loadUnit": load_unit,
        "identity": {
            "osName": if read_line("__OS__").is_empty() { "-".to_string() } else { read_line("__OS__") },
            "kernelName": if read_line("__KERNEL_NAME__").is_empty() { "-".to_string() } else { read_line("__KERNEL_NAME__") },
            "kernelVersion": if read_line("__KERNEL_VERSION__").is_empty() { "-".to_string() } else { read_line("__KERNEL_VERSION__") },
            "architecture": if read_line("__ARCH__").is_empty() { "-".to_string() } else { read_line("__ARCH__") },
            "hostname": if read_line("__HOSTNAME__").is_empty() { "-".to_string() } else { read_line("__HOSTNAME__") },
        },
        "cpuPercent": read_line("__CPU__").parse::<f64>().unwrap_or(0.0),
        "cpuUsage": {
            "user": cpu_user,
            "system": cpu_system,
            "nice": cpu_nice,
            "idle": cpu_idle,
            "ioWait": cpu_iowait,
            "irq": cpu_irq,
            "softIrq": cpu_softirq,
            "steal": cpu_steal,
        },
        "cpuInfoRows": cpu_info_rows,
        "gpuInfoRows": gpu_info_rows,
        "memoryPercent": mem_percent_num,
        "memoryUsage": if mem_total_bytes_num > 0.0 {
            format!("{}/{}", format_bytes_as_megabytes(mem_used_bytes_num), format_bytes_as_megabytes(mem_total_bytes_num))
        } else {
            "0/0".to_string()
        },
        "memoryAppUsage": if has_mem_app { Some(format_bytes_as_megabytes(mem_app_bytes_num)) } else { None },
        "memoryCacheUsage": if has_mem_cache { Some(format_bytes_as_megabytes(mem_cache_bytes_num)) } else { None },
        "memoryKernelUsage": if has_mem_kernel { Some(format_bytes_as_megabytes(mem_kernel_bytes_num)) } else { None },
        "memoryBreakdown": {
            "total": format_bytes_as_megabytes(mem_total_bytes_num),
            "used": format_bytes_as_megabytes(mem_used_bytes_num),
            "available": format_bytes_as_megabytes(mem_available_bytes_num),
            "percent": mem_percent_num,
        },
        "memoryRaw": {
            "totalBytes": mem_total_bytes_num,
            "usedBytes": mem_used_bytes_num,
            "availableBytes": mem_available_bytes_num,
            "percent": mem_percent_num,
            "appBytes": mem_app_bytes_num,
            "cacheBytes": mem_cache_bytes_num,
            "kernelBytes": mem_kernel_bytes_num,
        },
        "swapPercent": swap_percent_num,
        "swapUsage": if swap_total_bytes_num > 0.0 {
            format!("{}/{}", format_bytes_as_megabytes(swap_used_bytes_num), format_bytes_as_megabytes(swap_total_bytes_num))
        } else {
            "0/0".to_string()
        },
        "swapBreakdown": {
            "total": format_bytes_as_megabytes(swap_total_bytes_num),
            "used": format_bytes_as_megabytes(swap_used_bytes_num),
            "available": format_bytes_as_megabytes(swap_available_bytes_num),
            "percent": swap_percent_num,
        },
        "swapRaw": {
            "totalBytes": swap_total_bytes_num,
            "usedBytes": swap_used_bytes_num,
            "availableBytes": swap_available_bytes_num,
            "percent": swap_percent_num,
        },
        "diskRows": disk_rows,
        "fileSystemRows": file_system_rows,
        "networkInterfaces": network_interfaces_val,
        "activeNetworkInterface": "all",
        "networkRates": {
            "rx": format_rate(rx_rate),
            "tx": format_rate(tx_rate),
        },
        "networkSamples": [
            { "rx": rx_rate, "tx": tx_rate }
        ],
        "networkInterfaceRows": network_interface_rows,
        "networkRatesByInterface": network_rates_all,
                "networkSamplesByInterface": network_samples_all,
        "networkRawByInterface": network_raw_all,
        "topProcesses": top_processes,
    })
}
