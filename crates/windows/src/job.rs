// SPDX-License-Identifier: Apache-2.0
//! Job-object limits for the worker process.
//!
//! A job object is a resource-control measure, not a security sandbox. The core
//! still enforces its own row, byte and time budgets.

use crate::ffi;

pub struct JobGuard {
    handle: ffi::HANDLE,
}

impl JobGuard {
    /// Create a job with one-process, 512 MiB and kill-on-close limits, then
    /// assign `pid` to it.
    pub fn create_for_pid(pid: u32) -> Result<Self, String> {
        let handle = unsafe { ffi::CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if handle == 0 {
            return Err("CreateJobObjectW failed".to_string());
        }
        let guard = Self { handle };
        let mut info = ffi::JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = ffi::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | ffi::JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | ffi::JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        info.BasicLimitInformation.ActiveProcessLimit = 1;
        info.ProcessMemoryLimit = 512 * 1024 * 1024;
        let ok = unsafe {
            ffi::SetInformationJobObject(
                guard.handle,
                ffi::JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<ffi::JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            return Err("SetInformationJobObject failed".to_string());
        }
        let process = unsafe {
            ffi::OpenProcess(
                ffi::PROCESS_SET_QUOTA | ffi::PROCESS_TERMINATE,
                0,
                pid,
            )
        };
        if process == 0 {
            return Err("OpenProcess for the worker failed".to_string());
        }
        let assigned = unsafe { ffi::AssignProcessToJobObject(guard.handle, process) };
        unsafe {
            ffi::CloseHandle(process);
        }
        if assigned == 0 {
            return Err("AssignProcessToJobObject failed".to_string());
        }
        Ok(guard)
    }

    pub fn terminate(&self) {
        unsafe {
            ffi::TerminateJobObject(self.handle, 1);
        }
    }
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        unsafe {
            ffi::TerminateJobObject(self.handle, 1);
            ffi::CloseHandle(self.handle);
        }
    }
}
