//! SPDX-License-Identifier: GPL-3.0-or-later
//! Resume from standby and system clock/time zone changes, via broadcasts to a hidden
//! top-level window (broadcasts are not delivered to message-only windows). Each event asks
//! the scheduler to recompute and re-apply.

use std::sync::mpsc::Sender;

use tracing::info;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, HMENU, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_POWERBROADCAST,
    WM_SETTINGCHANGE, WM_TIMECHANGE, WNDCLASSW,
};

/// Resume after a user action (input).
const PBT_APMRESUMESUSPEND: WPARAM = WPARAM(7);
/// Resume of any kind, also without input (wake timer, Wake-on-LAN); sent before
/// `PBT_APMRESUMESUSPEND` when the user is present.
const PBT_APMRESUMEAUTOMATIC: WPARAM = WPARAM(0x12);

/// Why a window message should trigger a re-apply (`None`: ignore).
fn wake_reason(msg: u32, wparam: WPARAM) -> Option<&'static str> {
    match msg {
        WM_POWERBROADCAST if wparam == PBT_APMRESUMEAUTOMATIC => Some("resume (automatic)"),
        WM_POWERBROADCAST if wparam == PBT_APMRESUMESUSPEND => Some("resume (user)"),
        WM_TIMECHANGE => Some("system time changed"),
        _ => None,
    }
}

/// `WM_SETTINGCHANGE` with the area "ImmersiveColorSet" (light/dark theme changed).
fn is_color_set_change(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    // SAFETY: for WM_SETTINGCHANGE a non-null lParam points to a NUL-terminated wide string.
    let area = unsafe { windows::core::PCWSTR(lparam.0 as *const u16).to_string() };
    area.is_ok_and(|a| a == "ImmersiveColorSet")
}

thread_local! {
    static RESUME_TX: std::cell::RefCell<Option<Sender<()>>> = const { std::cell::RefCell::new(None) };
}

pub fn spawn(resume_tx: Sender<()>) {
    std::thread::Builder::new()
        .name("awayd-power".into())
        .spawn(move || {
            if let Err(e) = run_loop(resume_tx) {
                tracing::warn!(target: "awayd::power", "power watcher: {e}");
            }
        })
        .ok();
}

fn run_loop(resume_tx: Sender<()>) -> Result<(), String> {
    unsafe {
        let class_name: Vec<u16> = "AwaydPowerWnd\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let _hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            HMENU::default(),
            None,
            None,
        )
        .map_err(|e| e.to_string())?;

        RESUME_TX.with(|cell| *cell.borrow_mut() = Some(resume_tx));

        let mut msg = MSG::default();
        loop {
            let r = GetMessageW(&mut msg, None, 0, 0);
            if r.0 == 0 || r.0 == -1 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(())
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Light/dark taskbar switched: the tray redraws its icon (no re-apply needed).
    if msg == WM_SETTINGCHANGE && is_color_set_change(lparam) {
        super::theme::notify_theme_changed();
    }
    if let Some(reason) = wake_reason(msg, wparam) {
        info!(target: "awayd::power", reason, "{reason}");
        RESUME_TX.with(|cell| {
            if let Some(tx) = cell.borrow().as_ref() {
                let _ = tx.send(());
            }
        });
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_and_time_change_trigger_other_messages_do_not() {
        assert_eq!(
            wake_reason(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC),
            Some("resume (automatic)")
        );
        assert_eq!(
            wake_reason(WM_POWERBROADCAST, PBT_APMRESUMESUSPEND),
            Some("resume (user)")
        );
        assert_eq!(
            wake_reason(WM_TIMECHANGE, WPARAM(0)),
            Some("system time changed")
        );
        // PBT_APMSUSPEND (4): going to sleep is no reason to re-apply.
        assert_eq!(wake_reason(WM_POWERBROADCAST, WPARAM(4)), None);
        assert_eq!(wake_reason(0x0400, WPARAM(0)), None);
    }
}
