//! SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use awayd_core::autostart::{self, AutostartState};
use awayd_core::backends::{self, BackendInfo};
use awayd_core::config::paths;
use awayd_core::config::schema::{AppConfig, VacationEntry};
use awayd_core::StatusSnapshot;
use chrono::{Datelike, NaiveDate};
use eframe::egui;

use crate::client::Ipc;
use crate::pages::logs_page::{
    log_popout_viewport_id, show_logs_docked_placeholder, show_logs_embedded_tab, show_logs_full,
    LogViewState, LOG_POPOUT_DEFAULT_SIZE,
};
use crate::ui_prefs::SettingsUiPrefs;
use crate::widgets::date_range_picker::{show_date_range_field, DateRangeFieldState};
use crate::widgets::holiday_list_flyout::{HolidayFlyoutAction, HolidayListFlyout};
use crate::widgets::searchable_picker::SearchablePicker;
use crate::widgets::subdiv_labels::{format_subdiv_label, subdiv_options_for_country};
use crate::widgets::weekday_schedule_grid::{show_weekday_schedule_grid, WeekdayScheduleGridState};
use crate::zoom_preview;
use awayd_core::schedule::{normalize_schedule, schedule_days};

const CREDENTIAL_STORED_PLACEHOLDER: &str = "••••••••••••";
const STATUS_SLOT_H: f32 = 24.0;
const TOAST_FADE_IN: f64 = 0.15;
const TOAST_HOLD: f64 = 0.55;
const TOAST_FADE_OUT: f64 = 0.25;
const TOAST_GAP: f64 = 0.08;
const UI_PREFS_DEBOUNCE_SECS: f64 = 0.5;
/// Ignore resize tracking until zoom/layout settle after startup.
const WINDOW_TRACK_GRACE_SECS: f64 = 0.75;
/// Require stable inner size before committing to prefs (avoids one-frame spikes).
const WINDOW_SIZE_STABLE_SECS: f64 = 0.25;
/// 1.5× egui default slider width (100).
const SETTINGS_SLIDER_WIDTH: f32 = 150.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusKind {
    Success,
    Error,
}

#[derive(Clone, Copy)]
enum ToastPhase {
    FadeIn { start: f64 },
    Hold { until: f64 },
    FadeOut { start: f64 },
    Gap { until: f64 },
}

struct StatusState {
    text: String,
    kind: StatusKind,
    phase: Option<ToastPhase>,
}

/// Input state of one backend credential (e.g. the Discord token).
#[derive(Default)]
struct CredentialField {
    input: String,
    /// The daemon reports a stored value.
    stored: bool,
    editing: bool,
}

/// Backends that need a credential, in table order.
fn credential_backends() -> impl Iterator<Item = &'static BackendInfo> {
    backends::ALL
        .iter()
        .filter(|b| b.credential_label.is_some())
}

struct VacationRangeUi {
    range: DateRangeFieldState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Scheduler,
    Display,
    Logs,
}

pub struct AwaydSettingsApp {
    ipc: Ipc,
    rt: tokio::runtime::Runtime,
    wizard: bool,
    wizard_step: u32,
    /// Per backend id (only backends with a credential).
    credentials: HashMap<&'static str, CredentialField>,
    config: AppConfig,
    status: Option<StatusState>,
    frame_time: f64,
    countries: Vec<(String, String)>,
    subdivs: Vec<String>,
    subdiv_by_country: HashMap<String, String>,
    country_picker: SearchablePicker,
    subdiv_picker: SearchablePicker,
    holiday_flyout: HolidayListFlyout,
    schedule_grid: WeekdayScheduleGridState,
    vacation_ranges: Vec<VacationRangeUi>,
    vac_draft_label: String,
    vac_draft_start: String,
    vac_draft_end: String,
    vac_draft_range_state: DateRangeFieldState,
    daemon_ok: bool,
    ui_prefs: SettingsUiPrefs,
    ui_prefs_save_at: Option<f64>,
    zoom_applied: f32,
    zoom_pending: f32,
    zoom_dragging: bool,
    /// Re-apply saved zoom/viewport once egui reports inner size (see `apply_startup_prefs`).
    startup_prefs_pending: bool,
    startup_resize_attempts: u8,
    startup_position_attempts: u8,
    window_track_after: f64,
    tracked_geometry: Option<(f32, f32, f32, f32)>,
    tracked_geometry_since: f64,
    active_tab: SettingsTab,
    log_view: Option<LogViewState>,
    log_popout_open: bool,
    /// Set when an existing `config.toml` could not be read; blocks saving so the shown
    /// defaults never overwrite the real file.
    config_error: Option<String>,
    /// Login autostart entry as last read from the registry (`Err` = could not be read).
    autostart_state: Result<AutostartState, String>,
    /// Version reported by the running daemon (see `refresh_daemon_version`).
    daemon_version: Option<String>,
}

/// Decide which config the UI may edit.
///
/// - `Ok(None)`: no `config.toml` yet (first start): keep the defaults, saving is allowed.
/// - `Ok(Some(cfg))`: file read and valid.
/// - `Err(msg)`: file exists but could not be read or is invalid: saving must stay blocked.
fn resolve_loaded_config(
    file_exists: bool,
    text: Result<String, String>,
) -> Result<Option<AppConfig>, String> {
    if !file_exists {
        return Ok(None);
    }
    let raw = text?;
    awayd_core::config::parse_config(&raw)
        .map(Some)
        .map_err(|e| format!("{e:#}"))
}

/// Header text for the daemon, e.g. "daemon online (v0.1.0)".
fn daemon_status_label(online: bool, daemon_version: Option<&str>) -> String {
    match (online, daemon_version) {
        (false, _) => "daemon offline".to_string(),
        (true, Some(v)) => format!("daemon online (v{v})"),
        (true, None) => "daemon online (version unknown)".to_string(),
    }
}

/// Hint when the running daemon is a different build than this settings window (e.g. the
/// files were updated but awayd.exe was not restarted).
fn version_mismatch_hint(daemon_version: Option<&str>) -> Option<String> {
    let v = daemon_version?;
    (v != awayd_core::VERSION).then(|| {
        format!(
            "The running awayd.exe is v{v}, this settings window is v{}. Restart awayd (tray → Quit, then start awayd.exe).",
            awayd_core::VERSION
        )
    })
}

/// Next free vacation id `v<n>`: one above the highest number in use, so the id of a deleted
/// entry is never given to another one (the id keys the entry's date pickers; duplicates
/// shared their state, and daemon messages named an ambiguous entry).
fn next_vacation_id(entries: &[VacationEntry]) -> String {
    let next = entries
        .iter()
        .filter_map(|v| v.id.strip_prefix('v')?.parse::<u64>().ok())
        .max()
        .map_or(0, |n| n + 1);
    format!("v{next}")
}

/// Give every entry whose id was already used by an earlier one a new unique id.
/// Returns true when something changed.
fn dedupe_vacation_ids(entries: &mut [VacationEntry]) -> bool {
    let mut changed = false;
    for i in 0..entries.len() {
        if entries[..i].iter().any(|e| e.id == entries[i].id) {
            entries[i].id = next_vacation_id(entries);
            changed = true;
        }
    }
    changed
}

/// The daemon executable next to this settings binary (same lookup as `awayctl`).
fn awayd_exe() -> std::path::PathBuf {
    awayd_core::launch::resolve_awayd_binary("awayd")
}

enum ConfigErrorAction {
    Reload,
    BackupAndStartOver,
}

impl AwaydSettingsApp {
    pub fn new(cc: &eframe::CreationContext<'_>, ui_prefs: SettingsUiPrefs) -> Self {
        let zoom = ui_prefs.zoom;
        cc.egui_ctx.set_zoom_factor(zoom);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("settings runtime");
        let config = AppConfig::default();
        let schedule_days = schedule_days(&config.schedule);
        let initial_year = chrono::Local::now().date_naive().year();
        let mut subdiv_by_country = HashMap::new();
        subdiv_by_country.insert(
            config.schedule.country.clone(),
            config.schedule.subdiv.clone(),
        );
        let mut app = Self {
            ipc: Ipc::new(),
            rt,
            wizard: !paths::config_file().is_file(),
            wizard_step: 0,
            credentials: credential_backends()
                .map(|b| (b.id, CredentialField::default()))
                .collect(),
            schedule_grid: WeekdayScheduleGridState::from_days(&schedule_days),
            config,
            status: None,
            frame_time: 0.0,
            countries: vec![("DE".into(), "Germany".into())],
            subdivs: vec!["BW".into()],
            subdiv_by_country,
            country_picker: SearchablePicker::new(),
            subdiv_picker: SearchablePicker::new(),
            holiday_flyout: HolidayListFlyout::new(initial_year),
            vacation_ranges: Vec::new(),
            vac_draft_label: String::new(),
            vac_draft_start: String::new(),
            vac_draft_end: String::new(),
            vac_draft_range_state: DateRangeFieldState::from_range("", ""),
            daemon_ok: false,
            ui_prefs,
            ui_prefs_save_at: None,
            zoom_applied: zoom,
            zoom_pending: zoom,
            zoom_dragging: false,
            startup_prefs_pending: true,
            startup_resize_attempts: 0,
            startup_position_attempts: 0,
            window_track_after: f64::MAX,
            tracked_geometry: None,
            tracked_geometry_since: 0.0,
            active_tab: SettingsTab::Scheduler,
            log_view: None,
            log_popout_open: false,
            config_error: None,
            autostart_state: Ok(AutostartState::Off),
            daemon_version: None,
        };
        app.reload();
        app
    }

    fn country_options(&self) -> Vec<(String, String)> {
        self.countries
            .iter()
            .map(|(code, name)| (code.clone(), format!("{name} ({code})")))
            .collect()
    }

    fn subdiv_options(&self) -> Vec<(String, String)> {
        subdiv_options_for_country(&self.config.schedule.country, &self.subdivs)
            .into_iter()
            .map(|o| (o.code, o.label))
            .collect()
    }

    fn reload_subdivs_for_country(&mut self) {
        let cc = self.config.schedule.country.clone();
        if self.daemon_ok {
            if let Ok(s) = self.rt.block_on(self.ipc.list_subdivisions(&cc)) {
                self.subdivs = s;
            }
        }
        self.holiday_flyout.clear_loaded();
    }

    fn fetch_holiday_flyout(&mut self) {
        let country = self.config.schedule.country.clone();
        let subdiv = self.config.schedule.subdiv.clone();
        let year = self.holiday_flyout.view_year;
        if !self.daemon_ok {
            self.holiday_flyout.error = Some("Daemon offline — start awayd first.".into());
            return;
        }
        match self
            .rt
            .block_on(self.ipc.list_holidays(&country, &subdiv, year))
        {
            Ok(resp) => self.holiday_flyout.apply_response(&country, &subdiv, resp),
            Err(e) => {
                self.holiday_flyout.holidays.clear();
                self.holiday_flyout.error = Some(e.to_string());
            }
        }
    }

    fn refresh_holiday_data_for_country(&mut self) {
        let country = self.config.schedule.country.clone();
        if !self.daemon_ok {
            self.set_status_error("Daemon offline — cannot refresh holidays.");
            return;
        }
        match self.rt.block_on(self.ipc.refresh_holiday_data(&country)) {
            Ok(msg) => {
                if msg.starts_with("error:") {
                    self.set_status_error(msg);
                } else {
                    self.set_status_success("Holiday data refreshed.");
                    self.holiday_flyout.clear_loaded();
                    self.fetch_holiday_flyout();
                }
            }
            Err(e) => self.set_status_error(format!("Refresh failed: {e}")),
        }
    }

    fn handle_holiday_flyout_action(&mut self, action: HolidayFlyoutAction) {
        match action {
            HolidayFlyoutAction::None => {}
            HolidayFlyoutAction::Reload => self.fetch_holiday_flyout(),
            HolidayFlyoutAction::RefreshCountry => self.refresh_holiday_data_for_country(),
        }
    }

    fn remember_subdiv(&mut self) {
        let cc = self.config.schedule.country.clone();
        let sd = self.config.schedule.subdiv.clone();
        if !sd.is_empty() {
            self.subdiv_by_country.insert(cc, sd);
        }
    }

    fn region_pickers(&mut self, ui: &mut egui::Ui) -> bool {
        const LABEL_W: f32 = 64.0;
        const CONTROL_W: f32 = 280.0;
        const GRID_SPACING: f32 = 8.0;
        let button_w = CONTROL_W;
        let mut changed = false;
        let country_opts = self.country_options();
        let subdiv_opts = self.subdiv_options();

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                egui::Grid::new(ui.id().with("regions"))
                    .num_columns(2)
                    .spacing([GRID_SPACING, 4.0])
                    .min_col_width(LABEL_W)
                    .show(ui, |ui| {
                        ui.label("Country");
                        if let Some(code) = self.country_picker.show_button(
                            ui,
                            "country",
                            &self.config.schedule.country,
                            &country_opts,
                            button_w,
                        ) {
                            if code != self.config.schedule.country {
                                self.remember_subdiv();
                                self.config.schedule.country = code.clone();
                                self.config.schedule.subdiv = self
                                    .subdiv_by_country
                                    .get(&code)
                                    .cloned()
                                    .unwrap_or_default();
                                self.reload_subdivs_for_country();
                                changed = true;
                            }
                        }
                        ui.end_row();

                        ui.label("Region");
                        if let Some(code) = self.subdiv_picker.show_button(
                            ui,
                            "subdiv",
                            &self.config.schedule.subdiv,
                            &subdiv_opts,
                            button_w,
                        ) {
                            if code != self.config.schedule.subdiv {
                                self.config.schedule.subdiv = code;
                                self.remember_subdiv();
                                changed = true;
                            }
                        }
                        ui.end_row();

                        ui.label("");
                        let subdiv_label = format_subdiv_label(
                            &self.config.schedule.country,
                            &self.config.schedule.subdiv,
                        );
                        let action = self.holiday_flyout.show(
                            ui,
                            button_w,
                            &self.config.schedule.country,
                            &self.config.schedule.subdiv,
                            &subdiv_label,
                            self.daemon_ok,
                        );
                        self.handle_holiday_flyout_action(action);
                        ui.end_row();
                    });
            });
        });
        changed
    }

    fn ease_smoothstep(t: f32) -> f32 {
        t * t * (3.0 - 2.0 * t)
    }

    fn begin_success_toast(&mut self, text: String) {
        self.status = Some(StatusState {
            text,
            kind: StatusKind::Success,
            phase: Some(ToastPhase::FadeIn {
                start: self.frame_time,
            }),
        });
    }

    fn set_status_success(&mut self, text: impl Into<String>) {
        let text = text.into();
        if let Some(status) = &mut self.status {
            if status.kind == StatusKind::Success {
                status.text = text;
                status.phase = Some(ToastPhase::Gap {
                    until: self.frame_time + TOAST_GAP,
                });
                return;
            }
        }
        self.begin_success_toast(text);
    }

    fn set_status_error(&mut self, text: impl Into<String>) {
        self.status = Some(StatusState {
            text: text.into(),
            kind: StatusKind::Error,
            phase: None,
        });
    }

    fn status_opacity(&self) -> f32 {
        let Some(status) = &self.status else {
            return 0.0;
        };
        match status.kind {
            StatusKind::Error => 1.0,
            StatusKind::Success => {
                let Some(phase) = status.phase else {
                    return 1.0;
                };
                match phase {
                    ToastPhase::FadeIn { start } => {
                        let t = ((self.frame_time - start) / TOAST_FADE_IN).clamp(0.0, 1.0) as f32;
                        Self::ease_smoothstep(t)
                    }
                    ToastPhase::Hold { .. } => 1.0,
                    ToastPhase::FadeOut { start } => {
                        let t = ((self.frame_time - start) / TOAST_FADE_OUT).clamp(0.0, 1.0) as f32;
                        1.0 - Self::ease_smoothstep(t)
                    }
                    ToastPhase::Gap { .. } => 0.0,
                }
            }
        }
    }

    fn tick_status(&mut self, ctx: &egui::Context) {
        let mut animating = false;
        if let Some(status) = &mut self.status {
            if status.kind == StatusKind::Success {
                if let Some(phase) = status.phase {
                    animating = true;
                    match phase {
                        ToastPhase::FadeIn { start } => {
                            if self.frame_time >= start + TOAST_FADE_IN {
                                status.phase = Some(ToastPhase::Hold {
                                    until: self.frame_time + TOAST_HOLD,
                                });
                            }
                        }
                        ToastPhase::Hold { until } => {
                            if self.frame_time >= until {
                                status.phase = Some(ToastPhase::FadeOut {
                                    start: self.frame_time,
                                });
                            }
                        }
                        ToastPhase::FadeOut { start } => {
                            if self.frame_time >= start + TOAST_FADE_OUT {
                                self.status = None;
                                animating = false;
                            }
                        }
                        ToastPhase::Gap { until } => {
                            if self.frame_time >= until {
                                status.phase = Some(ToastPhase::FadeIn {
                                    start: self.frame_time,
                                });
                            }
                        }
                    }
                }
            }
        }
        if animating {
            ctx.request_repaint();
        }
    }

    fn show_status_slot(&self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(width, STATUS_SLOT_H), egui::Sense::hover());

        let placeholder_fill = ui
            .visuals()
            .widgets
            .noninteractive
            .bg_fill
            .gamma_multiply(0.12);
        let placeholder_stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        ui.painter().rect(
            rect,
            2.0,
            placeholder_fill,
            egui::Stroke::new(1.0_f32, placeholder_stroke.color.gamma_multiply(0.25)),
        );

        let Some(status) = &self.status else {
            return;
        };
        let alpha = self.status_opacity();
        if alpha < 0.01 {
            return;
        }

        let (base_fill, base_text) = match status.kind {
            StatusKind::Success => (egui::Color32::from_rgb(72, 160, 88), egui::Color32::WHITE),
            StatusKind::Error => (egui::Color32::from_rgb(180, 60, 60), egui::Color32::WHITE),
        };
        let fill = base_fill.gamma_multiply(alpha);
        let text_color = base_text.gamma_multiply(alpha);

        let inner = rect.shrink2(egui::vec2(0.0, 1.0));
        ui.painter().rect_filled(inner, 2.0, fill);
        ui.painter().text(
            inner.left_center() + egui::vec2(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            &status.text,
            egui::FontId::proportional(14.0),
            text_color,
        );
    }

    fn default_vacation_label(index: usize) -> String {
        format!("Vacation {}", index + 1)
    }

    fn resolve_vacation_label(raw: &str, index: usize) -> String {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            Self::default_vacation_label(index)
        } else {
            trimmed.to_string()
        }
    }

    fn normalize_vacation_labels(&mut self) {
        for (i, v) in self.config.schedule.vacations.iter_mut().enumerate() {
            v.label = Self::resolve_vacation_label(&v.label, i);
        }
    }

    fn sync_vacation_range_states(&mut self) {
        let n = self.config.schedule.vacations.len();
        if self.vacation_ranges.len() > n {
            self.vacation_ranges.truncate(n);
        }
        while self.vacation_ranges.len() < n {
            let i = self.vacation_ranges.len();
            let v = &self.config.schedule.vacations[i];
            self.vacation_ranges.push(VacationRangeUi {
                range: DateRangeFieldState::from_range(&v.start, &v.end),
            });
        }
    }

    fn vacation_dates_valid(start: &str, end: &str) -> bool {
        match (
            NaiveDate::parse_from_str(start.trim(), "%Y-%m-%d"),
            NaiveDate::parse_from_str(end.trim(), "%Y-%m-%d"),
        ) {
            (Ok(s), Ok(e)) => s <= e,
            _ => false,
        }
    }

    /// Password field for the credential of backend `id`. `widget_id` keeps wizard and
    /// scheduler page fields apart. Returns true when the input should be saved (Enter or
    /// leaving the field): saving while typing would store partial tokens and send requests
    /// with them.
    fn show_credential_field(
        &mut self,
        ui: &mut egui::Ui,
        id: &'static str,
        widget_id: &str,
    ) -> bool {
        let mut commit = false;
        let field = self.credentials.entry(id).or_default();
        let widget_id = ui.id().with(widget_id).with(id);
        if field.stored && !field.editing && field.input.is_empty() {
            let mut display = CREDENTIAL_STORED_PLACEHOLDER.to_string();
            let response = ui.add(
                egui::TextEdit::singleline(&mut display)
                    .id(widget_id)
                    .password(true)
                    .desired_width(280.0),
            );
            if response.gained_focus() {
                field.editing = true;
                field.input.clear();
            }
        } else {
            let response = ui.add(
                egui::TextEdit::singleline(&mut field.input)
                    .id(widget_id)
                    .password(true)
                    .desired_width(280.0),
            );
            if response.gained_focus() {
                field.editing = true;
            }
            commit = response.lost_focus();
        }
        if field.stored && !field.editing {
            ui.label("Saved in Credential Manager. Type a new value to replace.");
        } else if !field.input.trim().is_empty() {
            ui.label("Press Enter or leave the field to save.");
        }
        commit
    }

    /// One switch per service with its risk note and, while switched on, its credential
    /// field. Returns true when a switch changed (the caller decides when to save).
    fn services_ui(&mut self, ui: &mut egui::Ui, widget_id: &str, credential_hint: &str) -> bool {
        let mut changed = false;
        ui.label(egui::RichText::new("Services").strong());
        for b in backends::ALL {
            let mut on = self.config.backend_enabled(b.id);
            if ui
                .checkbox(&mut on, b.display_name)
                .on_hover_text(format!("Let awayd set your {} status", b.display_name))
                .changed()
            {
                self.config.set_backend_enabled(b.id, on);
                changed = true;
            }
            ui.indent(b.id, |ui| {
                if let Some(note) = b.risk_note {
                    ui.colored_label(ui.visuals().warn_fg_color, note);
                }
                if let (true, Some(label)) = (on, b.credential_label) {
                    ui.label(format!("{label} ({credential_hint}):"));
                    if self.show_credential_field(ui, b.id, widget_id)
                        && self.credential_input_pending(b.id)
                    {
                        self.save_credential(b.id);
                    }
                }
            });
        }
        if !backends::ALL
            .iter()
            .any(|b| self.config.backend_enabled(b.id))
        {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "No service is switched on: awayd changes no status.",
            );
        }
        changed
    }

    fn credential_input_pending(&self, id: &str) -> bool {
        self.credentials
            .get(id)
            .is_some_and(|f| !f.input.trim().is_empty())
    }

    /// Save all fields that still hold typed input (wizard "Next").
    fn flush_pending_credentials(&mut self) {
        for b in credential_backends() {
            if self.credential_input_pending(b.id) {
                self.save_credential(b.id);
            }
        }
    }

    /// Persisted width/height are inner viewport size at 100% zoom (`inner * zoom`).
    pub fn store_size_from_inner(inner_w: f32, inner_h: f32, zoom: f32) -> (f32, f32) {
        (inner_w * zoom, inner_h * zoom)
    }

    pub fn inner_size_from_store(store_w: f32, store_h: f32, zoom: f32) -> (f32, f32) {
        (store_w / zoom, store_h / zoom)
    }

    /// Saved/restored size is inner (client) area; position is outer top-left.
    fn track_window_geometry(&mut self, ctx: &egui::Context) {
        if self.startup_prefs_pending || self.frame_time < self.window_track_after {
            return;
        }
        let Some(inner) = ctx.input(|i| i.viewport().inner_rect) else {
            return;
        };
        let Some(outer) = ctx.input(|i| i.viewport().outer_rect) else {
            return;
        };
        let (inner_w, inner_h) = (inner.width(), inner.height());
        let (store_w, store_h) = Self::store_size_from_inner(inner_w, inner_h, self.zoom_applied);
        let pos_x = outer.min.x;
        let pos_y = outer.min.y;
        let candidate = (store_w, store_h, pos_x, pos_y);

        match self.tracked_geometry {
            Some(prev)
                if (prev.0 - store_w).abs() <= 0.5
                    && (prev.1 - store_h).abs() <= 0.5
                    && (prev.2 - pos_x).abs() <= 0.5
                    && (prev.3 - pos_y).abs() <= 0.5 =>
            {
                if self.frame_time - self.tracked_geometry_since < WINDOW_SIZE_STABLE_SECS {
                    return;
                }
            }
            _ => {
                self.tracked_geometry = Some(candidate);
                self.tracked_geometry_since = self.frame_time;
                return;
            }
        }

        let size_changed = (store_w - self.ui_prefs.width).abs() > 0.5
            || (store_h - self.ui_prefs.height).abs() > 0.5;
        let pos_changed = self
            .ui_prefs
            .pos_x
            .map(|x| (x - pos_x).abs() > 0.5)
            .unwrap_or(true)
            || self
                .ui_prefs
                .pos_y
                .map(|y| (y - pos_y).abs() > 0.5)
                .unwrap_or(true);
        if size_changed || pos_changed {
            self.ui_prefs.width = store_w;
            self.ui_prefs.height = store_h;
            self.ui_prefs.pos_x = Some(pos_x);
            self.ui_prefs.pos_y = Some(pos_y);
            self.ui_prefs_save_at = Some(self.frame_time + UI_PREFS_DEBOUNCE_SECS);
        }
    }

    fn apply_startup_prefs(&mut self, ctx: &egui::Context) {
        if !self.startup_prefs_pending {
            return;
        }
        ctx.set_zoom_factor(self.zoom_applied);
        let Some(inner) = ctx.input(|i| i.viewport().inner_rect) else {
            return;
        };
        let outer = ctx.input(|i| i.viewport().outer_rect);
        let (inner_w, inner_h) = (inner.width(), inner.height());
        let (target_inner_w, target_inner_h) = Self::inner_size_from_store(
            self.ui_prefs.width,
            self.ui_prefs.height,
            self.zoom_applied,
        );
        let dw = (inner_w - target_inner_w).abs();
        let dh = (inner_h - target_inner_h).abs();
        if dw > 1.0 || dh > 1.0 {
            if self.startup_resize_attempts < 3 {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    target_inner_w,
                    target_inner_h,
                )));
                self.startup_resize_attempts += 1;
                return;
            }
            let (store_w, store_h) =
                Self::store_size_from_inner(inner_w, inner_h, self.zoom_applied);
            self.ui_prefs.width = store_w;
            self.ui_prefs.height = store_h;
        }

        if let (Some(px), Some(py)) = (self.ui_prefs.pos_x, self.ui_prefs.pos_y) {
            if let Some(outer) = outer {
                let dx = (outer.min.x - px).abs();
                let dy = (outer.min.y - py).abs();
                if dx > 1.0 || dy > 1.0 {
                    if self.startup_position_attempts < 3 {
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                            px, py,
                        )));
                        self.startup_position_attempts += 1;
                        return;
                    }
                    self.ui_prefs.pos_x = Some(outer.min.x);
                    self.ui_prefs.pos_y = Some(outer.min.y);
                }
            }
        }

        self.startup_prefs_pending = false;
        self.window_track_after = self.frame_time + WINDOW_TRACK_GRACE_SECS;
        self.tracked_geometry = None;
    }

    fn flush_ui_prefs_save(&mut self) {
        self.ui_prefs_save_at = None;
        if let Err(e) = self.ui_prefs.save() {
            self.set_status_error(format!("UI prefs save failed: {e}"));
        }
    }

    fn tick_ui_prefs_save(&mut self) {
        let Some(at) = self.ui_prefs_save_at else {
            return;
        };
        if self.frame_time >= at {
            self.flush_ui_prefs_save();
        }
    }

    fn apply_zoom(&mut self, zoom: f32, ctx: &egui::Context) {
        self.zoom_applied = zoom;
        self.zoom_pending = zoom;
        self.ui_prefs.zoom = zoom;
        ctx.set_zoom_factor(zoom);
    }

    fn display_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Display");
        if !self.zoom_dragging {
            self.zoom_pending = self.zoom_applied;
        }

        ui.horizontal(|ui| {
            let zoom_pct = (self.zoom_pending * 100.0).round() as i32;
            ui.spacing_mut().slider_width = SETTINGS_SLIDER_WIDTH;
            let slider = ui.add(
                egui::Slider::new(&mut self.zoom_pending, 0.75..=1.5)
                    .text(format!("Zoom {zoom_pct}%")),
            );
            let ctx = ui.ctx().clone();

            if slider.drag_started() {
                self.zoom_dragging = true;
            }
            if slider.drag_stopped() {
                self.apply_zoom(self.zoom_pending, &ctx);
                self.zoom_dragging = false;
                self.flush_ui_prefs_save();
            }

            let at_default = (self.zoom_applied - 1.0).abs() < 0.001;
            ui.add_enabled_ui(!at_default, |ui| {
                if ui
                    .button("Reset")
                    .on_hover_text("Reset zoom to 100%")
                    .clicked()
                {
                    self.apply_zoom(1.0, &ctx);
                    self.zoom_dragging = false;
                    self.flush_ui_prefs_save();
                }
            });
        });

        if self.zoom_dragging {
            zoom_preview::show_typography_sample(ui, self.zoom_pending);
        }
    }

    fn apply_loaded_config(&mut self, loaded: Result<Option<AppConfig>, String>) {
        match loaded {
            Ok(Some(c)) => {
                self.config = c;
                self.config_error = None;
                // Configs saved before unique ids were given out may contain duplicates;
                // the repaired ids are written with the next save.
                if dedupe_vacation_ids(&mut self.config.schedule.vacations) {
                    tracing::info!("duplicate vacation ids renumbered");
                }
            }
            Ok(None) => self.config_error = None,
            Err(e) => self.config_error = Some(e),
        }
    }

    fn refresh_autostart(&mut self) {
        self.autostart_state = autostart::state(&awayd_exe()).map_err(|e| format!("{e:#}"));
    }

    /// Register or remove the login autostart entry (GitHub issue 1) and report the result.
    fn set_autostart(&mut self, on: bool) {
        let result = if on {
            autostart::install(&awayd_exe()).map(|()| "Autostart enabled")
        } else {
            autostart::uninstall().map(|removed| {
                if removed {
                    "Autostart disabled"
                } else {
                    "Autostart was not set"
                }
            })
        };
        match result {
            Ok(msg) => self.set_status_success(msg),
            Err(e) => self.set_status_error(format!("Autostart: {e:#}")),
        }
        self.refresh_autostart();
    }

    fn autostart_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Startup").strong());
        let state = self.autostart_state.clone();
        let state = match state {
            Ok(s) => s,
            Err(e) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Could not read the autostart entry: {e}"),
                );
                if ui.button("Retry").clicked() {
                    self.refresh_autostart();
                }
                return;
            }
        };
        let mut on = matches!(state, AutostartState::On { .. });
        if ui
            .checkbox(&mut on, "Start awayd at login")
            .on_hover_text("Adds or removes the value \"awayd\" under HKCU\\…\\CurrentVersion\\Run")
            .changed()
        {
            self.set_autostart(on);
        }
        match state {
            AutostartState::On { quoted: false } => {
                ui.horizontal(|ui| {
                    ui.label("The entry uses an older format (path without quotes).");
                    if ui.button("Repair").clicked() {
                        self.set_autostart(true);
                    }
                });
            }
            AutostartState::Elsewhere(path) => {
                ui.label(format!("The login entry starts another file: {path}"));
                ui.horizontal(|ui| {
                    if ui.button("Use this awayd.exe").clicked() {
                        self.set_autostart(true);
                    }
                    if ui.button("Remove entry").clicked() {
                        self.set_autostart(false);
                    }
                });
            }
            AutostartState::Off | AutostartState::On { quoted: true } => {}
        }
    }

    /// Version of the running daemon from its status JSON (`None` while offline, before its
    /// first tick or when it is an older build without the field).
    fn refresh_daemon_version(&mut self) {
        self.daemon_version = if self.daemon_ok {
            self.rt
                .block_on(self.ipc.status())
                .ok()
                .and_then(|raw| serde_json::from_str::<StatusSnapshot>(&raw).ok())
                .map(|s| s.version)
                .filter(|v| !v.is_empty())
        } else {
            None
        };
    }

    fn reload(&mut self) {
        self.daemon_ok = self.rt.block_on(self.ipc.ping());
        self.refresh_daemon_version();
        self.refresh_autostart();
        // Always read config.toml from disk: the daemon's copy is only refreshed by a
        // successful tick, so after a manual edit it can still hold the old valid config and
        // an unreadable file would go unnoticed (and be overwritten on the next save).
        let file_exists = paths::config_file().is_file();
        let text = std::fs::read_to_string(paths::config_file()).map_err(|e| e.to_string());
        self.apply_loaded_config(resolve_loaded_config(file_exists, text));
        if self.daemon_ok {
            if let Ok(list) = self.rt.block_on(self.ipc.list_countries()) {
                if !list.is_empty() {
                    self.countries = list;
                }
            }
            for b in credential_backends() {
                let stored = self
                    .rt
                    .block_on(self.ipc.has_credential(b.id))
                    .unwrap_or(false);
                self.credentials.entry(b.id).or_default().stored = stored;
            }
            self.reload_subdivs_for_country();
        } else if file_exists {
            self.reload_subdivs_for_country();
        }
        self.remember_subdiv();
        let days = schedule_days(&self.config.schedule);
        self.schedule_grid.sync_from_days(&days);
        self.sync_vacation_range_states();
        for field in self.credentials.values_mut() {
            if !field.editing {
                field.input.clear();
            }
        }
    }

    fn validate_before_save(&self) -> Result<(), String> {
        if !self.config.schedule.subdiv.is_empty() {
            return Ok(());
        }
        let opts = self.subdiv_options();
        if opts.is_empty() {
            return Ok(());
        }
        Err("Select a region".into())
    }

    fn save_schedule_if_valid(&mut self) {
        self.normalize_vacation_labels();
        self.save();
    }

    /// Copy the unreadable `config.toml` aside (never overwriting an earlier backup) and
    /// allow saving the values currently shown.
    fn backup_config_and_start_over(&mut self) {
        let src = paths::config_file();
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let dst = src.with_file_name(format!("config.toml.{stamp}.bak"));
        match std::fs::copy(&src, &dst) {
            Ok(_) => {
                self.config_error = None;
                self.set_status_success(format!(
                    "Backed up to {} — saving is enabled again",
                    dst.display()
                ));
            }
            Err(e) => self.set_status_error(format!("Backup failed, nothing changed: {e}")),
        }
    }

    fn save(&mut self) {
        if let Some(e) = &self.config_error {
            self.set_status_error(format!(
                "Not saved: config.toml could not be read ({e}). Fix the file and reload, or back it up and start over."
            ));
            return;
        }
        if let Err(e) = self.validate_before_save() {
            self.set_status_error(e);
            return;
        }
        for v in &self.config.schedule.vacations {
            if !Self::vacation_dates_valid(&v.start, &v.end) {
                self.set_status_error(format!("Vacation '{}': invalid or reversed dates", v.label));
                return;
            }
        }
        self.config.schedule.days = Some(self.schedule_grid.commit_days_for_save());
        normalize_schedule(&mut self.config.schedule);
        // Same validation as the daemon and `awayctl validate`; also guards the offline path,
        // which writes config.toml directly without the daemon's check.
        if let Err(e) = awayd_core::config::validate_app_config(&self.config) {
            self.set_status_error(format!("Not saved: {e}"));
            return;
        }
        // `mode` is kept as loaded: network mode can only be set in config.toml so far and
        // must not be switched off by an unrelated save.
        let toml = match toml::to_string_pretty(&self.config) {
            Ok(t) => t,
            Err(e) => {
                self.set_status_error(format!("serialize error: {e}"));
                return;
            }
        };
        if !self.daemon_ok {
            if let Err(e) =
                awayd_core::atomic_fs::atomic_write(&paths::config_file(), toml.as_bytes())
            {
                self.set_status_error(format!("write error: {e}"));
            } else {
                self.set_status_success("saved locally (daemon offline)");
            }
            return;
        }
        match self.rt.block_on(self.ipc.set_config_toml(&toml)) {
            Ok(true) => self.set_status_success("saved"),
            Ok(false) => self.set_status_error("daemon rejected config"),
            Err(e) => self.set_status_error(format!("save error: {e}")),
        }
    }

    fn save_credential(&mut self, id: &'static str) {
        let Some(value) = self
            .credentials
            .get(id)
            .map(|f| f.input.clone())
            .filter(|v| !v.trim().is_empty())
        else {
            return;
        };
        let label = backends::find(id)
            .and_then(|b| b.credential_label)
            .unwrap_or("credential");
        match self.rt.block_on(self.ipc.set_credential(id, &value)) {
            Ok(true) => {
                self.set_status_success(format!("{label} saved"));
                if let Some(field) = self.credentials.get_mut(id) {
                    field.input.clear();
                    field.editing = false;
                    field.stored = true;
                }
            }
            Ok(false) => self.set_status_error(format!("{label} rejected")),
            Err(e) => self.set_status_error(format!("{label} not saved: {e}")),
        }
    }
}

impl eframe::App for AwaydSettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_time = ctx.input(|i| i.time);
        self.apply_startup_prefs(ctx);
        self.tick_status(ctx);
        self.track_window_geometry(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("awayd");
                ui.label(format!("v{}", awayd_core::VERSION));
                ui.separator();
                let daemon = daemon_status_label(self.daemon_ok, self.daemon_version.as_deref());
                if self.daemon_ok {
                    ui.label(egui::RichText::new(daemon).color(egui::Color32::GREEN));
                } else {
                    ui.label(egui::RichText::new(daemon).color(egui::Color32::YELLOW));
                }
            });
            if let Some(hint) = version_mismatch_hint(self.daemon_version.as_deref()) {
                ui.colored_label(ui.visuals().warn_fg_color, hint);
            }
            if let Some(err) = self.config_error.clone() {
                match config_error_banner(ui, &err) {
                    Some(ConfigErrorAction::Reload) => self.reload(),
                    Some(ConfigErrorAction::BackupAndStartOver) => {
                        self.backup_config_and_start_over()
                    }
                    None => {}
                }
            }
            self.show_status_slot(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.wizard {
                self.wizard_ui(ui);
            } else {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.active_tab, SettingsTab::Scheduler, "Scheduler");
                    ui.selectable_value(&mut self.active_tab, SettingsTab::Display, "Display");
                    ui.selectable_value(&mut self.active_tab, SettingsTab::Logs, "Logs");
                });
                ui.separator();

                let logs_active = self.active_tab == SettingsTab::Logs || self.log_popout_open;
                if logs_active {
                    if self.log_view.is_none() {
                        self.log_view = Some(LogViewState::new());
                    }
                    if let Some(log) = &mut self.log_view {
                        log.poll();
                    }
                } else {
                    self.log_view = None;
                }

                match self.active_tab {
                    SettingsTab::Scheduler => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            self.scheduler_ui(ui);
                            ui.separator();
                            if ui.button("Run setup wizard again…").clicked() {
                                self.wizard = true;
                                self.wizard_step = 0;
                            }
                            ui.separator();
                            self.autostart_ui(ui);
                        });
                    }
                    SettingsTab::Display => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            self.display_ui(ui);
                        });
                    }
                    SettingsTab::Logs => {
                        if self.log_popout_open {
                            let mut dock_logs = false;
                            show_logs_docked_placeholder(ui, &mut dock_logs);
                            if dock_logs {
                                self.log_popout_open = false;
                                ctx.send_viewport_cmd_to(
                                    log_popout_viewport_id(),
                                    egui::ViewportCommand::Close,
                                );
                            }
                        } else if let Some(log) = &mut self.log_view {
                            show_logs_embedded_tab(
                                ui,
                                log,
                                self.daemon_ok,
                                &mut self.log_popout_open,
                            );
                        }
                    }
                }
            }
        });

        if self.log_popout_open {
            let daemon_ok = self.daemon_ok;
            let popout_open = &mut self.log_popout_open;
            if let Some(log) = &mut self.log_view {
                ctx.show_viewport_immediate(
                    log_popout_viewport_id(),
                    egui::ViewportBuilder::default()
                        .with_title("awayd Logs")
                        .with_inner_size(LOG_POPOUT_DEFAULT_SIZE),
                    |ctx, _class| {
                        if ctx.input(|i| i.viewport().close_requested()) {
                            *popout_open = false;
                            return;
                        }
                        egui::CentralPanel::default().show(ctx, |ui| {
                            show_logs_full(ui, log, daemon_ok);
                        });
                        ctx.request_repaint_after(std::time::Duration::from_millis(500));
                    },
                );
            }
        }

        if self.active_tab == SettingsTab::Logs || self.log_popout_open {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        self.tick_ui_prefs_save();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(e) = self.ui_prefs.save() {
            tracing::warn!("UI prefs save on exit failed: {e}");
        }
    }
}

impl AwaydSettingsApp {
    fn wizard_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Setup wizard");
        match self.wizard_step {
            0 => {
                ui.label(
                    "Welcome — configure automatic Discord/Steam status from your work schedule.",
                );
                if ui.button("Next").clicked() {
                    self.wizard_step = 1;
                }
            }
            1 => {
                // Switches are saved with the region/work hours steps and on "Finish".
                self.services_ui(
                    ui,
                    "wiz_credential",
                    "stored in Credential Manager, not config.toml",
                );
                ui.horizontal(|ui| {
                    if ui.button("Back").clicked() {
                        self.wizard_step = 0;
                    }
                    if ui.button("Next").clicked() {
                        self.flush_pending_credentials();
                        self.wizard_step = 2;
                    }
                });
            }
            2 => {
                ui.label("Country and region (public holidays):");
                if self.region_pickers(ui) {
                    self.save_schedule_if_valid();
                }
                ui.horizontal(|ui| {
                    if ui.button("Back").clicked() {
                        self.wizard_step = 1;
                    }
                    if ui.button("Next").clicked() {
                        if let Err(e) = self.validate_before_save() {
                            self.set_status_error(e);
                        } else {
                            self.wizard_step = 3;
                        }
                    }
                });
            }
            3 => {
                ui.label("Work hours (local time):");
                let mut changed = false;
                if show_weekday_schedule_grid(ui, "wiz_schedule", &mut self.schedule_grid) {
                    changed = true;
                }
                if changed {
                    self.config.schedule.days = Some(self.schedule_grid.commit_days_for_save());
                    self.save_schedule_if_valid();
                }
                ui.horizontal(|ui| {
                    if ui.button("Back").clicked() {
                        self.wizard_step = 2;
                    }
                    if ui.button("Finish").clicked() {
                        self.save();
                        self.wizard = false;
                    }
                });
            }
            _ => {}
        }
    }

    fn scheduler_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Scheduler");
        let network_active = self.config.mode.kind == "network";
        ui.add_enabled_ui(false, |ui| {
            let mut network = network_active;
            ui.checkbox(&mut network, "Use network mode (coming soon)");
        });
        if network_active {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Network mode is set in config.toml and stays active; the work hours below are \
                 used only while it is off.",
            );
        }
        ui.separator();
        let mut schedule_changed = false;
        if self.region_pickers(ui) {
            schedule_changed = true;
        }
        ui.separator();
        if show_weekday_schedule_grid(ui, "sched_days", &mut self.schedule_grid) {
            self.config.schedule.days = Some(self.schedule_grid.commit_days_for_save());
            schedule_changed = true;
        }

        ui.separator();
        if self.services_ui(ui, "sched_credential", "leave empty to keep existing") {
            schedule_changed = true;
        }

        ui.separator();
        if self.vacations_ui(ui) {
            schedule_changed = true;
        }

        if schedule_changed {
            self.save_schedule_if_valid();
        }
    }

    fn try_commit_vacation_draft(&mut self) -> bool {
        if !Self::vacation_dates_valid(&self.vac_draft_start, &self.vac_draft_end) {
            return false;
        }
        let index = self.config.schedule.vacations.len();
        let id = next_vacation_id(&self.config.schedule.vacations);
        self.config.schedule.vacations.push(VacationEntry {
            id,
            label: Self::resolve_vacation_label(&self.vac_draft_label, index),
            start: self.vac_draft_start.clone(),
            end: self.vac_draft_end.clone(),
            comment: String::new(),
        });
        self.vacation_ranges.push(VacationRangeUi {
            range: DateRangeFieldState::from_range(&self.vac_draft_start, &self.vac_draft_end),
        });
        self.vac_draft_label.clear();
        self.vac_draft_start.clear();
        self.vac_draft_end.clear();
        self.vac_draft_range_state = DateRangeFieldState::from_range("", "");
        true
    }

    fn vacations_ui(&mut self, ui: &mut egui::Ui) -> bool {
        ui.label("Vacations (treated like holidays — online):");
        let mut changed = false;
        let mut remove: Option<usize> = None;
        self.sync_vacation_range_states();
        let len = self.config.schedule.vacations.len();
        for i in 0..len {
            let id = self.config.schedule.vacations[i].id.clone();
            let mut label = self.config.schedule.vacations[i].label.clone();
            let hint = Self::default_vacation_label(i);
            let range_state = &mut self.vacation_ranges[i].range;
            let vac = &mut self.config.schedule.vacations[i];
            ui.horizontal(|ui| {
                let name_resp = ui.add(
                    egui::TextEdit::singleline(&mut label)
                        .hint_text(hint)
                        .desired_width(140.0),
                );
                if name_resp.lost_focus() {
                    let resolved = Self::resolve_vacation_label(&label, i);
                    if resolved != vac.label {
                        vac.label = resolved.clone();
                        label = resolved;
                        changed = true;
                    }
                }
                if show_date_range_field(
                    ui,
                    &format!("vac_{id}_range"),
                    &mut vac.start,
                    &mut vac.end,
                    range_state,
                ) {
                    changed = true;
                }
                if ui.button("🗑").on_hover_text("Remove vacation").clicked() {
                    remove = Some(i);
                }
            });
            if !label.trim().is_empty() {
                self.config.schedule.vacations[i].label = label;
            }
        }
        if let Some(i) = remove {
            self.config.schedule.vacations.remove(i);
            self.vacation_ranges.remove(i);
            changed = true;
        }

        let draft_hint = Self::default_vacation_label(len);
        ui.horizontal(|ui| {
            let draft_name = ui.add(
                egui::TextEdit::singleline(&mut self.vac_draft_label)
                    .hint_text(draft_hint)
                    .desired_width(140.0),
            );
            if draft_name.lost_focus() && !self.vac_draft_label.trim().is_empty() {
                changed = true;
            }
            if show_date_range_field(
                ui,
                "vac_draft_range",
                &mut self.vac_draft_start,
                &mut self.vac_draft_end,
                &mut self.vac_draft_range_state,
            ) {
                changed = true;
            }
        });

        if self.try_commit_vacation_draft() {
            changed = true;
        }
        changed
    }
}

/// Persistent notice while `config.toml` is unreadable.
fn config_error_banner(ui: &mut egui::Ui, err: &str) -> Option<ConfigErrorAction> {
    let mut action = None;
    ui.group(|ui| {
        ui.colored_label(
            ui.visuals().error_fg_color,
            "config.toml could not be read — changes are not saved.",
        );
        ui.label(format!("{}: {err}", paths::config_file().display()));
        ui.horizontal(|ui| {
            if ui
                .button("Reload")
                .on_hover_text("Read config.toml again after fixing it by hand")
                .clicked()
            {
                action = Some(ConfigErrorAction::Reload);
            }
            if ui
                .button("Back up and start over")
                .on_hover_text("Copy the file to a timestamped .bak and save the values shown here from now on")
                .clicked()
            {
                action = Some(ConfigErrorAction::BackupAndStartOver);
            }
        });
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_toml() -> String {
        toml::to_string_pretty(&AppConfig::default()).unwrap()
    }

    #[test]
    fn missing_file_keeps_defaults_without_error() {
        // The daemon reports "# empty config" when the file does not exist (first start).
        let r = resolve_loaded_config(false, Ok("# empty config\n".into()));
        assert!(matches!(r, Ok(None)));
        let r = resolve_loaded_config(false, Err("not found".into()));
        assert!(matches!(r, Ok(None)));
    }

    #[test]
    fn valid_file_is_loaded() {
        let r = resolve_loaded_config(true, Ok(valid_toml()));
        assert!(matches!(r, Ok(Some(_))));
    }

    #[test]
    fn broken_toml_blocks_saving() {
        let r = resolve_loaded_config(true, Ok("schema_version = 1\n[schedule\n".into()));
        assert!(r.is_err());
    }

    #[test]
    fn unsupported_schema_version_blocks_saving() {
        let text = valid_toml().replace("schema_version = 1", "schema_version = 2");
        assert!(
            text.contains("schema_version = 2"),
            "precondition: version replaced"
        );
        let err = resolve_loaded_config(true, Ok(text)).unwrap_err();
        assert!(err.contains("schema_version"), "message: {err}");
    }

    #[test]
    fn daemon_label_shows_version() {
        assert_eq!(daemon_status_label(false, Some("0.1.0")), "daemon offline");
        assert_eq!(
            daemon_status_label(true, Some("0.1.0")),
            "daemon online (v0.1.0)"
        );
        assert_eq!(
            daemon_status_label(true, None),
            "daemon online (version unknown)"
        );
    }

    #[test]
    fn mismatch_hint_only_for_other_versions() {
        assert_eq!(version_mismatch_hint(None), None);
        assert_eq!(version_mismatch_hint(Some(awayd_core::VERSION)), None);
        let hint = version_mismatch_hint(Some("0.0.1")).expect("different version");
        assert!(
            hint.contains("v0.0.1") && hint.contains(awayd_core::VERSION),
            "{hint}"
        );
    }

    #[test]
    fn read_error_blocks_saving() {
        let r = resolve_loaded_config(true, Err("access denied".into()));
        assert_eq!(r.unwrap_err(), "access denied");
    }

    fn vac(id: &str) -> VacationEntry {
        VacationEntry {
            id: id.into(),
            label: String::new(),
            start: "2026-12-24".into(),
            end: "2026-12-26".into(),
            comment: String::new(),
        }
    }

    #[test]
    fn new_vacation_id_is_never_a_deleted_one() {
        assert_eq!(next_vacation_id(&[]), "v0");
        // v0, v1, v2 created, v1 deleted: the next one must not be v2 again.
        let left = [vac("v0"), vac("v2")];
        assert_eq!(next_vacation_id(&left), "v3");
        // Ids not in the v<n> form (hand-written) are ignored for the numbering.
        assert_eq!(next_vacation_id(&[vac("summer")]), "v0");
    }

    #[test]
    fn duplicate_vacation_ids_are_renumbered() {
        let mut list = vec![vac("v0"), vac("v2"), vac("v2"), vac("v0")];
        assert!(dedupe_vacation_ids(&mut list));
        let ids: Vec<&str> = list.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["v0", "v2", "v3", "v4"]);
        assert!(!dedupe_vacation_ids(&mut list), "nothing left to repair");
    }
}
