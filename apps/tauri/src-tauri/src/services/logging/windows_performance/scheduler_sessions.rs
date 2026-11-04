//! Only reclaim FileTerm sessions whose PID and process creation time prove the owner is gone.
use super::processes::OwnedHandle;
use std::mem::{offset_of, size_of};
use std::ptr::null;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_INVALID_DATA, ERROR_INVALID_PARAMETER, ERROR_WMI_INSTANCE_NOT_FOUND,
    FILETIME, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Diagnostics::Etw::*;
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE,
};

const OWNER_MARKER: u32 = 0xf17e_0002;
const PREFIX: &str = "FileTerm-Scheduler-";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Owner {
    pub pid: u32,
    pub created: u64,
}

impl Owner {
    pub fn for_process(pid: u32) -> Result<Self, u32> {
        process_creation(pid)?
            .map(|created| Self { pid, created })
            .ok_or(ERROR_INVALID_PARAMETER)
    }

    fn name(self) -> String {
        format!("{PREFIX}v2-{}-{:016x}", self.pid, self.created)
    }

    fn guid(self) -> GUID {
        GUID {
            data1: OWNER_MARKER,
            data2: self.pid as u16,
            data3: (self.pid >> 16) as u16,
            data4: self.created.to_le_bytes(),
        }
    }

    fn from_properties(properties: &Properties) -> Option<Self> {
        let guid = properties.header.Wnode.Guid;
        if guid.data1 != OWNER_MARKER {
            return None;
        }
        let owner = Self {
            pid: u32::from(guid.data2) | (u32::from(guid.data3) << 16),
            created: u64::from_le_bytes(guid.data4),
        };
        (owner.pid != 0 && owner.created != 0 && owner.name() == properties.name()).then_some(owner)
    }
}

fn process_creation(pid: u32) -> Result<Option<u64>, u32> {
    let process = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if process.is_null() {
        let error = unsafe { GetLastError() };
        return if error == ERROR_INVALID_PARAMETER {
            Ok(None)
        } else {
            Err(error)
        };
    }
    let process = OwnedHandle(process);
    match unsafe { WaitForSingleObject(process.0, 0) } {
        WAIT_OBJECT_0 => return Ok(None),
        WAIT_TIMEOUT => {}
        _ => return Err(unsafe { GetLastError() }),
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) } == 0
    {
        return Err(unsafe { GetLastError() });
    }
    Ok(Some(
        (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
    ))
}

#[repr(C)]
pub(super) struct Properties {
    pub header: EVENT_TRACE_PROPERTIES,
    logger_name: [u16; 1024],
    logfile_name: [u16; 1024],
}

impl Properties {
    fn empty() -> Self {
        let mut properties = Self {
            header: EVENT_TRACE_PROPERTIES::default(),
            logger_name: [0; 1024],
            logfile_name: [0; 1024],
        };
        properties.header.Wnode.BufferSize = size_of::<Self>() as u32;
        properties.header.LoggerNameOffset = offset_of!(Self, logger_name) as u32;
        properties.header.LogFileNameOffset = offset_of!(Self, logfile_name) as u32;
        properties
    }

    fn for_owner(owner: Owner) -> Self {
        let mut properties = Self::empty();
        properties.header.Wnode.Guid = owner.guid();
        for (slot, value) in properties
            .logger_name
            .iter_mut()
            .zip(owner.name().encode_utf16())
        {
            *slot = value;
        }
        properties
    }

    fn name(&self) -> String {
        let end = self
            .logger_name
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(self.logger_name.len());
        String::from_utf16_lossy(&self.logger_name[..end])
    }
}

pub(super) struct Session {
    handle: CONTROLTRACE_HANDLE,
    pub properties: Properties,
    stopped: bool,
}

impl Session {
    pub fn start(owner: Owner) -> Result<Self, u32> {
        let mut properties = Properties::for_owner(owner);
        properties.header.LogFileNameOffset = 0; // Real-time only; never writes a system trace to disk.
        properties.header.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        properties.header.Wnode.ClientContext = 1;
        properties.header.LogFileMode = EVENT_TRACE_SYSTEM_LOGGER_MODE
            | EVENT_TRACE_REAL_TIME_MODE
            | EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING;
        properties.header.EnableFlags =
            EVENT_TRACE_FLAG_CSWITCH | EVENT_TRACE_FLAG_DISPATCHER | EVENT_TRACE_FLAG_NO_SYSCONFIG;
        properties.header.BufferSize = 64;
        properties.header.MinimumBuffers = 16;
        properties.header.MaximumBuffers = 16;
        properties.header.FlushTimer = 1;
        let mut session = Self {
            handle: CONTROLTRACE_HANDLE::default(),
            properties,
            stopped: true,
        };
        let error = unsafe {
            StartTraceW(
                &mut session.handle,
                session.properties.logger_name.as_ptr(),
                &mut session.properties.header,
            )
        };
        if error != 0 {
            return Err(error);
        }
        session.stopped = false;
        Ok(session)
    }

    pub fn name(&self) -> [u16; 1024] {
        self.properties.logger_name
    }

    pub fn stop(&mut self) -> u32 {
        if self.stopped {
            return 0;
        }
        let error = unsafe {
            ControlTraceW(
                self.handle,
                null(),
                &mut self.properties.header,
                EVENT_TRACE_CONTROL_STOP,
            )
        };
        self.stopped = matches!(error, 0 | ERROR_WMI_INSTANCE_NOT_FOUND);
        error
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) fn cleanup_current(owner: Owner) -> u32 {
    let mut properties = Properties::for_owner(owner);
    let error = unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            properties.logger_name.as_ptr(),
            &mut properties.header,
            EVENT_TRACE_CONTROL_QUERY,
        )
    };
    if error != 0 {
        return error;
    }
    if Owner::from_properties(&properties) != Some(owner) {
        return ERROR_INVALID_DATA;
    }
    stop_properties(&mut properties)
}

fn stop_properties(properties: &mut Properties) -> u32 {
    let handle = CONTROLTRACE_HANDLE {
        Value: unsafe { properties.header.Wnode.Anonymous1.HistoricalContext },
    };
    if handle.Value == 0 {
        return ERROR_INVALID_DATA;
    }
    // Stop the queried instance, rather than a name that another controller could replace.
    unsafe {
        ControlTraceW(
            handle,
            null(),
            &mut properties.header,
            EVENT_TRACE_CONTROL_STOP,
        )
    }
}

pub(super) fn cleanup_session(root: u32) -> u32 {
    Owner::for_process(root)
        .map(cleanup_current)
        .unwrap_or_else(|error| error)
}

pub(super) fn recover_orphans() -> Result<usize, u32> {
    // One bounded retry handles hosts with extra ETW loggers; incomplete results fail closed.
    let mut properties: Vec<_> = (0..64).map(|_| Properties::empty()).collect();
    let mut pointers: Vec<_> = properties
        .iter_mut()
        .map(|item| &mut item.header as *mut _)
        .collect();
    let mut count = 0;
    let mut error =
        unsafe { QueryAllTracesW(pointers.as_mut_ptr(), pointers.len() as u32, &mut count) };
    if error == windows_sys::Win32::Foundation::ERROR_MORE_DATA && count > 64 && count <= 256 {
        properties = (0..count).map(|_| Properties::empty()).collect();
        pointers = properties
            .iter_mut()
            .map(|item| &mut item.header as *mut _)
            .collect();
        error =
            unsafe { QueryAllTracesW(pointers.as_mut_ptr(), pointers.len() as u32, &mut count) };
    }
    if error != 0 {
        return Err(error);
    }
    let mut recovered = 0;
    for properties in properties.iter_mut().take(count as usize) {
        if !properties.name().starts_with(PREFIX) {
            continue;
        }
        // A matching prefix alone never grants permission to stop an unrelated trace.
        let owner = Owner::from_properties(properties).ok_or(ERROR_INVALID_DATA)?;
        if process_creation(owner.pid)? == Some(owner.created) {
            continue;
        }
        let error = stop_properties(properties);
        if !matches!(error, 0 | ERROR_WMI_INSTANCE_NOT_FOUND) {
            return Err(error);
        }
        recovered += 1;
    }
    Ok(recovered)
}

#[cfg(test)]
mod tests;
