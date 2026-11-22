//! SPDX-License-Identifier: GPL-3.0-or-later
//! Suspend/resume via WM_POWERBROADCAST on a hidden message-only window.

use std::sync::mpsc::Sender;

use tracing::info;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage,
    HMENU, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_POWERBROADCAST, WNDCLASSW,
};

const PBT_APMRESUMESUSPEND: WPARAM = WPARAM(7);

thread_local! {
    static RESUME_TX: std::cell::RefCell<Option<Sender<()>>> = const { std::cell::RefCell::new(None) };
}

pub fn spawn(resume_tx: Sender<()>) {
    std::thread::Builder::new()
        .name("sdss-power".into())
        .spawn(move || {
            if let Err(e) = run_loop(resume_tx) {
                tracing::warn!(target: "sdss::power", "power watcher: {e}");
            }
        })
        .ok();
}

fn run_loop(resume_tx: Sender<()>) -> Result<(), String> {
    unsafe {
        let class_name: Vec<u16> = "SdssPowerWnd\0".encode_utf16().collect();
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
    let _ = lparam;
    if msg == WM_POWERBROADCAST && wparam == PBT_APMRESUMESUSPEND {
        info!(target: "sdss::power", "resume from suspend");
        RESUME_TX.with(|cell| {
            if let Some(tx) = cell.borrow().as_ref() {
                let _ = tx.send(());
            }
        });
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
