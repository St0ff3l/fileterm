#[cfg(test)]
mod tests {
    use super::{
        append_pty_prompt_window, build_freebsd_metrics_command, build_posix_metrics_command,
        build_windows_metrics_command, build_windows_streaming_metrics_command,
        build_windows_streaming_metrics_exec_command, classify_posix_probe_body,
        classify_windows_probe_output, extend_with_cap, parse_system_metrics,
        pty_password_prompt_detected, EXEC_COMMAND_OUTPUT_CAP,
    };

    #[test]
    fn bounded_exec_output_marks_when_remote_data_is_discarded() {
        let mut output = vec![b'x'; EXEC_COMMAND_OUTPUT_CAP - 2];
        let mut capped = false;

        extend_with_cap(&mut output, b"abcd", &mut capped);
        extend_with_cap(&mut output, b"ignored", &mut capped);

        assert_eq!(output.len(), EXEC_COMMAND_OUTPUT_CAP);
        assert!(capped);
        assert_eq!(&output[EXEC_COMMAND_OUTPUT_CAP - 2..], b"ab");
    }

    #[test]
    fn pty_password_prompt_detection_handles_fragmented_prompts() {
        let mut window = Vec::new();
        append_pty_prompt_window(&mut window, b"Pass");
        assert!(!pty_password_prompt_detected(&window));
        append_pty_prompt_window(&mut window, b"word: ");
        assert!(pty_password_prompt_detected(&window));
        assert!(pty_password_prompt_detected("请输入密码：".as_bytes()));
        assert!(!pty_password_prompt_detected(b"uid=0(root) gid=0(root)"));
    }

    #[test]
    fn posix_metrics_command_emits_real_awk_line_breaks() {
        let command = build_posix_metrics_command("linux");

        assert!(command.contains(r#"printf "%s|%sK/%sK\n"#));
        // 进程输出格式：pid|user|rss(M)|pcpu(已归一化)|pmem|args
        assert!(command.contains(r#"printf "%s|%s|%.1fM|%.1f|%s|%s\n"#));
        assert!(command.contains("getconf _NPROCESSORS_ONLN"));
        assert!(command.contains("for (row_index = 1; row_index <= model_count; row_index++)"));
        assert!(!command.contains("for (index = 1; index <= model_count; index++)"));
        assert!(!command.contains(r#"printf "%s|%sK/%sK\\n"#));
        assert!(!command.contains(r#"printf "%.1fM|%s|%s|%s\\n"#));
    }

    #[test]
    fn posix_metrics_command_collects_amd_and_intel_drm_runtime_metrics() {
        let command = build_posix_metrics_command("linux");

        assert!(command.contains("nvidia_gpu_info"));
        assert!(command.contains("gpu_busy_percent"));
        assert!(command.contains("mem_info_vram_total"));
        assert!(command.contains("mem_info_vram_used"));
        assert!(command.contains("power1_average"));
        assert!(command.contains("power1_cap"));
        assert!(command.contains("0x1002|0X1002"));
        assert!(command.contains("0x8086|0X8086"));
        assert!(command.contains("i915/xe"));
        assert!(command.contains("intel_gpu_top -J -s 1000 -o - -d"));
    }

    #[cfg(unix)]
    #[test]
    fn posix_metrics_command_is_valid_sh_syntax() {
        let status = std::process::Command::new("sh")
            .args(["-n", "-c", &build_posix_metrics_command("linux")])
            .status()
            .expect("shell syntax checker should start");

        assert!(
            status.success(),
            "generated POSIX metrics script is invalid"
        );
    }

    #[test]
    fn freebsd_metrics_command_uses_base_system_interfaces() {
        let command = build_freebsd_metrics_command();

        assert!(command.contains("__PLATFORM__freebsd"));
        assert!(command.contains("kern.cp_time"));
        assert!(command.contains("hw.physmem"));
        assert!(command.contains("swapinfo -k"));
        assert!(command.contains("df -kP"));
        assert!(command.contains("ps -axo pid=,user=,rss=,pcpu=,pmem=,command="));
        assert!(command.contains("rctl -u"));
        assert!(command.contains("rctl -l"));
        assert!(command.contains("quota -v -f"));
        assert!(command.contains("memoryuse"));
        assert!(command.contains("pcpu"));
        assert!(command.contains("maxproc"));
        assert!(!command.contains("/proc/stat"));
        assert!(!command.contains("/proc/meminfo"));
    }

    #[test]
    fn freebsd_metrics_command_prefers_hosted_account_limits_when_available() {
        let command = build_freebsd_metrics_command();

        // Serv00 and similar hosted FreeBSD environments expose account
        // quotas separately from host-wide sysctl/df values. The command
        // remains optional and falls back to the base-system collectors when
        // the provider command or its output is unavailable.
        assert!(command.contains("devil info limits"));
        assert!(command.contains("account_limits"));
        assert!(command.contains("account quota|$account_disk_total_display"));
        assert!(command.contains("account_scope"));
        assert!(command.contains("current_user"));
        assert!(command.contains("ram memory"));
    }

    #[cfg(unix)]
    #[test]
    fn freebsd_metrics_command_emits_account_values_from_devil_fixture() {
        let script = format!(
            r#"devil() {{
  printf '%s\n' 'Disk quota: [=====] 0.42% (13.0M/3.0G)'
  printf '%s\n' 'Processes: [=====] 15.00% (3/20)'
  printf '%s\n' 'RAM memory: [=====] 7.31% (37.4M/512.0M)'
  printf '%s\n' 'CPU: [=====] 0.00% (0.0/100)'
}}
rctl() {{ return 127; }}
quota() {{ return 127; }}
{}
"#,
            build_freebsd_metrics_command()
        );
        let output = std::process::Command::new("sh")
            .args(["-c", &script])
            .output()
            .expect("FreeBSD metrics fixture should start");

        assert!(
            output.status.success(),
            "fixture script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("__CPU__0.0\n"));
        assert!(stdout.contains("__MEM_BYTES__39216742|536870912|497654170|7.30|0|0|0\n"));
        assert!(stdout.contains("|13.0M/3.0G\n"));
        assert!(stdout.contains("account quota|3.0G|13.0M|0.42%|3.0G|"));

        let metrics = parse_system_metrics(&stdout, "freebsd");
        assert_eq!(metrics["cpuPercent"], 0.0);
        assert_eq!(metrics["memoryRaw"]["totalBytes"], 536870912.0);
        assert_eq!(metrics["memoryRaw"]["usedBytes"], 39216742.0);
        assert_eq!(metrics["diskRows"][0]["usage"], "13.0 MB/3.0 GB");
        assert_eq!(metrics["fileSystemRows"][0]["name"], "account quota");
        assert_eq!(metrics["fileSystemRows"][0]["usagePercent"], "0.42%");
    }

    #[cfg(unix)]
    #[test]
    fn freebsd_metrics_command_prefers_standard_rctl_and_quota_values() {
        let script = format!(
            r#"rctl() {{
  if [ "$1" = "-u" ]; then
    printf '%s\n' 'user:fixture:memoryuse=67108864' 'user:fixture:pcpu=12.5' 'user:fixture:swapuse=1048576' 'user:fixture:maxproc=3'
  else
    printf '%s\n' 'user:fixture:memoryuse:deny=134217728/user' 'user:fixture:memoryuse:deny=1048576/process' 'user:fixture:pcpu:deny=25/user' 'user:fixture:swapuse:deny=4194304/user' 'user:fixture:maxproc:deny=20/user'
  fi
}}
quota() {{
  printf '%s\n' 'Disk quotas for user fixture:'
  printf '%s\n' 'Filesystem usage quota limit grace files quota limit grace'
  printf '%s\n' '/account 256 1024 2048 - 3 0 0 -'
}}
devil() {{
  printf '%s\n' 'Disk quota: [=====] 90.00% (9.0G/10.0G)'
  printf '%s\n' 'RAM memory: [=====] 90.00% (9.0G/10.0G)'
  printf '%s\n' 'CPU: [=====] 90.00% (90/100)'
}}
{}
"#,
            build_freebsd_metrics_command()
        );
        let output = std::process::Command::new("sh")
            .args(["-c", &script])
            .output()
            .expect("FreeBSD standard account fixture should start");

        assert!(
            output.status.success(),
            "fixture script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("__CPU__12.5\n"));
        assert!(stdout.contains("__MEM_BYTES__67108864|134217728|67108864|50.00|0|0|0\n"));
        assert!(stdout.contains("__SWAP_BYTES__1048576|4194304|3145728|25.0\n"));
        assert!(stdout.contains("/account|256.0K/2.0M\n"));

        let metrics = parse_system_metrics(&stdout, "freebsd");
        assert_eq!(metrics["cpuPercent"], 12.5);
        assert_eq!(metrics["memoryRaw"]["totalBytes"], 134217728.0);
        assert_eq!(metrics["memoryRaw"]["usedBytes"], 67108864.0);
        assert_eq!(metrics["swapRaw"]["totalBytes"], 4194304.0);
        assert_eq!(metrics["fileSystemRows"][0]["name"], "account quota");
        assert_eq!(metrics["fileSystemRows"][0]["mountPoint"], "/account");
        assert_eq!(metrics["fileSystemRows"][0]["usagePercent"], "12.50%");
    }

    #[cfg(unix)]
    #[test]
    fn freebsd_metrics_command_is_valid_sh_syntax() {
        let status = std::process::Command::new("sh")
            .args(["-n", "-c", &build_freebsd_metrics_command()])
            .status()
            .expect("shell syntax checker should start");

        assert!(
            status.success(),
            "generated FreeBSD metrics script is invalid"
        );
    }

    #[test]
    fn posix_metrics_command_samples_instantaneous_process_cpu() {
        // 进程 CPU 必须使用 /proc tick 增量，不能依赖 ps 的生命周期平均值。
        let command = build_posix_metrics_command("linux");

        assert!(command.contains("read_process_ticks()"));
        assert!(command.contains("process_ticks_before_file"));
        assert!(command.contains("process_ticks_after_file"));
        assert!(command.contains("process_cpu_tmp_file"));
        assert!(command.contains("delta * 100 / diff_total"));
        assert!(command.contains("delta > diff_total"));
        assert!(
            command.contains("($1 in before) && delta >= 0")
                || command.contains("if (!($1 in before)) next")
        );
        assert!(command.contains("ps -eo pid=,user=,rss=,pmem=,args="));
        assert!(command.contains("rank<=40 && rank<=row_count"));
        assert!(command.contains("if (comm == \"ps\" || comm == \"awk\""));
        assert!(
            !command.contains("cpu_pct=(logical_cpu_count + 0 > 0) ? $4 / logical_cpu_count : $4")
        );
        assert!(command.contains("cpu=cpu/logical_cpu_count"));
        assert!(command.contains(r#"printf "%s|%s|%.1fM|%.1f|%s|%s\n""#));
    }

    #[test]
    fn parser_keeps_disk_and_process_rows_separate() {
        // 新格式：pid|user|rss(M)|pcpu|pmem|args
        // 解析器按输入顺序保留行；构造采集命令时由 shell 端按瞬时 CPU 排序。
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__DISK_START__\n/|10K/20K\n/dev|30K/40K\n__DISK_END__\n__PROCS_START__\n1|root|1.0M|0.1|0.5|/usr/lib/systemd/systemd\n2|root|2.0M|0.2|1.0|/usr/sbin/sshd -D\n__PROCS_END__\n",
            "linux",
        );

        assert_eq!(metrics["diskRows"].as_array().map(Vec::len), Some(2));
        assert_eq!(metrics["topProcesses"].as_array().map(Vec::len), Some(2));
        // 按到达顺序，第一行是 systemd
        assert_eq!(
            metrics["topProcesses"][0]["command"],
            "/usr/lib/systemd/systemd"
        );
        assert_eq!(metrics["topProcesses"][0]["pid"], 1);
        assert_eq!(metrics["topProcesses"][0]["user"], "root");
        assert_eq!(metrics["topProcesses"][0]["cpu"], "0.1");
    }

    #[test]
    fn parser_backfills_legacy_disk_rows_from_filesystem_rows() {
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__FILESYSTEMS_START__\n/dev/sda1|100 GB|40 GB|40%|60 GB|/\n__FILESYSTEMS_END__\n",
            "linux",
        );

        assert_eq!(metrics["diskRows"][0]["path"], "/");
        assert_eq!(metrics["diskRows"][0]["usage"], "60 GB/100 GB");
    }

    #[test]
    fn parser_filters_transient_collector_processes() {
        // ps/awk/bash 等采集器自身进程应被过滤，不显示给用户
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__PROCS_START__\n100|root|1.0M|0.1|0.5|/usr/bin/sleep 1\n101|root|2.0M|0.2|1.0|/usr/sbin/nginx -g 'daemon off;'\n102|root|1.5M|0.3|0.8|ps -eo pid=,user=,rss=,pcpu=,pmem=,args= --sort=-pcpu\n__PROCS_END__\n",
            "linux",
        );

        let procs = metrics["topProcesses"].as_array().unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0]["command"], "/usr/sbin/nginx -g 'daemon off;'");
    }

    #[test]
    fn parser_rejects_invalid_or_unbounded_process_cpu_samples() {
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__PROCS_START__\n100|root|1.0M|40666.7|0.5|/usr/bin/bad-sample\n101|root|2.0M|12.3|1.0|/usr/bin/valid\n102|root|3.0M|NaN|1.0|/usr/bin/nan-sample\n__PROCS_END__\n",
            "linux",
        );

        let procs = metrics["topProcesses"].as_array().unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0]["pid"], 101);
        assert_eq!(procs[0]["cpu"], "12.3");
    }

    #[test]
    fn windows_metrics_command_emits_electron_compatible_load() {
        let command = build_windows_metrics_command();

        assert!(command.contains("($cpuPct * $logicalProcessorCount) / 100"));
        assert!(command.contains("Write-Output ('__LOAD__' + $systemLoad)"));
        assert!(command.contains("Write-Output '__LOAD_UNIT__busy-logical-processors'"));
        assert!(command.contains("Get-CpuUsagePercent"));
        assert!(!command.contains("-SampleInterval 0.3"));
        assert!(command.contains("Get-CimInstance Win32_VideoController"));
        assert!(command.contains("utilization.gpu"));
        assert!(command.contains("memory.used"));
        assert!(command.contains("temperature.gpu"));
        assert!(command.contains("$rows += ('{0}|{1}|{2}|{3}|{4}|{5}|{6}|{7}|{8}'"));
        assert!(command.contains("return $rows"));
        assert!(command.contains("$processor.L3CacheSize"));
        assert!(command.contains("$fsLines   +="));
        assert!(command.contains("prefer its runtime total"));
        assert!(command.contains("N/?A"));

        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__LOAD__1.25\n__LOAD_UNIT__busy-logical-processors\n",
            "windows",
        );
        assert_eq!(metrics["load"], "1.25");
        assert_eq!(metrics["loadUnit"], "busy-logical-processors");
    }

    #[test]
    fn parser_keeps_windows_static_hardware_and_filesystem_rows() {
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__CPUINFO_START__\n12th Gen Intel(R) Core(TM) i7-12700H|20|2300|L2 11.5 MB / L3 24.0 MB|-\n__CPUINFO_END__\n__GPUINFO_START__\nNVIDIA GeForce RTX 3070 Laptop GPU|NVIDIA|32.0.16.1047|4.0 GB\n__GPUINFO_END__\n__FILESYSTEMS_START__\nC:|400.1 GB|345.6 GB|86%|54.5 GB|C:\n__FILESYSTEMS_END__\n",
            "windows",
        );

        assert_eq!(metrics["cpuInfoRows"][0]["frequencyMHz"], "2300");
        assert_eq!(
            metrics["cpuInfoRows"][0]["cache"],
            "L2 11.5 MB / L3 24.0 MB"
        );
        assert_eq!(metrics["gpuInfoRows"][0]["vendor"], "NVIDIA");
        assert_eq!(metrics["gpuInfoRows"][0]["memory"], "4.0 GB");
        assert_eq!(metrics["fileSystemRows"][0]["mountPoint"], "C:");
        assert_eq!(metrics["fileSystemRows"][0]["usagePercent"], "86%");
    }

    #[test]
    fn parser_keeps_optional_gpu_runtime_metrics() {
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__GPUINFO_START__\nRTX 4090|NVIDIA|550.54|8.0 GB|75|4096 MiB|64 C|120.0 W|200.0 W\n__GPUINFO_END__\n",
            "linux",
        );

        let gpu = &metrics["gpuInfoRows"][0];
        assert_eq!(gpu["model"], "RTX 4090");
        assert_eq!(gpu["usagePercent"], 75.0);
        assert_eq!(gpu["memory"], "8.0 GB");
        assert_eq!(gpu["memoryUsed"], "4.0 GB");
        assert_eq!(gpu["memoryPercent"], 50.0);
        assert_eq!(gpu["temperatureCelsius"], 64.0);
        assert_eq!(gpu["powerUsage"], "120.0 W");
        assert_eq!(gpu["powerLimit"], "200.0 W");
    }

    #[test]
    fn parser_accepts_windows_gpu_units_and_prefers_runtime_vram_total() {
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__GPUINFO_START__\nNVIDIA GeForce RTX 3070 Laptop GPU|NVIDIA|32.0.16.1047|8192 MiB|49 %|920 MiB|49 C|19.37 W|-\n__GPUINFO_END__\n",
            "windows",
        );

        let gpu = &metrics["gpuInfoRows"][0];
        assert_eq!(gpu["usagePercent"], 49.0);
        assert_eq!(gpu["memory"], "8.0 GB");
        assert_eq!(gpu["memoryUsed"], "920.0 MB");
        assert_eq!(gpu["temperatureCelsius"], 49.0);
        assert_eq!(gpu["powerUsage"], "19.37 W");
        assert!(gpu["powerLimit"].is_null());
    }

    #[test]
    fn windows_streaming_metrics_reuses_warm_counters_on_a_fixed_clock() {
        let command = build_windows_streaming_metrics_command(1);

        assert!(command.contains("Diagnostics.PerformanceCounter('Processor'"));
        assert!(command.contains("$processCpuPct = if"));
        assert!(command
            .contains("'{0}||{1}M|{2}|0|{3}' -f $_.Id, $memMB, $processCpuPct, $_.ProcessName"));
        assert!(command.contains("Write-Output ('__CPU__' + $cpuPct)"));
        assert!(command.contains("$nextEmitMs += 1000"));
        assert!(command.contains("while ($true)"));
        assert!(command.matches("__FILETERM_METRICS_BLOCK__").count() >= 2);
        assert!(!command.contains("while ($true) {\n\n$ErrorActionPreference"));

        let low_frequency_command = build_windows_streaming_metrics_command(30);
        assert!(low_frequency_command.contains("$nextEmitMs += 30000"));

        let exec_command = build_windows_streaming_metrics_exec_command(1).unwrap();
        assert!(exec_command.len() < 8000);
        assert!(exec_command.contains("IO.Compression.GzipStream"));
    }

    #[test]
    fn parser_parses_windows_process_lines() {
        // Windows 发射端格式：pid||rss(M)|pcpu|pmem|ProcessName
        // 6 字段，user 为空，pmem 为 0
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__PROCS_START__\n1234||256.5M|12.3|0|chrome\n5678||128.0M|5.0|0|code\n__PROCS_END__\n",
            "windows",
        );

        let procs = metrics["topProcesses"].as_array().unwrap();
        assert!(
            !procs.is_empty(),
            "Windows top processes should not be empty"
        );
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[0]["pid"], 1234);
        assert_eq!(procs[0]["command"], "chrome");
        assert_eq!(procs[0]["cpu"], "12.3");
        assert_eq!(procs[1]["pid"], 5678);
        assert_eq!(procs[1]["command"], "code");
    }

    #[test]
    fn parser_rejects_malformed_windows_process_lines() {
        // Regression for S1: the original Windows emitter produced 4-field
        // rows (memMB|cpuT|0|ProcessName) while the parser required ≥6
        // fields (pid|user|rss|pcpu|pmem|args). Malformed rows must be
        // dropped silently rather than crash the parser, and well-formed
        // rows in the same block must still come through.
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__PROCS_START__\n256.5|12.3|0|chrome\n1234||256.5M|12.3|0|code\n__PROCS_END__\n",
            "windows",
        );

        let procs = metrics["topProcesses"].as_array().unwrap();
        assert_eq!(
            procs.len(),
            1,
            "malformed 4-field row must be dropped, well-formed 6-field row must survive"
        );
        assert_eq!(procs[0]["pid"], 1234);
        assert_eq!(procs[0]["command"], "code");
    }

    #[test]
    fn parser_handles_empty_windows_process_block() {
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__PROCS_START__\n__PROCS_END__\n",
            "windows",
        );
        assert_eq!(
            metrics["topProcesses"].as_array().map(Vec::len),
            Some(0),
            "empty process block must parse to an empty list, not null"
        );
    }

    #[test]
    fn posix_probe_classifies_linux_and_busybox_variants() {
        assert_eq!(classify_posix_probe_body("Linux\n"), Some("linux"));
        // CRLF pollution is normalized by the caller, but the classifier is
        // tolerant of stray case differences.
        assert_eq!(classify_posix_probe_body("LINUX\n"), Some("linux"));
        assert_eq!(classify_posix_probe_body("busybox\n"), Some("busybox"));
        assert_eq!(classify_posix_probe_body("OpenWrt\n"), Some("busybox"));
    }

    #[test]
    fn posix_probe_classifies_freebsd() {
        assert_eq!(classify_posix_probe_body("FreeBSD\n"), Some("freebsd"));
        assert_eq!(classify_posix_probe_body("freebsd 14.3\n"), Some("freebsd"));
    }

    #[test]
    fn posix_probe_classifies_darwin_so_macos_keeps_cwd_tracking() {
        // Regression for M1: without a darwin branch macOS remotes fell through
        // to the Windows probes and ended up as `unknown`, skipping the
        // POSIX CWD hook on the primary development platform.
        assert_eq!(classify_posix_probe_body("Darwin\n"), Some("darwin"));
        assert_eq!(classify_posix_probe_body("darwin\n"), Some("darwin"));
    }

    #[test]
    fn posix_probe_returns_none_for_unrecognized_bodies() {
        assert_eq!(classify_posix_probe_body(""), None);
        assert_eq!(classify_posix_probe_body("sunos\n"), None);
    }

    #[test]
    fn parser_accepts_freebsd_metrics_identity_and_rows() {
        let metrics = parse_system_metrics(
            "__PLATFORM__freebsd\n__OS__FreeBSD 14.3-RELEASE-p17\n__KERNEL_NAME__FreeBSD\n__KERNEL_VERSION__14.3-RELEASE-p17\n__ARCH__amd64\n__CPU__12.5\n__CPUINFO_START__\nIntel(R) Xeon(R) Platinum|2|2499|-|-\n__CPUINFO_END__\n__FILESYSTEMS_START__\n/dev/da0p2|100 GB|20 GB|20%|80 GB|/\n__FILESYSTEMS_END__\n",
            "unknown",
        );

        assert_eq!(metrics["platform"], "freebsd");
        assert_eq!(metrics["identity"]["osName"], "FreeBSD 14.3-RELEASE-p17");
        assert_eq!(metrics["identity"]["kernelName"], "FreeBSD");
        assert_eq!(metrics["cpuPercent"], 12.5);
        assert_eq!(metrics["cpuInfoRows"][0]["cores"], 2);
        assert_eq!(metrics["fileSystemRows"][0]["mountPoint"], "/");
    }

    #[test]
    fn windows_probe_recognizes_ver_and_powershell_outputs() {
        assert_eq!(
            classify_windows_probe_output("Microsoft Windows [Version 10.0.19045.4291]"),
            Some("windows")
        );
        assert_eq!(classify_windows_probe_output("Win32NT"), Some("windows"));
        assert_eq!(classify_windows_probe_output("win32nt"), Some("windows"));
        assert_eq!(classify_windows_probe_output("linux\n"), None);
    }

    #[test]
    fn parser_tolerates_missing_block_end_marker() {
        // 远端脚本被截断、网络中断或 PTY 缓冲区超限时，结束标记可能丢失。
        // read_block 应取从起始标记到字符串结尾的内容作为容错数据，
        // 而不是静默返回空，导致整段采集结果丢失。
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__DISK_START__\n/|10K/20K\n/dev|30K/40K\n__DISK_END__\n__PROCS_START__\n1|root|1.0M|0.1|0.5|/usr/lib/systemd/systemd\n2|root|2.0M|0.2|1.0|/usr/sbin/sshd -D\n",
            "linux",
        );

        // __PROCS_END__ 缺失，但 topProcesses 仍应保留两条已采集记录
        assert_eq!(metrics["topProcesses"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            metrics["topProcesses"][0]["command"],
            "/usr/lib/systemd/systemd"
        );
        assert_eq!(metrics["topProcesses"][1]["command"], "/usr/sbin/sshd -D");
    }
}
