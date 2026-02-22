use std::ffi::c_void;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Console::{GenerateConsoleCtrlEvent, CTRL_BREAK_EVENT};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, CREATE_NEW_PROCESS_GROUP,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
};

pub const SHELL_NAME: &str = "cmd.exe /C";

const STILL_ACTIVE: u32 = 259;

/// Holds a Windows Job Object handle for process group management.
pub struct ProcessGroupHandle {
    job: *mut c_void,
}

impl Drop for ProcessGroupHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.job);
        }
    }
}

// SAFETY: Job object handles are thread-safe Windows kernel objects.
unsafe impl Send for ProcessGroupHandle {}
unsafe impl Sync for ProcessGroupHandle {}

pub fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("cmd.exe");
    cmd.arg("/C").arg(command);
    cmd
}

pub fn configure_process_group(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

pub fn post_spawn_setup(child_pid: Option<u32>) -> Option<ProcessGroupHandle> {
    let pid = child_pid?;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            warn!("CreateJobObjectW failed");
            return None;
        }

        let proc_handle = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
        if proc_handle.is_null() {
            warn!(pid, "OpenProcess failed for job assignment");
            CloseHandle(job);
            return None;
        }

        let result = AssignProcessToJobObject(job, proc_handle);
        CloseHandle(proc_handle);

        if result == 0 {
            warn!(pid, "AssignProcessToJobObject failed");
            CloseHandle(job);
            return None;
        }

        Some(ProcessGroupHandle { job })
    }
}

pub async fn terminate_child(
    child: &mut tokio::process::Child,
    child_pid: Option<u32>,
    group_handle: Option<&ProcessGroupHandle>,
) {
    if let Some(pid) = child_pid {
        // Try CTRL_BREAK_EVENT first for graceful shutdown.
        unsafe {
            if GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) != 0 {
                debug!(pid, "sent CTRL_BREAK_EVENT");
            }
        }

        let grace = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        match grace {
            Ok(Ok(_status)) => {
                debug!(pid, "child exited after CTRL_BREAK");
                return;
            }
            _ => {
                debug!(pid, "child did not exit within 5s, terminating");
            }
        }
    }

    // Force terminate via job object or direct kill.
    if let Some(handle) = group_handle {
        unsafe {
            TerminateJobObject(handle.job, 1);
        }
    } else {
        let _ = child.kill().await;
    }
    let _ = child.wait().await;
}

pub fn is_process_alive(pid: u32) -> bool {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code: u32 = 0;
        let result = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        result != 0 && exit_code == STILL_ACTIVE
    }
}

pub fn identify_port_owner(_port: u16) -> Option<String> {
    // TODO: implement via GetExtendedTcpTable from Win32_NetworkManagement_IpHelper
    None
}
