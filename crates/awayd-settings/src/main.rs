//! SPDX-License-Identifier: GPL-3.0-or-later
#![windows_subsystem = "windows"]

mod app;
mod app_icon;
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
        // Already open (e.g. second click on the tray icon): show that window instead.
        bring_existing_window_to_front();
        return;
    }
    let start_page = page_argument(std::env::args().skip(1));
    let prefs = SettingsUiPrefs::load();
    // The stored size is the window's native size. The window is created before the app sets
    // its zoom (egui-winit uses the context's zoom, still 1.0, here), so it is used as is;
    // dividing by the zoom made the window open that much too small every time.
    // Sidebar plus a usable content column as minimum.
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([prefs.width, prefs.height])
        .with_min_inner_size([ui_prefs::MIN_WIDTH, ui_prefs::MIN_HEIGHT])
        .with_icon(std::sync::Arc::new(app_icon::icon_data()));
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
        Box::new(move |cc| Ok(Box::new(app::AwaydSettingsApp::new(cc, prefs, start_page)))),
    ) {
        eprintln!("awayd-settings: {e}");
        std::process::exit(1);
    }
}

/// Value of `--page <name>` (e.g. from the tray: "services"); unknown names are ignored.
fn page_argument(mut args: impl Iterator<Item = String>) -> Option<String> {
    while let Some(a) = args.next() {
        if a == "--page" {
            return args.next();
        }
    }
    None
}

/// Restore and focus the window of the settings instance that is already running.
fn bring_existing_window_to_front() {
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    // SAFETY: plain window lookups and activation by handle.
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, w!("awayd Settings")) {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_argument_is_read() {
        let args = |v: &[&str]| {
            v.iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .into_iter()
        };
        assert_eq!(
            page_argument(args(&["--page", "services"])),
            Some("services".into())
        );
        assert_eq!(page_argument(args(&["--page"])), None);
        assert_eq!(page_argument(args(&[])), None);
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
