//! SPDX-License-Identifier: GPL-3.0-or-later
//! Running processes from a Toolhelp snapshot (no WMI: no COM, no WMI quota).

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

/// Visit every process of one snapshot as (executable name, PID) until `visit` returns
/// `false`. `Err` when the snapshot cannot be taken.
fn scan_processes(mut visit: impl FnMut(&str, u32) -> bool) -> Result<(), String> {
    // SAFETY: plain Win32 calls; the snapshot handle is closed before returning and the
    // entry struct is initialised with its size as the API requires.
    unsafe {
        let snapshot =
            CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if !visit(&name, entry.th32ProcessID) {
                break;
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        Ok(())
    }
}

/// `true` if a process whose executable name equals `exe_name` (case-insensitive, e.g.
/// `"steam.exe"`) is running. Returns `Err` when the snapshot cannot be taken.
pub fn is_process_running(exe_name: &str) -> Result<bool, String> {
    let mut found = false;
    scan_processes(|name, _| {
        found = name.eq_ignore_ascii_case(exe_name);
        !found
    })?;
    Ok(found)
}

/// All running processes as (executable name, PID), from one snapshot.
#[cfg(test)]
pub fn running_processes() -> Result<Vec<(String, u32)>, String> {
    let mut out = Vec::new();
    scan_processes(|name, pid| {
        out.push((name.to_string(), pid));
        true
    })?;
    Ok(out)
}

/// PIDs of all running processes. Much cheaper than a Toolhelp snapshot (measured about
/// 0.03 ms against 7.6 ms with ~450 processes): no names are copied.
pub fn running_pids() -> Result<Vec<u32>, String> {
    use windows::Win32::System::ProcessStatus::EnumProcesses;
    let mut pids = vec![0u32; 1024];
    loop {
        let mut needed = 0u32;
        let bytes = (pids.len() * std::mem::size_of::<u32>()) as u32;
        // SAFETY: the buffer is valid for `bytes` bytes; the API writes at most that much.
        unsafe { EnumProcesses(pids.as_mut_ptr(), bytes, &mut needed) }
            .map_err(|e| e.to_string())?;
        // A full buffer may mean it was too small: try again with a larger one.
        if needed < bytes {
            pids.truncate(needed as usize / std::mem::size_of::<u32>());
            return Ok(pids);
        }
        pids.resize(pids.len() * 2, 0);
    }
}

/// Memory and CPU time of this process (for "Resource use" in the settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnUsage {
    pub working_set_bytes: u64,
    pub private_bytes: u64,
    /// Kernel plus user time since the process started.
    pub cpu_ms: u64,
    pub uptime_s: u64,
}

pub fn own_usage() -> Result<OwnUsage, String> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
    // SAFETY: plain queries on the current process with correctly sized out-structs.
    unsafe {
        let process = GetCurrentProcess();
        let mut mem = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        GetProcessMemoryInfo(
            process,
            &mut mem as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            mem.cb,
        )
        .map_err(|e| e.to_string())?;
        let (mut created, mut exited, mut kernel, mut user) = Default::default();
        GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user)
            .map_err(|e| e.to_string())?;
        let now = GetSystemTimeAsFileTime();
        // FILETIME ticks are 100 ns.
        Ok(OwnUsage {
            working_set_bytes: mem.WorkingSetSize as u64,
            private_bytes: mem.PrivateUsage as u64,
            cpu_ms: (ticks(kernel) + ticks(user)) / 10_000,
            uptime_s: ticks(now).saturating_sub(ticks(created)) / 10_000_000,
        })
    }
}

/// Executable file name of process `pid` (e.g. `"steam.exe"`); `None` when the process is
/// gone or may not be queried (e.g. protected system processes).
pub fn process_name(pid: u32) -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: the handle is closed before returning; the buffer outlives the call and its
    // length is passed in `len`.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_own_process_case_insensitive() {
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_name().unwrap().to_string_lossy().to_uppercase();
        assert_eq!(is_process_running(&name), Ok(true), "own exe {name}");
    }

    #[test]
    fn unknown_name_is_not_running() {
        assert_eq!(
            is_process_running("awayd-surely-not-running-7f3a.exe"),
            Ok(false)
        );
    }

    #[test]
    fn own_usage_is_plausible() {
        let u = own_usage().unwrap();
        assert!(u.working_set_bytes > 1024 * 1024, "{u:?}");
        assert!(u.private_bytes > 0, "{u:?}");
        assert!(u.uptime_s < 24 * 3600, "{u:?}");
    }

    #[test]
    fn pid_list_and_name_of_own_process() {
        let own = std::process::id();
        assert!(running_pids().unwrap().contains(&own), "own pid {own}");
        let exe = std::env::current_exe().unwrap();
        let expected = exe.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(process_name(own).as_deref(), Some(expected.as_str()));
        assert_eq!(process_name(u32::MAX - 3), None);
    }

    #[test]
    fn snapshot_lists_own_pid() {
        let own = std::process::id();
        let list = running_processes().unwrap();
        assert!(list.iter().any(|(_, pid)| *pid == own), "own pid {own}");
    }
}
