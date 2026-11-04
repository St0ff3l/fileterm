pub(super) const CONSUMER_CPU_LIMIT_100NS: u64 = 500_000;
pub(super) const HELPER_CPU_LIMIT_100NS: u64 = 2_000_000;

pub(super) fn exceeds_cpu_budget(used: Option<u64>, limit: u64) -> bool {
    used.is_none_or(|used| used > limit)
}

#[cfg(target_os = "windows")]
pub(super) fn process_cpu() -> Option<u64> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return None;
    }
    let ticks =
        |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    Some(ticks(kernel).saturating_add(ticks(user)))
}

#[cfg(test)]
mod tests;
