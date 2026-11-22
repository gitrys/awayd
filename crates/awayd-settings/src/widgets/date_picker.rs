//! SPDX-License-Identifier: GPL-3.0-or-later
//! YYYY-MM-DD text field with calendar popup (month grid).

use chrono::{Datelike, NaiveDate};
use eframe::egui;

/// Persistent edit state for a single date field.
#[derive(Clone)]
pub struct DateFieldState {
    pub edit: String,
    focused: bool,
    view_year: i32,
    view_month: u32,
}

impl DateFieldState {
    pub fn from_date_str(s: &str) -> Self {
        let d = parse_date(s).unwrap_or_else(|| chrono::Local::now().date_naive());
        Self {
            edit: if s.trim().is_empty() {
                String::new()
            } else {
                format_date(d)
            },
            focused: false,
            view_year: d.year(),
            view_month: d.month(),
        }
    }

    pub fn sync_from_str(&mut self, s: &str) {
        if !self.focused {
            self.edit = s.to_string();
            if let Some(d) = parse_date(s) {
                self.view_year = d.year();
                self.view_month = d.month();
            }
        }
    }
}

fn format_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

fn first_weekday(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(year, month, 1)
        .map(|d| d.weekday().num_days_from_monday())
        .unwrap_or(0)
}

/// Compact date field: text edit + calendar button. Writes ISO date into `value`.
pub fn show_date_field(
    ui: &mut egui::Ui,
    id_source: &str,
    value: &mut String,
    state: &mut DateFieldState,
) {
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.edit)
                .id(ui.id().with(id_source).with("edit"))
                .desired_width(88.0)
                .hint_text("YYYY-MM-DD"),
        );

        if response.gained_focus() {
            state.focused = true;
        }
        if response.changed() {
            if let Some(d) = parse_date(&state.edit) {
                *value = format_date(d);
                state.view_year = d.year();
                state.view_month = d.month();
            }
        }
        if response.lost_focus() {
            state.focused = false;
            if let Some(d) = parse_date(&state.edit) {
                *value = format_date(d);
                state.edit = format_date(d);
            } else if value.trim().is_empty() {
                state.edit.clear();
            } else {
                state.edit = value.clone();
            }
        }

        let cal_id = ui.id().with(id_source).with("cal");
        let cal_response = ui.button("📅").on_hover_text("Pick date");
        if cal_response.clicked() {
            if let Some(d) = parse_date(&state.edit).or_else(|| parse_date(value)) {
                state.view_year = d.year();
                state.view_month = d.month();
            }
            ui.memory_mut(|m| m.open_popup(cal_id));
        }

        egui::popup::popup_below_widget(
            ui,
            cal_id,
            &cal_response,
            egui::popup::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(220.0);
                ui.horizontal(|ui| {
                    if ui.button("<").clicked() {
                        if state.view_month == 1 {
                            state.view_month = 12;
                            state.view_year -= 1;
                        } else {
                            state.view_month -= 1;
                        }
                    }
                    ui.label(format!("{}-{:02}", state.view_year, state.view_month));
                    if ui.button(">").clicked() {
                        if state.view_month == 12 {
                            state.view_month = 1;
                            state.view_year += 1;
                        } else {
                            state.view_month += 1;
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    for wd in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                        ui.label(wd);
                    }
                });
                let dim = days_in_month(state.view_year, state.view_month);
                let offset = first_weekday(state.view_year, state.view_month) as usize;
                let selected = parse_date(&state.edit).or_else(|| parse_date(value));
                let mut picked: Option<NaiveDate> = None;
                let total = offset + dim as usize;
                let rows = total.div_ceil(7);
                egui::Grid::new(ui.id().with(id_source).with("grid"))
                    .num_columns(7)
                    .spacing([4.0, 4.0])
                    .show(ui, |ui| {
                        let mut day = 1u32;
                        for row in 0..rows {
                            for col in 0..7 {
                                let cell = row * 7 + col;
                                if cell < offset || day > dim {
                                    ui.label("");
                                } else {
                                    let d = NaiveDate::from_ymd_opt(
                                        state.view_year,
                                        state.view_month,
                                        day,
                                    );
                                    let is_sel =
                                        selected.zip(d).map(|(a, b)| a == b).unwrap_or(false);
                                    let label = if is_sel {
                                        egui::RichText::new(format!("{day}")).strong()
                                    } else {
                                        egui::RichText::new(format!("{day}"))
                                    };
                                    if ui.button(label).clicked() {
                                        picked = d;
                                    }
                                    day += 1;
                                }
                            }
                            ui.end_row();
                        }
                    });
                if let Some(d) = picked {
                    *value = format_date(d);
                    state.edit = format_date(d);
                    ui.memory_mut(|m| m.close_popup());
                }
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iso() {
        assert!(parse_date("2026-07-04").is_some());
    }
}
