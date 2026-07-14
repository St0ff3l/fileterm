// System metrics collection — russh async port.
//
// `probe_remote_platform` and `exec_command[_with_stdin]` are async and
// operate on a `russh::client::Handle`. All parsing/formatting helpers
// below are pure functions and unchanged from the ssh2 era.

use std::collections::HashMap;
use std::time::Duration;

use russh::client::{Handle, Handler};
use russh::ChannelMsg;
use tokio::time::timeout;

// A few SSH servers emit EOF/CLOSE before the final stdout packet is drained.
// Keep a very short grace window after that marker: it preserves output while
// still guaranteeing that servers which omit ExitStatus cannot hold a caller
// until its much longer command watchdog fires.
const EXEC_CHANNEL_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);

pub async fn probe_remote_platform<H: Handler>(handle: &Handle<H>) -> String {
    // 1. Try POSIX probe
    let posix_cmd = "sh -lc 'printf \"__FILETERM_PROBE_START__\\n\"; uname -s 2>/dev/null; shell_exe=$(readlink /proc/$$/exe 2>/dev/null || readlink /bin/sh 2>/dev/null || true); case \"$shell_exe\" in *busybox*) printf \"busybox\\n\" ;; esac; if [ -f /etc/openwrt_release ]; then printf \"openwrt\\n\"; fi; printf \"__FILETERM_PROBE_END__\\n\"'";

    let posix_result = exec_command(handle, posix_cmd).await;
    eprintln!(
        "[SSH probe] posix exec_command result_ok={} len={}",
        posix_result.is_ok(),
        posix_result.as_ref().map(|s| s.len()).unwrap_or(0)
    );
    if let Ok(output) = &posix_result {
        // CRLF normalization — Windows remotes emit `\r\n` which would
        // pollute platform detection (e.g. `linux\r` fails `contains`).
        let output = output.replace("\r\n", "\n").replace('\r', "\n");
        eprintln!(
            "[SSH probe] posix normalized output (first 300): {:?}",
            output.chars().take(300).collect::<String>()
        );
        if let Some(body) = extract_probe_body(&output) {
            let normalized = body.to_lowercase();
            eprintln!("[SSH probe] body='{}' normalized='{}'", body, normalized);
            if normalized.contains("openwrt") || normalized.contains("busybox") {
                return "busybox".to_string();
            }
            if normalized.contains("linux") {
                return "linux".to_string();
            }
        }
    }

    // 2. Try Windows probes
    let windows_cmds = [
        "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"[Environment]::OSVersion.Platform\"",
        "pwsh -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"[Environment]::OSVersion.Platform\"",
        "cmd /c ver",
    ];
    for cmd in &windows_cmds {
        if let Ok(output) = exec_command(handle, cmd).await {
            let output = output.replace("\r\n", "\n").replace('\r', "\n");
            let normalized = output.to_lowercase();
            eprintln!(
                "[SSH probe] windows cmd='{}' output='{}'",
                cmd,
                output.chars().take(100).collect::<String>()
            );
            if normalized.contains("windows") || normalized.contains("win32nt") {
                return "windows".to_string();
            }
        }
    }

    eprintln!("[SSH probe] all probes failed — returning 'unknown'");
    "unknown".to_string()
}

/// Run a command via the exec channel and collect its combined stdout/stderr.
pub async fn exec_command<H: Handler>(handle: &Handle<H>, cmd: &str) -> Result<String, String> {
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel.exec(true, cmd).await.map_err(|e| e.to_string())?;

    let mut output: Vec<u8> = Vec::new();
    let mut draining_after_close = false;
    loop {
        let message = if draining_after_close {
            match timeout(EXEC_CHANNEL_DRAIN_TIMEOUT, channel.wait()).await {
                Ok(message) => message,
                Err(_) => break,
            }
        } else {
            channel.wait().await
        };
        match message {
            Some(ChannelMsg::Data { data }) => {
                output.extend_from_slice(data.as_ref());
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                output.extend_from_slice(data.as_ref());
            }
            Some(ChannelMsg::ExitStatus { .. }) | None => break,
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                draining_after_close = true;
            }
            _ => {}
        }
    }
    Ok(String::from_utf8_lossy(&output).into_owned())
}

/// Run a command via the exec channel, write `stdin` to the channel, then
/// collect the combined stdout/stderr.
pub async fn exec_command_with_stdin<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: &str,
) -> Result<String, String> {
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel.exec(true, cmd).await.map_err(|e| e.to_string())?;
    let stdin_bytes = stdin.as_bytes().to_vec();
    channel.data(&stdin_bytes[..]).await.map_err(|e| e.to_string())?;
    channel.eof().await.map_err(|e| e.to_string())?;

    let mut output: Vec<u8> = Vec::new();
    let mut draining_after_close = false;
    loop {
        let message = if draining_after_close {
            match timeout(EXEC_CHANNEL_DRAIN_TIMEOUT, channel.wait()).await {
                Ok(message) => message,
                Err(_) => break,
            }
        } else {
            channel.wait().await
        };
        match message {
            Some(ChannelMsg::Data { data }) => {
                output.extend_from_slice(data.as_ref());
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                output.extend_from_slice(data.as_ref());
            }
            Some(ChannelMsg::ExitStatus { .. }) | None => break,
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                draining_after_close = true;
            }
            _ => {}
        }
    }
    Ok(String::from_utf8_lossy(&output).into_owned())
}

fn extract_probe_body(raw: &str) -> Option<String> {
    let start_marker = "__FILETERM_PROBE_START__";
    let end_marker = "__FILETERM_PROBE_END__";
    let start = raw.find(start_marker)?;
    let end = raw.find(end_marker)?;
    if end <= start {
        return None;
    }
    Some(raw[start + start_marker.len()..end].to_string())
}

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
        let decimals = if bytes >= 10.0 * 1024.0 * 1024.0 * 1024.0 { 0 } else { 1 };
        format!("{:.*} GB", decimals, bytes / 1024.0 / 1024.0 / 1024.0)
    } else if bytes >= 1024.0 * 1024.0 {
        let decimals = if bytes >= 10.0 * 1024.0 * 1024.0 { 0 } else { 1 };
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
            if line.starts_with(key) {
                return line[key.len()..].trim().to_string();
            }
        }
        "".to_string()
    };

    let read_block = |start: &str, end: &str| -> Vec<String> {
        let start_index = match normalized_raw.find(start) {
            Some(idx) => idx,
            None => return Vec::new(),
        };
        let end_index = match normalized_raw[start_index + start.len()..].find(end) {
            Some(idx) => start_index + start.len() + idx,
            None => return Vec::new(),
        };
        normalized_raw[start_index + start.len()..end_index]
            .trim()
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
    let cpu_system = cpu_parts.get(1).copied().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
    let cpu_nice = cpu_parts.get(2).copied().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
    let cpu_idle = cpu_parts.get(3).copied().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
    let cpu_iowait = cpu_parts.get(4).copied().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
    let cpu_irq = cpu_parts.get(5).copied().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
    let cpu_softirq = cpu_parts.get(6).copied().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
    let cpu_steal = cpu_parts.get(7).copied().unwrap_or("0").parse::<f64>().unwrap_or(0.0);

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

            network_rates_by_interface.insert(name.clone(), serde_json::json!({
                "rx": format_rate(rx),
                "tx": format_rate(tx),
            }));

            network_samples_by_interface.insert(name.clone(), serde_json::json!([
                { "rx": rx, "tx": tx }
            ]));

            network_raw_by_interface.insert(name.clone(), serde_json::json!({
                "name": name,
                "rxBytes": rx_total,
                "txBytes": tx_total,
                "rxBytesPerSecond": rx,
                "txBytesPerSecond": tx,
            }));
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
            gpu_info_rows.push(serde_json::json!({
                "model": parts[0],
                "vendor": if parts[1].is_empty() { "-" } else { parts[1] },
                "driver": if parts[2].is_empty() { "-" } else { parts[2] },
                "memory": if parts[3].is_empty() { "-" } else { parts[3] },
            }));
        }
    }

    // Top processes
    let transient_collector_commands: std::collections::HashSet<&str> =
        ["ps", "awk", "bash", "sleep", "sh", "powershell", "pwsh"]
            .iter()
            .cloned()
            .collect();
    let mut grouped_processes: HashMap<String, (f64, f64, f64)> = HashMap::new();
    for line in read_block("__PROCS_START__", "__PROCS_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 4 {
            let memory_str = parts[0].to_lowercase();
            let memory_mb: f64 = memory_str.replace('m', "").parse().unwrap_or(0.0);
            let cpu_val: f64 = parts[1].parse().unwrap_or(0.0);
            let elapsed: f64 = parts[2].parse().unwrap_or(0.0);
            let command = parts[3].to_string();

            if !transient_collector_commands.contains(command.as_str()) {
                let entry = grouped_processes.entry(command).or_insert((0.0, 0.0, 0.0));
                entry.0 += memory_mb;
                entry.1 += cpu_val;
                entry.2 = entry.2.max(elapsed);
            }
        }
    }
    // Sort numeric values before formatting them. The old implementation
    // formatted all values, then compiled a Regex for every comparison while
    // sorting. On hosts with large process tables that consumed multiple CPU
    // cores continuously and starved Tauri's window/event loop.
    let mut grouped_processes: Vec<(String, (f64, f64, f64))> = grouped_processes.into_iter().collect();
    grouped_processes.sort_by(|a, b| b.1.0.total_cmp(&a.1.0));
    let top_processes: Vec<serde_json::Value> = grouped_processes
        .into_iter()
        .take(128)
        .map(|(command, (memory_mb, cpu, elapsed))| {
            serde_json::json!({
                "memory": format_process_megabytes(memory_mb),
                "cpu": format!("{:.1}", cpu),
                "command": command,
                "elapsedSeconds": elapsed as i64,
            })
        })
        .collect();

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
    let has_mem_cache =
        mem_cache.parse::<f64>().unwrap_or(0.0) > 0.0 || mem_cache_bytes_num > 0.0;
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

pub fn build_posix_metrics_command(platform: &str) -> String {
    let complete_marker = "__FILETERM_METRICS_COMPLETE__";
    format!(r#"cd / >/dev/null 2>&1 || true
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
sleep "$sleep_interval"
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
cpu_info=$(awk -F: '
  /^model name[[:space:]]*:/ || /^Hardware[[:space:]]*:/ || /^Processor[[:space:]]*:/ {{
    current=$2
    sub(/^[[:space:]]+/, "", current)
    if (current != "") {{
      model_order[++model_count]=current
      seen[current]=1
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
    for (index = 1; index <= model_count; index++) {{
      model=model_order[index]
      if (printed[model]) continue
      printed[model]=1
      printf "%s|%s|%s|%s|%s\n", model, (cores[model] == "" ? "0" : cores[model]), (mhz[model] == "" ? "-" : mhz[model]), (cache[model] == "" ? "-" : cache[model]), (bogomips[model] == "" ? "-" : bogomips[model])
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
gpu_info=$(run_bounded 1 nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader,nounits 2>/dev/null | awk -F',' '
  function trim(value) {{
    sub(/^[[:space:]]+/, "", value)
    sub(/[[:space:]]+$/, "", value)
    return value
  }}
  NF >= 3 {{
    model=trim($1)
    driver=trim($2)
    memory=trim($3)
    printf "%s|NVIDIA|%s|%s MiB\n", model, (driver == "" ? "-" : driver), (memory == "" ? "-" : memory)
  }}
')
if [ -z "$gpu_info" ]; then
  gpu_info=$(run_bounded 1 lspci 2>/dev/null | awk '
    BEGIN {{ IGNORECASE=1 }}
    /VGA compatible controller|3D controller|Display controller/ {{
      line=$0
      sub(/^[[:xdigit:]:.]+[[:space:]]+[^:]+: /, "", line)
      vendor=line
      sub(/[[:space:]].*$/, "", vendor)
      printf "%s|%s|-|-\n", line, (vendor == "" ? "-" : vendor)
    }}
  ')
fi
ifaces=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); if (name != "lo") {{ if (out != "") out=out ","; out=out name }}}} END {{print out}}' /proc/net/dev 2>/dev/null)
active_iface=$(awk '$2 == 00000000 {{print $1; exit}}' /proc/net/route 2>/dev/null)
[ -z "$active_iface" ] && active_iface=$(echo "$ifaces" | awk -F, '{{print $1}}')
rx1=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[2]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
tx1=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[10]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
before_file="/tmp/fileterm-if-before-$$"
after_file="/tmp/fileterm-if-after-$$"
trap 'rm -f "$before_file" "$after_file"' 0 1 2 15
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
if has_bounded_runner; then
  procs=$(run_bounded 1 ps -eo rss=,pcpu=,etimes=,comm= 2>/dev/null | head -n 128 | awk 'NF >= 4 {{printf "%.1fM|%s|%s|%s\n", $1/1024, $2, $3, $4}}')
  [ -z "$procs" ] && procs=$(run_bounded 1 ps 2>/dev/null | head -n 128 | awk 'NR>1 && NF >= 5 {{proc_name=$5; sub(/^.*\//, "", proc_name); printf "%.1fM|0|0|%s\n", $3/1024, proc_name}}')
else
  procs=$(ps -eo rss=,pcpu=,etimes=,comm= 2>/dev/null | head -n 128 | awk 'NF >= 4 {{printf "%.1fM|%s|%s|%s\n", $1/1024, $2, $3, $4}}')
  [ -z "$procs" ] && procs=$(ps 2>/dev/null | head -n 128 | awk 'NR>1 && NF >= 5 {{proc_name=$5; sub(/^.*\//, "", proc_name); printf "%.1fM|0|0|%s\n", $3/1024, proc_name}}')
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
echo "{}"
"#, platform, complete_marker)
}

/// PowerShell-based metrics script for Windows remotes.
/// Emits the same `__KEY__VALUE` markers as `build_posix_metrics_command`.
pub fn build_windows_metrics_command() -> String {
    r#"
$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}

function Write-Metric([string]$Name, [object]$Value) {
    if ($null -eq $Value) { $Value = '' }
    Write-Output ('__' + $Name + '__' + [string]$Value)
}

$os = Get-CimInstance Win32_OperatingSystem
$cs = Get-CimInstance Win32_ComputerSystem
$cpu = Get-CimInstance Win32_Processor
$memTotal = [double]$os.TotalVisibleMemorySize * 1KB
$memFree  = [double]$os.FreePhysicalMemory  * 1KB
$memUsed  = $memTotal - $memFree
$memPct   = if ($memTotal -gt 0) { [Math]::Round($memUsed * 100 / $memTotal) } else { 0 }

$swapTotal = [double]$os.TotalVirtualMemorySize * 1KB
$swapFree  = [double]$os.FreeVirtualMemory      * 1KB
$swapUsed  = $swapTotal - $swapFree
$swapPct   = if ($swapTotal -gt 0) { [Math]::Round($swapUsed * 100 / $swapTotal) } else { 0 }

# CPU usage sampled over 0.5s
$cpu1 = (Get-Counter '\Processor(_Total)\% Processor Time' -SampleInterval 0.3).CounterSamples
Start-Sleep -Milliseconds 500
$cpuPct = if ($cpu1) { [Math]::Round($cpu1.CookedValue) } else { 0 }

$hostname = $env:COMPUTERNAME
$ip = ''
$net = Get-NetIPConfiguration -ErrorAction SilentlyContinue | Where-Object { $_.IPv4DefaultGateway -ne $null } | Select-Object -First 1
if ($net) { $ip = $net.IPv4Address.IPAddress }

$uptimeSec = 0
if ($os.LastBootUpTime) {
    $uptimeSec = [int]((Get-Date) - $os.LastBootUpTime).TotalSeconds
}

$cpuModel = ($cpu | Select-Object -First 1).Name
$cpuCores = ($cpu | Measure-Object NumberOfLogicalProcessors -Sum).Sum
if (-not $cpuCores) { $cpuCores = 0 }

$disks = Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3'
$diskLines = @()
$fsLines = @()
foreach ($d in $disks) {
    $size = [double]$d.Size
    $free = [double]$d.FreeSpace
    $used = $size - $free
    $pct  = if ($size -gt 0) { [Math]::Round($used * 100 / $size) } else { 0 }
    $sizeStr = if ($size -ge 1GB) { ('{0:N1} GB' -f ($size / 1GB)) } elseif ($size -ge 1MB) { ('{0:N1} MB' -f ($size / 1MB)) } else { ('{0:N0} KB' -f ($size / 1KB)) }
    $usedStr = if ($used -ge 1GB) { ('{0:N1} GB' -f ($used / 1GB)) } elseif ($used -ge 1MB) { ('{0:N1} MB' -f ($used / 1MB)) } else { ('{0:N0} KB' -f ($used / 1KB)) }
    $diskLines += ('{0}|{1}/{2}' -f $d.DeviceID, $usedStr, $sizeStr)
    $fsLines   += ('{0}|{1:N0} KB|{2:N0} KB|{3}|{4:N0} KB|{5}' -f $d.DeviceID, $size / 1KB, $used / 1KB, $pct, $free / 1KB, $d.DeviceID)
}

$procs = Get-Process | Sort-Object -Property WS -Descending | Select-Object -First 20
$procLines = @()
foreach ($p in $procs) {
    $memMB = [Math]::Round($p.WorkingSet64 / 1MB, 1)
    $cpuT  = if ($p.CPU) { [Math]::Round($p.CPU, 1) } else { 0 }
    $procLines += ('{0}M|{1}|0|{2}' -f $memMB, $cpuT, $p.ProcessName)
}

$ifaces = (Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' } | Select-Object -ExpandProperty Name) -join ','
$rx1 = 0; $tx1 = 0
$ifStats = @{}
foreach ($i in (Get-NetAdapterStatistics -ErrorAction SilentlyContinue)) {
    $ifStats[$i.Name] = @{ rx = $i.ReceivedBytes; tx = $i.SentBytes }
    $rx1 += $i.ReceivedBytes
    $tx1 += $i.SentBytes
}
Start-Sleep -Milliseconds 500
$rx2 = 0; $tx2 = 0
$ifRates = @()
foreach ($i in (Get-NetAdapterStatistics -ErrorAction SilentlyContinue)) {
    $rx2 += $i.ReceivedBytes
    $tx2 += $i.SentBytes
    $prev = $ifStats[$i.Name]
    if ($prev) {
        $rxRate = ($i.ReceivedBytes - $prev.rx) * 2
        $txRate = ($i.SentBytes   - $prev.tx) * 2
        $ifRates += ('{0}|{1}|{2}|{3}|{4}' -f $i.Name, $i.ReceivedBytes, $i.SentBytes, $rxRate, $txRate)
    }
}
$rxRate = ($rx2 - $rx1) * 2
$txRate = ($tx2 - $tx1) * 2

Write-Output ('__PLATFORM__windows')
Write-Output ('__OS__' + $os.Caption)
Write-Output ('__KERNEL_NAME__Windows')
Write-Output ('__KERNEL_VERSION__' + $os.Version)
Write-Output ('__ARCH__' + $env:PROCESSOR_ARCHITECTURE)
Write-Output ('__HOSTNAME__' + $hostname)
Write-Output ('__IP__' + $ip)
Write-Output '__UPTIME__'
Write-Output ('__UPTIME_SECONDS__' + $uptimeSec)
Write-Output '__LOAD__-'
Write-Output ('__CPU__' + $cpuPct)
Write-Output ('__CPU_USAGE__{0}|{1}|0|{2}|0|0|0|0' -f $cpuPct, $cpuPct, [Math]::Max(0, 100 - $cpuPct))
Write-Output ('__MEM__{0}|{1}|{2}|0|0|0' -f [Math]::Round($memUsed / 1MB), [Math]::Round($memTotal / 1MB), $memPct)
Write-Output ('__MEM_BYTES__{0}|{1}|{2}|{3}|0|0|0' -f $memUsed, $memTotal, $memFree, $memPct)
Write-Output ('__SWAP__{0}|{1}|{2}' -f [Math]::Round($swapUsed / 1MB), [Math]::Round($swapTotal / 1MB), $swapPct)
Write-Output ('__SWAP_BYTES__{0}|{1}|{2}|{3}' -f $swapUsed, $swapTotal, $swapFree, $swapPct)
Write-Output '__CPUINFO_START__'
Write-Output ('{0}|{1}|-|-|-' -f $cpuModel, $cpuCores)
Write-Output '__CPUINFO_END__'
Write-Output '__GPUINFO_START__'
Write-Output '__GPUINFO_END__'
Write-Output ('__IFACES__' + $ifaces)
Write-Output '__ACTIVE_IFACE__all'
Write-Output ('__RATES__{0}|{1}' -f $rxRate, $txRate)
Write-Output '__IFACE_RATES_START__'
$ifRates | ForEach-Object { Write-Output $_ }
Write-Output '__IFACE_RATES_END__'
Write-Output '__DISK_START__'
$diskLines | ForEach-Object { Write-Output $_ }
Write-Output '__DISK_END__'
Write-Output '__FILESYSTEMS_START__'
$fsLines | ForEach-Object { Write-Output $_ }
Write-Output '__FILESYSTEMS_END__'
Write-Output '__PROCS_START__'
$procLines | ForEach-Object { Write-Output $_ }
Write-Output '__PROCS_END__'
Write-Output '__FILETERM_METRICS_COMPLETE__'
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::{build_posix_metrics_command, parse_system_metrics};

    #[test]
    fn posix_metrics_command_emits_real_awk_line_breaks() {
        let command = build_posix_metrics_command("linux");

        assert!(command.contains(r#"printf "%s|%sK/%sK\n"#));
        assert!(command.contains(r#"printf "%.1fM|%s|%s|%s\n"#));
        assert!(!command.contains(r#"printf "%s|%sK/%sK\\n"#));
        assert!(!command.contains(r#"printf "%.1fM|%s|%s|%s\\n"#));
    }

    #[test]
    fn parser_keeps_disk_and_process_rows_separate() {
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__DISK_START__\n/|10K/20K\n/dev|30K/40K\n__DISK_END__\n__PROCS_START__\n1.0M|0.1|4|systemd\n2.0M|0.2|5|sshd\n__PROCS_END__\n",
            "linux",
        );

        assert_eq!(metrics["diskRows"].as_array().map(Vec::len), Some(2));
        assert_eq!(metrics["topProcesses"].as_array().map(Vec::len), Some(2));
        assert_eq!(metrics["topProcesses"][0]["command"], "sshd");
    }
}
