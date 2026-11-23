//! SPDX-License-Identifier: GPL-3.0-or-later
//! Taskbar theme, small-icon size and dark context menus for the tray.

use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{w, PCSTR};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};

/// `true` when the taskbar uses the light theme (`SystemUsesLightTheme` = 1). Missing value
/// (older Windows): dark, as Windows 10 shipped.
pub fn taskbar_uses_light_theme() -> bool {
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: reads one DWORD into `value`; `size` holds the buffer size.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    r.is_ok() && value == 1
}

/// Pixel size of small icons (tray) at the current DPI: 16 at 100 %, 24 at 150 %, …
pub fn small_icon_size() -> u32 {
    // SAFETY: plain query without pointers.
    let px = unsafe { GetSystemMetrics(SM_CXSMICON) };
    if px > 0 {
        px as u32
    } else {
        16
    }
}

/// Thread id of the tray thread, to wake it on a theme change (0 = no tray).
static TRAY_THREAD: AtomicU32 = AtomicU32::new(0);
/// Thread message to the tray thread: the theme changed, redraw the icon.
pub const WM_APP_THEME: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 2;

pub fn set_tray_thread(id: u32) {
    TRAY_THREAD.store(id, Ordering::Relaxed);
}

/// Called by the hidden broadcast window on `WM_SETTINGCHANGE` "ImmersiveColorSet".
pub fn notify_theme_changed() {
    let id = TRAY_THREAD.load(Ordering::Relaxed);
    if id != 0 {
        // SAFETY: posting a message to a thread id; fails harmlessly if it is gone.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                id,
                WM_APP_THEME,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}

/// Let context menus follow the Windows app theme (dark menus in dark mode). Uses the
/// undocumented uxtheme exports `SetPreferredAppMode` (ordinal 135, Windows 10 1809+) and
/// `FlushMenuThemes` (136), loaded by ordinal like muda does for its dark menu bar. When they
/// are missing nothing happens and menus stay light.
pub fn allow_dark_menus() {
    const ALLOW_DARK: i32 = 1;
    type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
    type FlushMenuThemes = unsafe extern "system" fn();
    // SAFETY: the function pointers are only called when the exports exist and have the
    // documented-by-usage signatures; the library stays loaded while they are called.
    unsafe {
        let Ok(lib) = LoadLibraryW(w!("uxtheme.dll")) else {
            return;
        };
        let set = GetProcAddress(lib, PCSTR(135 as *const u8));
        let flush = GetProcAddress(lib, PCSTR(136 as *const u8));
        if let Some(set) = set {
            let set: SetPreferredAppMode = std::mem::transmute(set);
            set(ALLOW_DARK);
            if let Some(flush) = flush {
                let flush: FlushMenuThemes = std::mem::transmute(flush);
                flush();
            }
            tracing::debug!(target: "awayd::tray", "dark context menus allowed");
        }
        // The library handle is kept on purpose: the preference lives in uxtheme's state and
        // must not be unloaded with it.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_icon_size_is_plausible() {
        let s = small_icon_size();
        assert!((16..=64).contains(&s), "{s}");
    }

    #[test]
    fn theme_query_does_not_fail() {
        // Value depends on the machine; the call itself must work.
        let _ = taskbar_uses_light_theme();
    }
}
