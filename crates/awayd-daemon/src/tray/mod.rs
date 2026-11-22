//! SPDX-License-Identifier: GPL-3.0-or-later
//! System tray UI (main thread).

mod icon;
mod menu;

use std::sync::Arc;

use awayd_core::status::StatusSnapshot;
use tokio::sync::watch;
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIconBuilder;
use tray_icon::{MouseButtonState, TrayIconEvent};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, WM_APP,
};

use crate::service::SchedulerService;

/// Thread message to the tray thread: the status changed.
const WM_APP_STATUS: u32 = WM_APP + 1;

fn apply_status_ui(tray: &tray_icon::TrayIcon, tray_menu: &menu::TrayMenu, s: &StatusSnapshot) {
    tray_menu.refresh_status_lines(s);
    let _ = tray.set_tooltip(Some(&menu::tooltip_text(s)));
    let _ = tray.set_icon(Some(icon::icon_for_status(s)));
}

pub fn run_tray(
    service: Arc<SchedulerService>,
    mut status_rx: watch::Receiver<String>,
    rt: tokio::runtime::Handle,
) -> anyhow::Result<()> {
    let tray_menu = menu::TrayMenu::build();
    let tray = TrayIconBuilder::new()
        .with_tooltip("awayd")
        .with_icon(icon::icon_off())
        .with_menu(Box::new(tray_menu.menu.clone()))
        .build()?;
    tray.set_show_menu_on_left_click(true);

    let mut latest: Option<StatusSnapshot> = None;

    // Apply cached status immediately (scheduler may have ticked before tray loop started).
    let json = rt.block_on(async { service.status.read().await.json.clone() });
    if !json.is_empty() {
        if let Ok(s) = serde_json::from_str::<StatusSnapshot>(&json) {
            latest = Some(s.clone());
            apply_status_ui(&tray, &tray_menu, &s);
        }
    }

    // The thread sleeps in `GetMessageW` until a window message arrives (tray click, menu)
    // instead of polling every 16 ms. Status changes come from another thread: a task posts a
    // thread message to wake it. Windows drops thread messages while a modal loop runs (tray
    // menu open), so the status channel is also checked after every other message; a change
    // during an open menu is shown at the latest when the menu closes.
    let tray_thread = unsafe { GetCurrentThreadId() };
    let mut wake_rx = status_rx.clone();
    rt.spawn(async move {
        loop {
            let closed = wake_rx.changed().await.is_err();
            // SAFETY: posting a message to a thread id; fails harmlessly if the thread is gone.
            unsafe {
                let _ = PostThreadMessageW(tray_thread, WM_APP_STATUS, WPARAM(0), LPARAM(0));
            }
            if closed {
                // One last wake-up so the tray sees the closed channel and ends.
                break;
            }
        }
    });

    let mut msg = MSG::default();
    loop {
        // SAFETY: standard Win32 message loop on the thread that owns the tray window.
        match unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 {
            0 => return Ok(()), // WM_QUIT
            -1 => anyhow::bail!("GetMessageW failed: {}", std::io::Error::last_os_error()),
            _ => unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            },
        }

        while status_rx
            .has_changed()
            .map_err(|_| anyhow::anyhow!("status channel closed"))?
        {
            let json = status_rx.borrow_and_update().clone();
            if let Ok(s) = serde_json::from_str::<StatusSnapshot>(&json) {
                latest = Some(s.clone());
                apply_status_ui(&tray, &tray_menu, &s);
            }
        }
        // Menu and tray events are queued while their window messages are dispatched.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            menu::handle_event(event, service.clone(), rt.clone());
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(ref s) = latest {
                    tray_menu.refresh_status_lines(s);
                }
            }
        }
    }
}
