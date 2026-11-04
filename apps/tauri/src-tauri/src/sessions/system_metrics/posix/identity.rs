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
    include!("tests/posix_identity_tests.rs");
}
