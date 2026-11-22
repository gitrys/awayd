//! SPDX-License-Identifier: GPL-3.0-or-later
//! Per-weekday work hours grid (Option A).

use awayd_core::config::schema::DayWindow;
use awayd_core::schedule::DAY_LABELS;
use chrono::NaiveTime;
use eframe::egui;

use crate::widgets::time_picker::{show_time_field_inline, TimeFieldState};

#[derive(Clone)]
pub struct WeekdayScheduleGridState {
    pub days: [DayWindow; 7],
    pub linked: bool,
    pub master_start: TimeFieldState,
    pub master_end: TimeFieldState,
    row_start: [TimeFieldState; 7],
    row_end: [TimeFieldState; 7],
}

impl WeekdayScheduleGridState {
    pub fn from_days(days: &[DayWindow; 7]) -> Self {
        let linked = infer_linked(days);
        let mut row_start = std::array::from_fn(|_| TimeFieldState::from_time(default_start()));
        let mut row_end = std::array::from_fn(|_| TimeFieldState::from_time(default_end()));
        for i in 0..7 {
            row_start[i] = TimeFieldState::from_time(days[i].start);
            row_end[i] = TimeFieldState::from_time(days[i].end);
        }
        let (ms, me) = master_times_from_days(days);
        Self {
            days: *days,
            linked,
            master_start: TimeFieldState::from_time(ms),
            master_end: TimeFieldState::from_time(me),
            row_start,
            row_end,
        }
    }

    pub fn sync_from_days(&mut self, days: &[DayWindow; 7]) {
        self.days = *days;
        self.linked = infer_linked(days);
        for i in 0..7 {
            self.row_start[i].sync_from_time(days[i].start);
            self.row_end[i].sync_from_time(days[i].end);
        }
        let (ms, me) = master_times_from_days(days);
        self.master_start.sync_from_time(ms);
        self.master_end.sync_from_time(me);
    }

    /// Build `[schedule.days]` for save: linked mode applies master times to enabled rows only.
    pub fn commit_days_for_save(&self) -> [DayWindow; 7] {
        let mut out = self.days;
        if self.linked {
            let (start, end) = self.master_times_from_fields();
            for day in out.iter_mut().filter(|d| d.enabled) {
                day.start = start;
                day.end = end;
            }
        }
        out
    }

    fn master_times_from_fields(&self) -> (NaiveTime, NaiveTime) {
        let start = parse_time_edit(&self.master_start.edit).unwrap_or_else(default_start);
        let end = parse_time_edit(&self.master_end.edit).unwrap_or_else(default_end);
        (start, end)
    }

    fn sync_master_from_days(&mut self) {
        let (ms, me) = master_times_from_days(&self.days);
        self.master_start.sync_from_time(ms);
        self.master_end.sync_from_time(me);
    }
}

fn infer_linked(days: &[DayWindow; 7]) -> bool {
    let enabled: Vec<_> = days.iter().filter(|d| d.enabled).collect();
    if enabled.is_empty() {
        return true;
    }
    let first = (enabled[0].start, enabled[0].end);
    enabled.iter().all(|d| (d.start, d.end) == first)
}

fn master_times_from_days(days: &[DayWindow; 7]) -> (NaiveTime, NaiveTime) {
    days.iter()
        .find(|d| d.enabled)
        .map(|d| (d.start, d.end))
        .unwrap_or((default_start(), default_end()))
}

fn default_start() -> NaiveTime {
    NaiveTime::from_hms_opt(9, 0, 0).unwrap()
}

fn default_end() -> NaiveTime {
    NaiveTime::from_hms_opt(17, 0, 0).unwrap()
}

fn parse_time_edit(s: &str) -> Option<NaiveTime> {
    let t = s.trim();
    NaiveTime::parse_from_str(t, "%H:%M")
        .ok()
        .or_else(|| NaiveTime::parse_from_str(t, "%H:%M:%S").ok())
}

/// Draw the weekly schedule grid. Returns true if any value changed.
pub fn show_weekday_schedule_grid(
    ui: &mut egui::Ui,
    id_source: &str,
    state: &mut WeekdayScheduleGridState,
) -> bool {
    let mut changed = false;

    ui.label("Work hours by day");

    if ui
        .checkbox(&mut state.linked, "Use same hours for all active days")
        .changed()
    {
        changed = true;
        if state.linked {
            state.sync_master_from_days();
        }
    }

    ui.horizontal(|ui| {
        let (mut start, mut end) = state.master_times_from_fields();
        ui.label("Start");
        ui.add_enabled_ui(state.linked, |ui| {
            if show_time_field_inline(
                ui,
                &format!("{id_source}_master_start"),
                &mut start,
                &mut state.master_start,
            ) {
                changed = true;
            }
        });
        ui.add_space(8.0);
        ui.label("End");
        ui.add_enabled_ui(state.linked, |ui| {
            if show_time_field_inline(
                ui,
                &format!("{id_source}_master_end"),
                &mut end,
                &mut state.master_end,
            ) {
                changed = true;
            }
        });
    });

    let mut custom = !state.linked;
    if ui.checkbox(&mut custom, "Use custom table").changed() {
        changed = true;
        state.linked = !custom;
        if state.linked {
            state.sync_master_from_days();
        }
    }
    if !state.linked && !custom {
        state.linked = true;
    }

    egui::Grid::new(format!("{id_source}_grid"))
        .num_columns(4)
        .spacing([12.0, 4.0])
        .min_col_width(72.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Day").strong());
            ui.label(egui::RichText::new("On?").strong());
            ui.label(egui::RichText::new("Start").strong());
            ui.label(egui::RichText::new("End").strong());
            ui.end_row();

            for i in 0..7 {
                ui.label(DAY_LABELS[i]);
                if ui.checkbox(&mut state.days[i].enabled, "").changed() {
                    changed = true;
                }

                let row_times_editable = state.days[i].enabled && !state.linked;
                ui.add_enabled_ui(row_times_editable, |ui| {
                    let mut start = state.days[i].start;
                    if show_time_field_inline(
                        ui,
                        &format!("{id_source}_d{i}_start"),
                        &mut start,
                        &mut state.row_start[i],
                    ) {
                        state.days[i].start = start;
                        changed = true;
                    }
                });
                ui.add_enabled_ui(row_times_editable, |ui| {
                    let mut end = state.days[i].end;
                    if show_time_field_inline(
                        ui,
                        &format!("{id_source}_d{i}_end"),
                        &mut end,
                        &mut state.row_end[i],
                    ) {
                        state.days[i].end = end;
                        changed = true;
                    }
                });
                ui.end_row();
            }
        });

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use awayd_core::schedule::default_week_days;

    #[test]
    fn from_default_days() {
        let days = default_week_days();
        let state = WeekdayScheduleGridState::from_days(&days);
        assert!(state.days[0].enabled);
        assert!(!state.days[6].enabled);
        assert!(state.linked);
    }

    #[test]
    fn infer_linked_false_when_hours_differ() {
        let mut days = default_week_days();
        days[1].start = NaiveTime::from_hms_opt(10, 0, 0).unwrap();
        assert!(!infer_linked(&days));
    }

    #[test]
    fn master_edit_does_not_change_table_until_commit() {
        let days = default_week_days();
        let mut state = WeekdayScheduleGridState::from_days(&days);
        let row_start_before = state.days[0].start;
        let row_edit_before = state.row_start[0].edit.clone();

        state.master_start.edit = "10:30".to_string();
        let committed = state.commit_days_for_save();

        assert_eq!(state.days[0].start, row_start_before);
        assert_eq!(state.row_start[0].edit, row_edit_before);
        assert_eq!(
            committed[0].start,
            NaiveTime::from_hms_opt(10, 30, 0).unwrap()
        );
        assert_eq!(committed[0].end, NaiveTime::from_hms_opt(17, 0, 0).unwrap());
    }

    #[test]
    fn custom_mode_commit_preserves_per_row_times() {
        let mut days = default_week_days();
        days[0].start = NaiveTime::from_hms_opt(8, 0, 0).unwrap();
        let mut state = WeekdayScheduleGridState::from_days(&days);
        state.linked = false;
        let committed = state.commit_days_for_save();
        assert_eq!(
            committed[0].start,
            NaiveTime::from_hms_opt(8, 0, 0).unwrap()
        );
    }
}
