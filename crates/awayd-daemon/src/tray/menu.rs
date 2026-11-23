//! SPDX-License-Identifier: GPL-3.0-or-later
//! Tray context menu and tooltip. The native Windows menu is built once; a status change
//! only updates texts, and the warning line and "Clear override" are inserted or removed
//! only when they appear or go away.

use std::cell::Cell;
use std::sync::Arc;

use awayd_core::status::{StatusSnapshot, SOURCE_ERROR};
use awayd_ipc::SchedulerApi;
use chrono::{DateTime, Local, NaiveDateTime};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tokio::runtime::Handle;

use crate::service::SchedulerService;

pub const ID_STATUS: &str = "awayd.status";
pub const ID_REASON: &str = "awayd.reason";
pub const ID_WARNING: &str = "awayd.warning";
pub const ID_OVR_CLR: &str = "awayd.override.clear";
pub const ID_APPLY: &str = "awayd.apply";
pub const ID_SETTINGS: &str = "awayd.settings";
pub const ID_QUIT: &str = "awayd.quit";
const ID_OVR_PREFIX: &str = "awayd.override.";

/// Override durations (0 = until the next change), the same as on the Overview page.
const DURATIONS: [(u32, &str); 5] = [
    (0, "Until the next change"),
    (30 * 60, "30 minutes"),
    (60 * 60, "1 hour"),
    (2 * 60 * 60, "2 hours"),
    (4 * 60 * 60, "4 hours"),
];

/// The Windows 11 taskbar cuts tray tooltips after 64 characters (observed; the API takes
/// 128), so longer texts end with "…" here instead of in the middle of a word.
const TOOLTIP_MAX: usize = 64;
const WARNING_MAX: usize = 60;

pub struct TrayMenu {
    pub menu: Menu,
    status: MenuItem,
    reason: MenuItem,
    warning: MenuItem,
    clear: MenuItem,
    warning_shown: Cell<bool>,
    clear_shown: Cell<bool>,
    /// Settings page the warning line opens.
    warning_page: Cell<&'static str>,
}

fn duration_submenu(title: &str, mode: &str) -> Submenu {
    let sub = Submenu::new(title, true);
    for (secs, label) in DURATIONS {
        let id = format!("{ID_OVR_PREFIX}{mode}.{secs}");
        let _ = sub.append(&MenuItem::with_id(id, label, true, None));
    }
    sub
}

impl TrayMenu {
    pub fn build() -> Self {
        let menu = Menu::new();
        // Enabled so Windows draws it in the normal text colour (disabled items are grey);
        // a click opens the Overview page.
        let status = MenuItem::with_id(ID_STATUS, "awayd", true, None);
        let reason = MenuItem::with_id(ID_REASON, "…", false, None);
        let warning = MenuItem::with_id(ID_WARNING, "", true, None);
        let clear = MenuItem::with_id(ID_OVR_CLR, "Clear override", true, None);

        let _ = menu.append(&status);
        let _ = menu.append(&reason);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&duration_submenu("Invisible", "invisible"));
        let _ = menu.append(&duration_submenu("Online", "online"));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id(ID_APPLY, "Apply now", true, None));
        let _ = menu.append(&MenuItem::with_id(
            ID_SETTINGS,
            "Open settings…",
            true,
            None,
        ));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id(ID_QUIT, "Quit", true, None));

        Self {
            menu,
            status,
            reason,
            warning,
            clear,
            warning_shown: Cell::new(false),
            clear_shown: Cell::new(false),
            warning_page: Cell::new("overview"),
        }
    }

    pub fn refresh(&self, s: &StatusSnapshot) {
        let lines = MenuLines::for_status(s, Local::now());
        self.status.set_text(&lines.status);
        self.reason.set_text(&lines.reason);

        // Order: status, reason, [warning], separator, Invisible ▸, Online ▸, [Clear override].
        // "Clear override" is taken out first so the warning index below stays right.
        if self.clear_shown.get() {
            let _ = self.menu.remove(&self.clear);
            self.clear_shown.set(false);
        }
        match (&lines.warning, self.warning_shown.get()) {
            (Some((text, page)), shown) => {
                self.warning.set_text(text);
                self.warning_page.set(page);
                if !shown {
                    let _ = self.menu.insert(&self.warning, 2);
                    self.warning_shown.set(true);
                }
            }
            (None, true) => {
                let _ = self.menu.remove(&self.warning);
                self.warning_shown.set(false);
            }
            (None, false) => {}
        }
        if lines.override_active {
            let at = if self.warning_shown.get() { 6 } else { 5 };
            let _ = self.menu.insert(&self.clear, at);
            self.clear_shown.set(true);
        }
    }

    /// Settings page for a click on the warning line.
    pub fn warning_page(&self) -> &'static str {
        self.warning_page.get()
    }
}

/// Texts of the dynamic menu lines.
#[derive(Debug, PartialEq, Eq)]
struct MenuLines {
    status: String,
    reason: String,
    /// Text and settings page to open.
    warning: Option<(String, &'static str)>,
    override_active: bool,
}

impl MenuLines {
    fn for_status(s: &StatusSnapshot, now: DateTime<Local>) -> Self {
        let warning = s.errors.first().map(|e| {
            (
                format!("⚠ {}", truncate_chars(e, WARNING_MAX)),
                page_for_warning(e),
            )
        });
        if s.source == SOURCE_ERROR {
            return Self {
                status: "Status unknown".into(),
                reason: "The last status could not be applied".into(),
                warning,
                override_active: false,
            };
        }
        Self {
            status: format!("{}{}", state_word(s), until_text(s, now)),
            reason: reason_text(s).into(),
            warning: if s.ok { None } else { warning },
            override_active: s.source == "manual_override",
        }
    }
}

fn state_word(s: &StatusSnapshot) -> &'static str {
    if s.work_mode {
        "Invisible"
    } else {
        "Online"
    }
}

/// " · until 17:00" / " · until Tue 09:00"; empty in network mode without override (the
/// next change depends on the device, not on a time).
fn until_text(s: &StatusSnapshot, now: DateTime<Local>) -> String {
    if s.mode_kind == "network" && s.source != "manual_override" {
        return String::new();
    }
    let Some(at) = s
        .next_wake
        .as_deref()
        .and_then(|w| NaiveDateTime::parse_from_str(w, "%Y-%m-%dT%H:%M:%S").ok())
    else {
        return String::new();
    };
    if at.date() == now.date_naive() {
        format!(" · until {}", at.format("%H:%M"))
    } else {
        format!(" · until {}", at.format("%a %H:%M"))
    }
}

fn reason_text(s: &StatusSnapshot) -> &'static str {
    match (s.source.as_str(), s.work_mode) {
        ("manual_override", _) if s.mode_kind == "network" => {
            "Override, then network presence again"
        }
        ("manual_override", _) => "Override, then work hours again",
        ("network", true) => "The work device answers",
        ("network", false) => "The work device does not answer",
        (_, true) => "Work hours",
        (_, false) => "Outside work hours, or a day off",
    }
}

/// Short reason for the tooltip.
fn reason_short(s: &StatusSnapshot) -> &'static str {
    match (s.source.as_str(), s.work_mode) {
        ("manual_override", _) => "override",
        ("network", true) => "device present",
        ("network", false) => "device absent",
        (_, true) => "work hours",
        (_, false) => "off hours",
    }
}

/// Warnings about a service's token or connection open the Services page.
fn page_for_warning(warning: &str) -> &'static str {
    let w = warning.to_ascii_lowercase();
    let about_service = awayd_core::backends::ALL
        .iter()
        .any(|b| w.contains(b.id) || w.contains(&b.display_name.to_ascii_lowercase()));
    if about_service {
        "services"
    } else {
        "overview"
    }
}

/// "awayd · Invisible until 17:00 (work hours) · Discord token missing…".
pub fn tooltip_text(s: &StatusSnapshot) -> String {
    tooltip_at(s, Local::now())
}

fn tooltip_at(s: &StatusSnapshot, now: DateTime<Local>) -> String {
    let first = s.errors.first().map(String::as_str);
    let text = if s.source == SOURCE_ERROR {
        format!(
            "awayd · status unknown · {}",
            first.unwrap_or("not applied")
        )
    } else {
        let until = until_text(s, now).replace(" · ", " ");
        match first {
            // With a warning the reason gives way, and the warning loses its explanation in
            // brackets: the space is short.
            Some(w) if !s.ok => format!("awayd · {}{until} · ⚠ {}", state_word(s), warning_core(w)),
            _ => format!("awayd · {}{until} ({})", state_word(s), reason_short(s)),
        }
    };
    truncate_chars(&text, TOOLTIP_MAX)
}

/// The part of a warning before its explanation in brackets ("Discord token missing
/// (Credential Manager or fallback file)" → "Discord token missing").
fn warning_core(w: &str) -> &str {
    w.split(" (").next().unwrap_or(w).trim()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// What a menu click asks for.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    OpenSettings(&'static str),
    OpenWarningPage,
    Override(&'static str, u32),
    ClearOverride,
    ApplyNow,
    Quit,
    None,
}

fn action_for(id: &str) -> Action {
    match id {
        ID_STATUS => Action::OpenSettings("overview"),
        ID_WARNING => Action::OpenWarningPage,
        ID_SETTINGS => Action::OpenSettings("overview"),
        ID_OVR_CLR => Action::ClearOverride,
        ID_APPLY => Action::ApplyNow,
        ID_QUIT => Action::Quit,
        other => {
            let Some(rest) = other.strip_prefix(ID_OVR_PREFIX) else {
                return Action::None;
            };
            let Some((mode, secs)) = rest.split_once('.') else {
                return Action::None;
            };
            let mode = match mode {
                "invisible" => "invisible",
                "online" => "online",
                _ => return Action::None,
            };
            secs.parse()
                .map_or(Action::None, |s| Action::Override(mode, s))
        }
    }
}

/// Open the settings on `page`; a running settings window is brought to the front instead.
pub fn open_settings(page: &str) {
    if let Err(e) = awayd_core::launch::spawn_settings_page(page) {
        tracing::warn!(target: "awayd::tray", "open settings: {e}");
    }
}

pub fn handle_event(event: MenuEvent, menu: &TrayMenu, service: Arc<SchedulerService>, rt: Handle) {
    match action_for(&event.id().0) {
        Action::OpenSettings(page) => open_settings(page),
        Action::OpenWarningPage => open_settings(menu.warning_page()),
        Action::Override(mode, secs) => {
            rt.spawn(async move {
                let _ = SchedulerApi::set_override(service.as_ref(), mode, secs).await;
            });
        }
        Action::ClearOverride => {
            rt.spawn(async move {
                let _ = SchedulerApi::clear_override(service.as_ref()).await;
            });
        }
        Action::ApplyNow => {
            rt.spawn(async move {
                let _ = SchedulerApi::apply_now(service.as_ref()).await;
            });
        }
        Action::Quit => {
            SchedulerApi::quit(service.as_ref());
            std::process::exit(0);
        }
        Action::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awayd_core::status::STATUS_SCHEMA_VERSION;
    use chrono::TimeZone;

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 23, 13, 48, 0).unwrap()
    }

    fn snap(work_mode: bool, source: &str, next_wake: Option<&str>) -> StatusSnapshot {
        StatusSnapshot {
            ok: true,
            schema: STATUS_SCHEMA_VERSION,
            work_mode,
            source: source.into(),
            mode_kind: "schedule".into(),
            next_wake: next_wake.map(str::to_string),
            ..StatusSnapshot::default()
        }
    }

    #[test]
    fn status_and_reason_lines() {
        let l = MenuLines::for_status(&snap(true, "schedule", Some("2026-11-23T17:00:00")), now());
        assert_eq!(l.status, "Invisible · until 17:00");
        assert_eq!(l.reason, "Work hours");
        assert_eq!(l.warning, None);
        assert!(!l.override_active);

        let l = MenuLines::for_status(&snap(false, "schedule", Some("2026-11-24T09:00:00")), now());
        assert_eq!(l.status, "Online · until Tue 09:00");
        assert_eq!(l.reason, "Outside work hours, or a day off");

        let l = MenuLines::for_status(
            &snap(false, "manual_override", Some("2026-11-23T15:30:00")),
            now(),
        );
        assert_eq!(l.status, "Online · until 15:30");
        assert!(l.override_active);
    }

    #[test]
    fn network_mode_has_no_until() {
        let mut s = snap(true, "network", Some("2026-11-23T14:00:00"));
        s.mode_kind = "network".into();
        let l = MenuLines::for_status(&s, now());
        assert_eq!(l.status, "Invisible");
        assert_eq!(l.reason, "The work device answers");
    }

    #[test]
    fn warning_line_and_page() {
        let mut s = snap(true, "schedule", None);
        s.ok = false;
        s.errors = vec!["Discord token missing (Credential Manager or fallback file)".into()];
        let l = MenuLines::for_status(&s, now());
        let (text, page) = l.warning.unwrap();
        assert!(text.starts_with("⚠ Discord token missing"), "{text}");
        assert!(text.chars().count() <= WARNING_MAX + 2);
        assert_eq!(page, "services");
        assert_eq!(page_for_warning("holiday download failed"), "overview");
    }

    #[test]
    fn failed_tick_is_shown_as_unknown() {
        let mut s = snap(false, SOURCE_ERROR, None);
        s.ok = false;
        s.errors = vec!["load config: parse config.toml".into()];
        let l = MenuLines::for_status(&s, now());
        assert_eq!(l.status, "Status unknown");
        assert_eq!(
            tooltip_at(&s, now()),
            "awayd · status unknown · load config: parse config.toml"
        );
    }

    #[test]
    fn tooltips_in_plain_words() {
        assert_eq!(
            tooltip_at(&snap(true, "schedule", Some("2026-11-23T17:00:00")), now()),
            "awayd · Invisible until 17:00 (work hours)"
        );
        let mut s = snap(true, "schedule", Some("2026-11-23T16:25:00"));
        s.ok = false;
        s.errors = vec!["Discord token missing (Credential Manager or fallback file)".into()];
        assert_eq!(
            tooltip_at(&s, now()),
            "awayd · Invisible until 16:25 · ⚠ Discord token missing"
        );
        // Long states and warnings still stay within what the taskbar shows.
        s.next_wake = Some("2026-11-24T09:00:00".into());
        s.errors = vec!["x".repeat(200)];
        let t = tooltip_at(&s, now());
        assert_eq!(t.chars().count(), TOOLTIP_MAX);
        assert!(t.ends_with('…'));
        let mut s = snap(false, SOURCE_ERROR, None);
        s.errors = vec!["x".repeat(300)];
        let t = tooltip_at(&s, now());
        assert_eq!(t.chars().count(), TOOLTIP_MAX);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn menu_ids_map_to_actions() {
        assert_eq!(action_for(ID_STATUS), Action::OpenSettings("overview"));
        assert_eq!(
            action_for("awayd.override.invisible.3600"),
            Action::Override("invisible", 3600)
        );
        assert_eq!(
            action_for("awayd.override.online.0"),
            Action::Override("online", 0)
        );
        assert_eq!(action_for(ID_OVR_CLR), Action::ClearOverride);
        assert_eq!(action_for("awayd.override.sideways.10"), Action::None);
        assert_eq!(action_for("awayd.override.online.x"), Action::None);
        // Every duration of both submenus maps to an override.
        for (secs, _) in DURATIONS {
            for mode in ["invisible", "online"] {
                assert_eq!(
                    action_for(&format!("{ID_OVR_PREFIX}{mode}.{secs}")),
                    Action::Override(mode, secs)
                );
            }
        }
    }
}
