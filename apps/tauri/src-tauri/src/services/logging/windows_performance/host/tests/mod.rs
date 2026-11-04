#[test]
fn native_environment_is_readable_without_shell_commands() {
    let host = super::snapshot();
    assert!(host["windowsBuild"].is_string(), "{host}");
    assert!(host["cpuModel"].is_string(), "{host}");
    assert!(
        host["cpuTopology"]["efficiencyClasses"].is_array(),
        "{host}"
    );
    assert!(super::power_status()["acLineStatus"].is_number());
}
