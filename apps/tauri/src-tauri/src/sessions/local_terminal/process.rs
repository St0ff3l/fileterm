#[derive(Default)]
struct LocalProcessTree {
    #[cfg(target_os = "windows")]
    job_handle: Option<usize>,
}

impl LocalProcessTree {
    fn attach(child: &dyn portable_pty::Child) -> Self {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::{
                Foundation::{CloseHandle, HANDLE},
                System::JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                },
            };

            let Some(raw_process) = child.as_raw_handle() else {
                return Self::default();
            };
            let job_handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job_handle.is_null() {
                return Self::default();
            }

            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = unsafe {
                SetInformationJobObject(
                    job_handle,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION)
                        .cast::<std::ffi::c_void>(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) != 0
            };
            let assigned =
                unsafe { AssignProcessToJobObject(job_handle, raw_process as HANDLE) != 0 };
            if !configured || !assigned {
                unsafe {
                    CloseHandle(job_handle);
                }
                return Self::default();
            }

            Self {
                job_handle: Some(job_handle as usize),
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = child;
            Self::default()
        }
    }

    fn terminate(&self, child: &mut dyn portable_pty::Child) {
        #[cfg(target_os = "windows")]
        if let Some(job_handle) = self.job_handle {
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;

            let terminated = unsafe { TerminateJobObject(job_handle as _, 1) != 0 };
            if terminated {
                return;
            }
        }

        #[cfg(unix)]
        if let Some(pid) = child.process_id().filter(|pid| *pid > 0) {
            if pid <= i32::MAX as u32 {
                let process_group = -(pid as libc::pid_t);
                let child_alive = child.try_wait().ok().flatten().is_none();
                if child_alive {
                    unsafe {
                        libc::kill(process_group, libc::SIGHUP);
                    }
                    for _ in 0..5 {
                        if child.try_wait().ok().flatten().is_some() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(25));
                    }
                }
                unsafe {
                    libc::kill(process_group, libc::SIGKILL);
                }
                return;
            }
        }

        let _ = child.kill();
    }
}

impl Drop for LocalProcessTree {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        if let Some(job_handle) = self.job_handle.take() {
            use windows_sys::Win32::Foundation::CloseHandle;

            unsafe {
                CloseHandle(job_handle as _);
            }
        }
    }
}
