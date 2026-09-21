#[cfg(test)]
mod process_selection_tests {
    use super::parse_system_metrics;
    #[cfg(unix)]
    use super::{build_posix_metrics_command, select_posix_process_rows_script};

    #[cfg(unix)]
    #[test]
    fn bounded_union_preserves_each_global_top_40() {
        let rows = (1..=200)
            .map(|pid| {
                format!(
                    "{pid}|user|{}.0M|{:.1}|0|cmd-{:03}",
                    201 - pid,
                    pid as f64 / 10.0,
                    pid * 73 % 201
                )
            })
            .collect::<Vec<_>>();
        let output = std::process::Command::new("sh")
            .args([
                "-c",
                &format!(
                    "procs=\"$FILETERM_TEST_ROWS\"\n{}\nprintf '%s\\n' \"$procs\"",
                    select_posix_process_rows_script()
                ),
            ])
            .env("FILETERM_TEST_ROWS", rows.join("\n"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let selected = String::from_utf8(output.stdout).unwrap();
        let selected_rows = selected.lines().collect::<Vec<_>>();
        assert!(selected_rows.len() <= 120);
        assert!(selected_rows.len() > 40);
        for row in rows.iter().take(40).chain(rows.iter().rev().take(40)) {
            assert!(
                selected_rows.contains(&row.as_str()),
                "missing CPU/memory candidate: {row}"
            );
        }
        let mut command_rows = rows.iter().collect::<Vec<_>>();
        command_rows.sort_by_key(|row| row.splitn(6, '|').last().unwrap());
        for row in command_rows.iter().take(40) {
            assert!(
                selected_rows.contains(&row.as_str()),
                "missing command candidate: {row}"
            );
        }
        let commands = selected_rows
            .iter()
            .map(|row| row.splitn(6, '|').last().unwrap())
            .collect::<Vec<_>>();
        assert!(commands.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[cfg(unix)]
    #[test]
    fn reused_pid_does_not_invalidate_other_cpu_samples() {
        let command = build_posix_metrics_command("linux");
        let awk = command
            .split("if awk -F'|' -v diff_total=\"$diff_total\" '\n")
            .nth(1)
            .unwrap()
            .split("' \"$process_ticks_before_file\"")
            .next()
            .unwrap();
        let root =
            std::env::temp_dir().join(format!("fileterm-process-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let before = root.join("before");
        let after = root.join("after");
        std::fs::write(&before, "1|100|123\n2|10|124\n3|10|125\n4|10|126\n").unwrap();
        std::fs::write(&after, "1|200|123\n2|30|999\n3|5000|125\n4|10|126\n").unwrap();
        let output = std::process::Command::new("awk")
            .args(["-F|", "-v", "diff_total=3200", awk])
            .arg(&before)
            .arg(&after)
            .output()
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(output.status.success());
        // 100 ticks on a 32-core machine's 3200 ticks = 3.125%, not 100%.
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "1|3.1250\n4|0.0000\n"
        );
    }

    #[test]
    fn script_workloads_keep_whole_machine_cpu() {
        let metrics = parse_system_metrics("__PLATFORM__linux\n__CPU__50\n__PROCS_START__\n1|root|1M|25|0|bash busy.sh\n2|root|2M|25|0|awk busy.awk\n__PROCS_END__\n", "linux");
        let rows = metrics["topProcesses"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        let sum = rows
            .iter()
            .map(|row| row["cpu"].as_str().unwrap().parse::<f64>().unwrap())
            .sum::<f64>();
        assert_eq!(sum, 50.0);
    }
}
