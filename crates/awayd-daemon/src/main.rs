//! SPDX-License-Identifier: GPL-3.0-or-later
#![windows_subsystem = "windows"]

use awayd_daemon::run::{run_backend, setup};
use awayd_daemon::tray;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::System::Threading::WaitForSingleObject;

fn main() {
    let advanced = awayd_core::config::load_config(&awayd_core::config::paths::config_file())
        .map(|c| c.advanced)
        .unwrap_or_default();
    if let Err(e) = awayd_core::logging::init_daemon_logging(&advanced) {
        eprintln!("awayd logging init failed: {e:#}");
    }
    if !acquire_single_instance() {
        tracing::info!(
            target: "awayd::lifecycle",
            pid = std::process::id(),
            "another awayd instance is running; exiting"
        );
        return;
    }
    // Restarts and crashes become visible in the log: every start logs this line, every
    // orderly end "awayd stopping".
    tracing::info!(
        target: "awayd::lifecycle",
        version = awayd_core::VERSION,
        pid = std::process::id(),
        portable = awayd_core::config::paths::is_portable(),
        config = %awayd_core::config::paths::config_file().display(),
        "awayd started"
    );
    let handle = match setup() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("awayd setup failed: {e:#}");
            std::process::exit(1);
        }
    };

    let service = handle.service.clone();
    let status_rx = handle.status_rx.clone();
    let shutdown = handle.shutdown.clone();

    let (rt_tx, rt_rx) = std::sync::mpsc::sync_channel::<tokio::runtime::Handle>(1);

    let backend = std::thread::Builder::new()
        .name("awayd-tokio".into())
        .spawn(move || {
            // Two workers: awayd mostly sleeps, and blocking work (downloads, ping) runs on
            // Tokio's blocking pool. The default (one worker per core) started 32 idle
            // threads on a 32-thread CPU.
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("tokio runtime");
            let _ = rt_tx.send(rt.handle().clone());
            if let Err(e) = rt.block_on(run_backend(handle)) {
                eprintln!("awayd backend: {e:#}");
            }
        })
        .expect("spawn backend");

    let rt_handle = rt_rx.recv().expect("tokio handle");

    if let Err(e) = tray::run_tray(service, status_rx, rt_handle) {
        eprintln!("awayd tray: {e:#}");
        tracing::error!(target: "awayd::lifecycle", error = %format!("{e:#}"), "tray failed");
    }
    tracing::info!(target: "awayd::lifecycle", "awayd stopping (tray ended)");
    shutdown.cancel();
    let _ = backend.join();
}

fn acquire_single_instance() -> bool {
    // Per Windows session (`Local\`), like the pipe name: another logged-in user gets their
    // own awayd instead of none.
    let name: Vec<u16> = "Local\\awayd\0".encode_utf16().collect();
    unsafe {
        let mutex: HANDLE =
            CreateMutexW(None, false, windows::core::PCWSTR(name.as_ptr())).unwrap_or_default();
        if mutex.is_invalid() {
            return false;
        }
        let wait = WaitForSingleObject(mutex, 0);
        if wait == WAIT_TIMEOUT {
            let _ = CloseHandle(mutex);
            return false;
        }
        if wait != WAIT_OBJECT_0 {
            let _ = CloseHandle(mutex);
            return false;
        }
        true
    }
}
