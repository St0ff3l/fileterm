//! Read-only Linux socket snapshots from ss or netstat; privilege routing is owned by the SSH worker.
use regex::Regex;
use serde_json::{json, Value};

// Resolve on every collection so installing Entware takes effect on refresh.
// Avoid -H/-O: Entware ss 4.4 does not implement these newer formatting flags.
pub const COMMAND: &str = r#"LC_ALL=C sh -c '
if [ "$(uname -s)" != Linux ]; then echo FILETERM_NETWORK_UNSUPPORTED; exit 2; fi
ss_path=$(command -v ss 2>/dev/null)
if [ -z "$ss_path" ]; then
    for candidate in /opt/bin/ss /opt/sbin/ss; do
        if [ -x "$candidate" ]; then ss_path=$candidate; break; fi
    done
fi
if [ -n "$ss_path" ]; then
    "$ss_path" -n -a -t -u -p -i
elif command -v netstat >/dev/null 2>&1; then
    echo __FILETERM_NETSTAT__
    netstat -antup 2>/dev/null || netstat -antu
else
    echo FILETERM_NETWORK_SS_REQUIRED
    exit 3
fi'"#;

const MAX_SOCKETS: usize = 2000;

fn endpoint(value: &str) -> Option<(&str, u16)> {
    let (address, port) = value.rsplit_once(':')?;
    Some((
        address.trim_matches(['[', ']']),
        if port == "*" { 0 } else { port.parse().ok()? },
    ))
}

pub fn parse(output: &str, output_truncated: bool) -> Result<Value, String> {
    let process = Regex::new(r#"\("([^"]+)",pid=(\d+)"#).expect("static regex");
    let netstat = output
        .lines()
        .any(|line| line.trim() == "__FILETERM_NETSTAT__");
    let mut rows = Vec::new();
    let mut truncated = output_truncated;
    // Older ss versions put TCP counters on an indented continuation line.
    let mut lines: Vec<String> = Vec::new();
    let mut has_header = false;
    for line in output.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("Netid ") || trimmed.starts_with("Netid\t") {
            has_header = true;
            continue;
        }
        if !netstat && line.starts_with(char::is_whitespace) {
            if let Some(previous) = lines.last_mut() {
                previous.push(' ');
                previous.push_str(trimmed);
                continue;
            }
        }
        lines.push(line.to_string());
    }
    for line in &lines {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 5 || !matches!(fields[0], "tcp" | "tcp6" | "udp" | "udp6") {
            continue;
        }
        let local_index = if netstat { 3 } else { 4 };
        let Some((local_address, local_port)) =
            fields.get(local_index).and_then(|value| endpoint(value))
        else {
            continue;
        };
        let Some((remote_address, remote_port)) = fields
            .get(local_index + 1)
            .and_then(|value| endpoint(value))
        else {
            continue;
        };
        if rows.len() == MAX_SOCKETS {
            truncated = true;
            break;
        }
        let protocol = fields[0].trim_end_matches('6');
        let state = if netstat {
            if protocol == "udp" {
                if remote_port == 0 {
                    "UNCONN"
                } else {
                    "ESTAB"
                }
            } else {
                fields.get(5).copied().unwrap_or("UNKNOWN")
            }
        } else {
            fields[1]
        };
        let owner = process.captures(line);
        let netstat_owner = netstat
            .then(|| fields.last().and_then(|field| field.split_once('/')))
            .flatten();
        let pid = if netstat {
            netstat_owner.and_then(|(pid, _)| pid.parse::<u32>().ok())
        } else {
            owner.as_ref().and_then(|c| c[2].parse::<u32>().ok())
        };
        let process_name = if netstat {
            netstat_owner.map(|(_, name)| name.to_string())
        } else {
            owner.as_ref().map(|c| c[1].to_string())
        };
        let counter = |key: &str| -> Option<u64> {
            fields
                .iter()
                .find_map(|field| field.strip_prefix(key)?.parse().ok())
        };
        let sent = counter("bytes_sent:");
        let acked = counter("bytes_acked:");
        rows.push(json!({
            "protocol": protocol, "state": state,
            "localAddress": local_address, "localPort": local_port,
            "remoteAddress": remote_address, "remotePort": remote_port,
            "pid": pid,
            "process": process_name,
            "txBytes": sent.or(acked), "rxBytes": counter("bytes_received:"),
            "txBytesSource": if sent.is_some() { Some("sent") } else if acked.is_some() { Some("acked") } else { None },
            "listening": state == "LISTEN" || (protocol == "udp" && remote_port == 0),
        }));
    }
    // An empty successful ss snapshot is valid, but diagnostics must not look like an empty host.
    if rows.is_empty() && !netstat && !has_header && !output.trim().is_empty() {
        return Err("NETWORK_SNAPSHOT_INVALID".into());
    }
    Ok(
        json!({ "source": if netstat { "netstat" } else { "ss" }, "sockets": rows, "truncated": truncated }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_upload_uses_acked_bytes_without_overriding_sent_or_unknown() {
        let value = parse(concat!(
            "tcp ESTAB 0 0 10.0.0.1:22 10.0.0.2:50000\n\t cubic bytes_acked:1234 bytes_received:5678\n",
            "tcp ESTAB 0 0 10.0.0.1:23 10.0.0.2:50001 bytes_sent:2000 bytes_acked:1234\n",
            "tcp ESTAB 0 0 10.0.0.1:24 10.0.0.2:50002 bytes_sent:0 bytes_acked:100\n",
            "tcp LISTEN 0 128 *:80 *:*\n"
        ), false).unwrap();
        let sockets = &value["sockets"];
        assert_eq!(sockets[0]["txBytes"], 1234);
        assert_eq!(sockets[0]["txBytesSource"], "acked");
        assert_eq!(sockets[1]["txBytes"], 2000);
        assert_eq!(sockets[1]["txBytesSource"], "sent");
        assert_eq!(sockets[2]["txBytes"], 0);
        assert!(sockets[3]["txBytes"].is_null());
        assert!(sockets[3]["txBytesSource"].is_null());
    }

    #[test]
    fn parses_legacy_ss_headers_and_multiline_counters() {
        let header = "Netid State Recv-Q Send-Q Local Address:Port Peer Address:Port\n";
        let value = parse(&format!("{header}tcp ESTAB 0 0 10.0.0.1:22 10.0.0.2:50000 users:((\"sshd\",pid=9,fd=3))\n\t cubic bytes_sent:1024 bytes_received:2048\n"), false).unwrap();
        assert_eq!(value["sockets"][0]["txBytes"], 1024);
        assert_eq!(value["sockets"][0]["rxBytes"], 2048);
        assert_eq!(value["sockets"][0]["pid"], 9);
        assert!(parse(header, false).unwrap()["sockets"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn collector_rediscovers_entware_without_path_changes_and_prefers_system_ss() {
        use std::fs;
        use std::os::unix::fs::{symlink, PermissionsExt};
        use std::process::Command;
        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let fixture = Fixture(std::env::temp_dir().join(format!(
                "fileterm-network-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        let path = fixture.0.join("path");
        let opt_bin = fixture.0.join("opt-bin-ss");
        let opt_sbin = fixture.0.join("opt-sbin-ss");
        fs::create_dir_all(&path).unwrap();
        symlink("/bin/sh", path.join("sh")).unwrap();
        let executable = |file: &std::path::Path, content: &str| {
            fs::write(file, content).unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
        };
        executable(&path.join("uname"), "#!/bin/sh\necho Linux\n");
        executable(&path.join("netstat"), "#!/bin/sh\necho netstat-fixture\n");
        let command = COMMAND
            .replace("/opt/bin/ss", opt_bin.to_str().unwrap())
            .replace("/opt/sbin/ss", opt_sbin.to_str().unwrap());
        let run = || {
            let output = Command::new("/bin/sh")
                .arg("-c")
                .arg(&command)
                .env("PATH", &path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        };
        assert!(run().contains("__FILETERM_NETSTAT__"));
        let script = |label: &str| {
            format!("#!/bin/sh\n[ \"$*\" = \"-n -a -t -u -p -i\" ] || exit 90\necho {label}\n")
        };
        executable(&opt_sbin, &script("sbin-fixture"));
        assert_eq!(run().trim(), "sbin-fixture");
        executable(&opt_bin, &script("bin-fixture"));
        assert_eq!(run().trim(), "bin-fixture");
        executable(&path.join("ss"), &script("system-fixture"));
        assert_eq!(run().trim(), "system-fixture");
    }

    #[test]
    fn parses_ipv4_ipv6_udp_and_permission_limited_owners() {
        let value = parse(concat!(
            "tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=1188,fd=3))\n",
            "tcp ESTAB 0 0 [::ffff:10.0.0.1]:22 [::ffff:10.0.0.2]:50000 users:((\"sshd\",pid=1200,fd=4)) bytes_sent:1024 bytes_received:2048\n",
            "udp UNCONN 0 0 [::]:53 [::]:*\n"
        ), false).unwrap();
        assert_eq!(value["source"], "ss");
        assert_eq!(value["sockets"][0]["pid"], 1188);
        assert_eq!(value["sockets"][1]["remoteAddress"], "::ffff:10.0.0.2");
        assert_eq!(value["sockets"][1]["txBytes"], 1024);
        assert!(value["sockets"][2]["pid"].is_null());
        assert!(value["sockets"][2]["txBytes"].is_null());
        assert_eq!(value["sockets"][2]["listening"], true);
    }

    #[test]
    fn netstat_fallback_handles_busybox_udp_and_ipv6_without_traffic_counters() {
        let value = parse(
            concat!(
                "__FILETERM_NETSTAT__\nActive Internet connections (servers and established)\n",
                "Proto Recv-Q Send-Q Local Address Foreign Address State PID/Program name\n",
                "tcp 0 0 0.0.0.0:22 0.0.0.0:* LISTEN 1188/sshd\n",
                "tcp6 0 0 :::443 :::* LISTEN -\n",
                "tcp 0 0 192.168.1.2:22 192.168.1.3:55000 ESTABLISHED 1200/sshd\n",
                "udp 0 0 0.0.0.0:53 0.0.0.0:* 555/dnsmasq\n",
                "udp 0 0 :::53 :::*\n"
            ),
            false,
        )
        .unwrap();
        assert_eq!(value["sockets"].as_array().unwrap().len(), 5);
        assert_eq!(value["source"], "netstat");
        assert_eq!(value["sockets"][0]["process"], "sshd");
        assert_eq!(value["sockets"][1]["protocol"], "tcp");
        assert_eq!(value["sockets"][1]["localAddress"], "::");
        assert!(value["sockets"][1]["pid"].is_null());
        assert_eq!(value["sockets"][2]["state"], "ESTABLISHED");
        assert_eq!(value["sockets"][3]["pid"], 555);
        assert_eq!(value["sockets"][4]["listening"], true);
        assert!(value["sockets"][0]["txBytes"].is_null());
    }

    #[test]
    fn bounds_rows_and_distinguishes_empty_from_errors() {
        assert!(parse("", false).unwrap()["sockets"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(parse("ss: invalid option", false).is_err());
        let raw = "tcp LISTEN 0 128 *:22 *:*\n".repeat(MAX_SOCKETS + 1);
        let value = parse(&raw, false).unwrap();
        assert_eq!(value["sockets"].as_array().unwrap().len(), MAX_SOCKETS);
        assert_eq!(value["truncated"], true);
        assert_eq!(parse("", true).unwrap()["truncated"], true);
    }
}
