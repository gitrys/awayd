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
pub fn running_processes() -> Result<Vec<(String, u32)>, String> {
    let mut out = Vec::new();
    scan_processes(|name, pid| {
        out.push((name.to_string(), pid));
        true
    })?;
    Ok(out)
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
    fn snapshot_lists_own_pid() {
        let own = std::process::id();
        let list = running_processes().unwrap();
        assert!(list.iter().any(|(_, pid)| *pid == own), "own pid {own}");
    }
}
