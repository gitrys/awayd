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
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

use crate::service::SchedulerService;

fn pump_win32_messages() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn apply_status_ui(tray: &tray_icon::TrayIcon, tray_menu: &menu::TrayMenu, s: &StatusSnapshot) {
    tray_menu.refresh_status_lines(s);
    let title = format!(
        "awayd — {} ({})",
        if s.work_mode { "work" } else { "off" },
        s.source
    );
    let _ = tray.set_tooltip(Some(&title));
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
    let _ = tray.set_show_menu_on_left_click(true);

    let mut latest: Option<StatusSnapshot> = None;

    // Apply cached status immediately (scheduler may have ticked before tray loop started).
    let json = rt.block_on(async { service.status.read().await.json.clone() });
    if !json.is_empty() {
        if let Ok(s) = serde_json::from_str::<StatusSnapshot>(&json) {
            latest = Some(s.clone());
            apply_status_ui(&tray, &tray_menu, &s);
        }
    }

    loop {
        pump_win32_messages();

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
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            menu::handle_event(event, service.clone(), rt.clone());
        }
        if let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button_state, .. } = event {
                if button_state == MouseButtonState::Up {
                    if let Some(ref s) = latest {
                        tray_menu.refresh_status_lines(s);
                    }
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
