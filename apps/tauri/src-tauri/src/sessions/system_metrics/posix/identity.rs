/// Keep display names separate from machine-readable distro IDs. Derivatives
/// often omit their base distro's name from PRETTY_NAME (e.g. Pop!_OS).
fn build_posix_os_identity_script() -> &'static str {
    r#"os_release_file=/etc/os-release
[ -r "$os_release_file" ] || os_release_file=/usr/lib/os-release
os_release_identity=$( (
  unset PRETTY_NAME NAME ID ID_LIKE
  . "$os_release_file" >/dev/null 2>&1 && printf "%s\n%s\n%s\n" "${PRETTY_NAME:-$NAME}" "$ID" "$ID_LIKE"
) 2>/dev/null )
os_name=$(printf "%s\n" "$os_release_identity" | sed -n '1p')
os_id=$(printf "%s\n" "$os_release_identity" | sed -n '2p')
os_id_like=$(printf "%s\n" "$os_release_identity" | sed -n '3p')
[ -z "$os_name" ] && os_name=$(sed -n 's/^DISTRIB_DESCRIPTION=['"'"'"]\{0,1\}\(.*\)['"'"'"]\{0,1\}$/\1/p' /etc/openwrt_release 2>/dev/null | head -n 1)
[ -z "$os_name" ] && os_name=$(uname -s 2>/dev/null)
"#
}

#[cfg(all(test, unix))]
mod posix_identity_tests {
    use super::{build_posix_os_identity_script, parse_system_metrics};

    #[test]
    fn os_release_collects_ids_precedence_vendor_fallback_and_name_fallback() {
        struct FixtureDirectory(std::path::PathBuf);
        impl Drop for FixtureDirectory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let directory = FixtureDirectory(std::env::temp_dir().join(format!(
            "fileterm-os-identity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
        std::fs::create_dir_all(&directory.0).unwrap();
        let etc_release = directory.0.join("etc-os-release");
        let vendor_release = directory.0.join("vendor-os-release");
        let openwrt_release = directory.0.join("openwrt-release");
        let quote = |path: &std::path::Path| {
            format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
        };
        let script = build_posix_os_identity_script()
            .replace("/etc/os-release", &quote(&etc_release))
            .replace("/usr/lib/os-release", &quote(&vendor_release))
            .replace("/etc/openwrt_release", &quote(&openwrt_release));
        for (etc, vendor, expected_name, expected_id, expected_parents) in [
            (
                Some("PRETTY_NAME='Pop!_OS 24.04'\nID=pop\nID_LIKE='ubuntu debian'\n"),
                Some("PRETTY_NAME=NixOS\nID=nixos\n"),
                "Pop!_OS 24.04", "pop", "ubuntu debian",
            ),
            (None, Some("PRETTY_NAME=NixOS\nID=nixos\n"), "NixOS", "nixos", ""),
            (
                Some("NAME='Custom derivative'\nID=custom\nID_LIKE=debian\n"),
                None, "Custom derivative", "custom", "debian",
            ),
            (None, None, "Linux", "", ""),
        ] {
            for (path, contents) in [(&etc_release, etc), (&vendor_release, vendor)] {
                if let Some(contents) = contents {
                    std::fs::write(path, contents).unwrap();
                } else {
                    let _ = std::fs::remove_file(path);
                }
            }
            let command = format!(
                "uname() {{ printf 'Linux\\n'; }}\n{script}\nprintf '__OS__%s\\n__OS_ID__%s\\n__OS_ID_LIKE__%s\\n' \"$os_name\" \"$os_id\" \"$os_id_like\""
            );
            let output = std::process::Command::new("sh")
                .args(["-c", &command])
                .env("PRETTY_NAME", "inherited-display-name")
                .env("ID", "inherited-id")
                .env("ID_LIKE", "inherited-family")
                .output()
                .unwrap();
            assert!(output.status.success());
            let parsed = parse_system_metrics(&String::from_utf8(output.stdout).unwrap(), "linux");
            assert_eq!(parsed["identity"]["osName"], expected_name);
            assert_eq!(parsed["identity"]["osId"], expected_id);
            assert_eq!(parsed["identity"]["osIdLike"], expected_parents);
        }
    }
}
