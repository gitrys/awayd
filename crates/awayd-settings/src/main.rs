//! SPDX-License-Identifier: GPL-3.0-or-later
#![windows_subsystem = "windows"]

mod app;
mod background;
mod client;
mod pages;
mod theme;
mod ui_prefs;
mod widgets;
mod zoom_preview;

use ui_prefs::SettingsUiPrefs;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};

fn main() {
    awayd_core::logging::init_tracing();
    if !acquire_single_instance() {
        return;
    }
    let prefs = SettingsUiPrefs::load();
    let (inner_w, inner_h) =
        app::AwaydSettingsApp::inner_size_from_store(prefs.width, prefs.height, prefs.zoom);
    // Sidebar plus a usable content column.
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([inner_w, inner_h])
        .with_min_inner_size([640.0, 440.0]);
    if let (Some(x), Some(y)) = (prefs.pos_x, prefs.pos_y) {
        viewport = viewport.with_position([x, y]);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "awayd Settings",
        options,
        Box::new(move |cc| Ok(Box::new(app::AwaydSettingsApp::new(cc, prefs)))),
    ) {
        eprintln!("awayd-settings: {e}");
        std::process::exit(1);
    }
}

fn acquire_single_instance() -> bool {
    let name: Vec<u16> = "Global\\awayd-settings\0".encode_utf16().collect();
    unsafe {
        let mutex: HANDLE =
            CreateMutexW(None, false, windows::core::PCWSTR(name.as_ptr())).unwrap_or_default();
        if mutex.is_invalid() {
            return false;
        }
        let wait = WaitForSingleObject(mutex, 0);
        if wait == WAIT_TIMEOUT || wait != WAIT_OBJECT_0 {
            let _ = CloseHandle(mutex);
            return false;
        }
        true
    }
}
