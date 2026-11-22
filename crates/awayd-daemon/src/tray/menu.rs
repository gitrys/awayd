//! SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use awayd_core::status::{StatusSnapshot, SOURCE_ERROR};
use awayd_ipc::SchedulerApi;
use chrono::{DateTime, Local, NaiveDateTime, Timelike};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tokio::runtime::Handle;

use crate::service::SchedulerService;

pub const ID_VERSION: &str = "awayd.version";
pub const ID_STATUS_MODE: &str = "awayd.status.mode";
pub const ID_STATUS_NEXT: &str = "awayd.status.next";
pub const ID_STATUS_APPLY: &str = "awayd.status.apply";
pub const ID_APPLY: &str = "awayd.apply";
pub const ID_OVR_INV: &str = "awayd.override.invisible";
pub const ID_OVR_ON: &str = "awayd.override.online";
pub const ID_OVR_CLR: &str = "awayd.override.clear";
pub const ID_SETTINGS: &str = "awayd.settings";
pub const ID_QUIT: &str = "awayd.quit";

pub struct TrayMenu {
    pub menu: Menu,
    status_mode: MenuItem,
    status_next: MenuItem,
    status_apply: MenuItem,
}

impl TrayMenu {
    pub fn build() -> Self {
        let menu = Menu::new();

        let version = MenuItem::with_id(ID_VERSION, version_line(), false, None);
        let status_mode = MenuItem::with_id(ID_STATUS_MODE, "…", false, None);
        let status_next = MenuItem::with_id(ID_STATUS_NEXT, "…", false, None);
        let status_apply = MenuItem::with_id(ID_STATUS_APPLY, "…", false, None);

        let ovr_inv = MenuItem::with_id(ID_OVR_INV, "Override: invisible", true, None);
        let ovr_on = MenuItem::with_id(ID_OVR_ON, "Override: online", true, None);
        let ovr_clr = MenuItem::with_id(ID_OVR_CLR, "Clear override", true, None);
        let settings = MenuItem::with_id(ID_SETTINGS, "Settings…", true, None);
        let apply = MenuItem::with_id(ID_APPLY, "Apply now", true, None);
        let quit = MenuItem::with_id(ID_QUIT, "Quit", true, None);

        let override_sub = Submenu::new("Override", true);
        let _ = override_sub.append(&ovr_inv);
        let _ = override_sub.append(&ovr_on);
        let _ = override_sub.append(&ovr_clr);

        let more_sub = Submenu::new("More…", true);
        let _ = more_sub.append(&apply);

        let _ = menu.append(&version);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&status_mode);
        let _ = menu.append(&status_next);
        let _ = menu.append(&status_apply);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&override_sub);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&settings);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&more_sub);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        Self {
            menu,
            status_mode,
            status_next,
            status_apply,
        }
    }

    pub fn refresh_status_lines(&self, s: &StatusSnapshot) {
        let (mode, next, apply) = format_status_lines(s, Local::now());
        self.status_mode.set_text(mode);
        self.status_next.set_text(next);
        self.status_apply.set_text(apply);
    }
}

/// First (disabled) tray menu line.
pub fn version_line() -> String {
    format!("awayd {}", awayd_core::VERSION)
}

/// Windows truncates tray tooltips at 127 characters.
const TOOLTIP_MAX: usize = 120;

/// Tray tooltip: a failed tick says "error"; a tick with warnings keeps the mode and adds the
/// first warning.
pub fn tooltip_text(s: &StatusSnapshot) -> String {
    let first = s.errors.first().map(String::as_str);
    let text = if s.source == SOURCE_ERROR {
        format!("awayd — error: {}", first.unwrap_or("status not applied"))
    } else {
        let mode = if s.work_mode { "work" } else { "off" };
        match first {
            Some(w) if !s.ok => format!("awayd — {mode} ({}) · warning: {w}", s.source),
            _ => format!("awayd — {mode} ({})", s.source),
        }
    };
    truncate_chars(&text, TOOLTIP_MAX)
}

pub fn format_status_lines(s: &StatusSnapshot, now: DateTime<Local>) -> (String, String, String) {
    if s.source == SOURCE_ERROR {
        return (
            "Error: status not applied (see Logs)".to_string(),
            format_next_line(s, now),
            format_apply_line(s),
        );
    }
    let mode_core = if s.work_mode {
        "Invisible (work)"
    } else {
        "Online (off)"
    };
    let mode = if s.ok {
        mode_core.to_string()
    } else {
        format!("! {mode_core}")
    };

    let next = format_next_line(s, now);
    let apply = format_apply_line(s);

    (mode, next, apply)
}

fn format_next_line(s: &StatusSnapshot, now: DateTime<Local>) -> String {
    let Some(ref raw) = s.next_wake else {
        return format!("Next: — · {}", s.source);
    };

    let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S") else {
        return format!("Next: {raw} · {}", s.source);
    };

    let wake = match naive.and_local_timezone(Local) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => dt,
        chrono::LocalResult::None => return format!("Next: {raw} · {}", s.source),
    };

    if wake <= now {
        return format!("Next: soon · {}", s.source);
    }

    let delta = wake - now;
    let total_mins = delta.num_minutes();
    if total_mins > 0 && total_mins <= 24 * 60 {
        let hours = total_mins / 60;
        let mins = total_mins % 60;
        let rel = if hours > 0 {
            format!("in {hours}h {mins}m")
        } else {
            format!("in {mins}m")
        };
        return format!("Next: {rel} · {}", s.source);
    }

    let time = format!("{:02}:{:02}", wake.hour(), wake.minute());
    if wake.date_naive() == now.date_naive() {
        format!("Next: {time} · {}", s.source)
    } else {
        format!("Next: {} {} · {}", wake.format("%m-%d"), time, s.source)
    }
}

fn format_apply_line(s: &StatusSnapshot) -> String {
    let mut line = match &s.last_apply {
        Some(la) if la.ok => "Last apply: OK".to_string(),
        Some(_) => "Last apply: failed".to_string(),
        None => "Last apply: —".to_string(),
    };

    if let Some(la) = &s.last_apply {
        if !la.ok {
            if let Some(ref e) = la.discord_error {
                line.push_str(&format!(" — discord: {}", truncate_chars(e, 28)));
            } else if let Some(ref e) = la.steam_error {
                line.push_str(&format!(" — steam: {}", truncate_chars(e, 28)));
            }
        }
    }

    if let Some(err) = s.errors.first() {
        let truncated = truncate_chars(err, 40);
        line.push_str(&format!(" — {truncated}"));
    }

    line
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub fn handle_event(event: MenuEvent, service: Arc<SchedulerService>, rt: Handle) {
    let id = event.id().0.clone();
    rt.spawn(async move {
        match id.as_str() {
            ID_APPLY => {
                let _ = SchedulerApi::apply_now(service.as_ref()).await;
            }
            ID_OVR_INV => {
                let _ = SchedulerApi::set_override(service.as_ref(), "invisible", 0).await;
            }
            ID_OVR_ON => {
                let _ = SchedulerApi::set_override(service.as_ref(), "online", 0).await;
            }
            ID_OVR_CLR => {
                let _ = SchedulerApi::clear_override(service.as_ref()).await;
            }
            ID_SETTINGS => {
                let _ = awayd_core::launch::spawn_settings();
            }
            ID_QUIT => {
                SchedulerApi::quit(service.as_ref());
                std::process::exit(0);
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use awayd_core::status::{LastApplyRecord, STATUS_SCHEMA_VERSION};

    fn snap(work_mode: bool, source: &str, next_wake: Option<&str>) -> StatusSnapshot {
        StatusSnapshot {
            ok: true,
            schema: STATUS_SCHEMA_VERSION,
            work_mode,
            source: source.into(),
            mode_kind: "schedule".into(),
            next_wake: next_wake.map(str::to_string),
            last_apply: Some(LastApplyRecord {
                ok: true,
                work_mode,
                source: source.into(),
                at: "2026-06-05T12:00:00".into(),
                discord_error: None,
                steam_error: None,
            }),
            errors: vec![],
            version: String::new(),
        }
    }

    #[test]
    fn format_mode_work_and_off() {
        let now = Local::now();
        let (mode, _, _) = format_status_lines(&snap(true, "schedule", None), now);
        assert_eq!(mode, "Invisible (work)");
        let (mode, _, _) = format_status_lines(&snap(false, "schedule", None), now);
        assert_eq!(mode, "Online (off)");
    }

    #[test]
    fn format_next_relative() {
        let now = Local::now();
        let wake = (now + chrono::Duration::minutes(90)).format("%Y-%m-%dT%H:%M:%S");
        let (_, next, _) =
            format_status_lines(&snap(false, "schedule", Some(&wake.to_string())), now);
        assert!(next.starts_with("Next: in "));
        assert!(next.contains('m'));
        assert!(next.contains("schedule"));
    }

    #[test]
    fn version_line_shows_package_version() {
        assert_eq!(
            version_line(),
            format!("awayd {}", env!("CARGO_PKG_VERSION"))
        );
        assert!(version_line().starts_with("awayd 0."));
    }

    #[test]
    fn failed_tick_is_shown_as_error() {
        let mut s = snap(false, SOURCE_ERROR, None);
        s.ok = false;
        s.last_apply = None;
        s.errors = vec!["load config: parse config.toml".into()];
        let (mode, _, _) = format_status_lines(&s, Local::now());
        assert_eq!(mode, "Error: status not applied (see Logs)");
        assert_eq!(
            tooltip_text(&s),
            "awayd — error: load config: parse config.toml"
        );
    }

    #[test]
    fn warning_keeps_mode_in_tooltip() {
        let mut s = snap(true, "schedule", None);
        s.ok = false;
        s.errors = vec!["discord token missing (keyring or fallback file)".into()];
        let (mode, _, _) = format_status_lines(&s, Local::now());
        assert_eq!(mode, "! Invisible (work)");
        assert_eq!(
            tooltip_text(&s),
            "awayd — work (schedule) · warning: discord token missing (keyring or fallback file)"
        );
        assert_eq!(
            tooltip_text(&snap(false, "manual_override", None)),
            "awayd — off (manual_override)"
        );
    }

    #[test]
    fn tooltip_is_truncated() {
        let mut s = snap(false, SOURCE_ERROR, None);
        s.errors = vec!["x".repeat(300)];
        let t = tooltip_text(&s);
        assert_eq!(t.chars().count(), TOOLTIP_MAX);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn format_apply_with_error() {
        let mut s = snap(true, "schedule", None);
        s.ok = false;
        s.last_apply = Some(LastApplyRecord {
            ok: false,
            work_mode: true,
            source: "schedule".into(),
            at: "2026-06-05T12:00:00".into(),
            discord_error: Some("unauthorized".into()),
            steam_error: None,
        });
        s.errors = vec!["discord token missing (keyring or fallback file)".into()];
        let (_, _, apply) = format_status_lines(&s, Local::now());
        assert!(apply.contains("failed"));
        assert!(apply.contains("discord token"));
    }
}
