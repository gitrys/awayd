//! SPDX-License-Identifier: GPL-3.0-or-later
//! Two-click vacation date range picker (one calendar popup).

use chrono::{Datelike, NaiveDate};
use eframe::egui::{self, Align, Layout, Vec2};

const CELL_W: f32 = 28.0;
const CELL_H: f32 = 26.0;
const HINT: &str =
    "Click start, then end. Earlier click = new start. Right-click to clear. Same day = one day off.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickPhase {
    PickStart,
    PickEnd,
}

/// UI state for an in-progress or committed date range.
#[derive(Clone)]
pub struct DateRangeFieldState {
    view_year: i32,
    view_month: u32,
    tentative_start: Option<NaiveDate>,
    phase: PickPhase,
}

impl DateRangeFieldState {
    pub fn from_range(start: &str, end: &str) -> Self {
        let anchor = parse_date(start)
            .or_else(|| parse_date(end))
            .unwrap_or_else(|| chrono::Local::now().date_naive());
        Self {
            view_year: anchor.year(),
            view_month: anchor.month(),
            tentative_start: None,
            phase: PickPhase::PickStart,
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

fn first_weekday_offset(year: i32, month: u32) -> usize {
    NaiveDate::from_ymd_opt(year, month, 1)
        .map(|d| d.weekday().num_days_from_monday() as usize)
        .unwrap_or(0)
}

fn range_display(start: &str, end: &str) -> String {
    let s = start.trim();
    let e = end.trim();
    if s.is_empty() && e.is_empty() {
        return "—".to_string();
    }
    if s.is_empty() {
        return e.to_string();
    }
    if e.is_empty() || s == e {
        return s.to_string();
    }
    format!("{s} – {e}")
}

fn in_committed_range(d: NaiveDate, start: &str, end: &str) -> bool {
    match (parse_date(start), parse_date(end)) {
        (Some(s), Some(e)) => d >= s && d <= e,
        _ => false,
    }
}

fn is_picking_in_progress(state: &DateRangeFieldState) -> bool {
    state.tentative_start.is_some() || state.phase == PickPhase::PickEnd
}

fn clear_in_progress_pick(state: &mut DateRangeFieldState) {
    state.tentative_start = None;
    state.phase = PickPhase::PickStart;
}

fn day_secondary_clicked(response: &egui::Response, ui: &egui::Ui) -> bool {
    response.secondary_clicked()
        || (response.hovered()
            && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary)))
}

fn handle_day_secondary(
    state: &mut DateRangeFieldState,
    start: &mut String,
    end: &mut String,
) {
    if is_picking_in_progress(state) {
        clear_in_progress_pick(state);
    } else if !start.trim().is_empty() || !end.trim().is_empty() {
        start.clear();
        end.clear();
    }
}

fn day_cell_style(
    d: NaiveDate,
    tentative: Option<NaiveDate>,
    start: &str,
    end: &str,
    visuals: &egui::Visuals,
) -> (egui::Color32, bool) {
    if let Some(ts) = tentative {
        if d == ts {
            return (visuals.selection.bg_fill, true);
        }
    }
    if let (Some(s), Some(e)) = (parse_date(start), parse_date(end)) {
        if d == s || d == e {
            return (visuals.selection.bg_fill, true);
        }
        if d > s && d < e {
            return (visuals.widgets.inactive.bg_fill, false);
        }
    } else if in_committed_range(d, start, end) {
        return (visuals.widgets.inactive.bg_fill, false);
    }
    (egui::Color32::TRANSPARENT, false)
}

/// Name row segment: range label + calendar button. Returns true when range is committed.
pub fn show_date_range_field(
    ui: &mut egui::Ui,
    id_source: &str,
    start: &mut String,
    end: &mut String,
    state: &mut DateRangeFieldState,
) -> bool {
    let mut committed = false;
    ui.horizontal(|ui| {
        let text = range_display(start, end);
        ui.label(text);
        let cal_id = ui.id().with(id_source).with("cal");
        let cal_response = ui.button("📅").on_hover_text("Pick date range");
        if cal_response.clicked() {
            state.tentative_start = None;
            state.phase = PickPhase::PickStart;
            if let Some(d) = parse_date(start).or_else(|| parse_date(end)) {
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
                ui.set_min_width(7.0 * CELL_W + 16.0);
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
                ui.label(HINT);
                ui.separator();

                let dim = days_in_month(state.view_year, state.view_month);
                let offset = first_weekday_offset(state.view_year, state.view_month);
                let total = offset + dim as usize;
                let rows = total.div_ceil(7);

                egui::Grid::new(ui.id().with(id_source).with("grid"))
                    .num_columns(7)
                    .spacing([2.0, 2.0])
                    .show(ui, |ui| {
                        for wd in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                            ui.allocate_ui_with_layout(
                                Vec2::new(CELL_W, 16.0),
                                Layout::top_down(Align::Center),
                                |ui| {
                                    ui.label(wd);
                                },
                            );
                        }
                        ui.end_row();

                        let mut day = 1u32;
                        for row in 0..rows {
                            for col in 0..7 {
                                let cell = row * 7 + col;
                                if cell < offset || day > dim {
                                    ui.allocate_ui(Vec2::new(CELL_W, CELL_H), |ui| {
                                        ui.label("");
                                    });
                                } else {
                                    let d = NaiveDate::from_ymd_opt(
                                        state.view_year,
                                        state.view_month,
                                        day,
                                    )
                                    .expect("valid day");
                                    let (fill, strong) =
                                        day_cell_style(d, state.tentative_start, start, end, ui.visuals());
                                    let label = if strong {
                                        egui::RichText::new(format!("{day}")).strong()
                                    } else {
                                        egui::RichText::new(format!("{day}"))
                                    };
                                    let btn = ui.add(
                                        egui::Button::new(label)
                                            .fill(fill)
                                            .min_size(Vec2::new(CELL_W, CELL_H)),
                                    );
                                    if day_secondary_clicked(&btn, ui) {
                                        handle_day_secondary(state, start, end);
                                    } else if btn.clicked() {
                                        match state.phase {
                                            PickPhase::PickStart => {
                                                state.tentative_start = Some(d);
                                                state.phase = PickPhase::PickEnd;
                                            }
                                            PickPhase::PickEnd => {
                                                let Some(ts) = state.tentative_start else {
                                                    state.tentative_start = Some(d);
                                                    state.phase = PickPhase::PickEnd;
                                                    day += 1;
                                                    continue;
                                                };
                                                if d < ts {
                                                    state.tentative_start = Some(d);
                                                } else {
                                                    *start = format_date(ts);
                                                    *end = format_date(d);
                                                    state.tentative_start = None;
                                                    state.phase = PickPhase::PickStart;
                                                    committed = true;
                                                    ui.memory_mut(|m| m.close_popup());
                                                }
                                            }
                                        }
                                    }
                                    day += 1;
                                }
                            }
                            ui.end_row();
                        }
                    });

                if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary))
                    && is_picking_in_progress(state)
                {
                    clear_in_progress_pick(state);
                }

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    clear_in_progress_pick(state);
                    ui.memory_mut(|m| m.close_popup());
                }
            },
        );

        if !ui.memory(|m| m.is_popup_open(cal_id)) && !committed {
            state.tentative_start = None;
            state.phase = PickPhase::PickStart;
        }
    });
    committed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_display_formats() {
        assert_eq!(range_display("2026-06-10", "2026-06-15"), "2026-06-10 – 2026-06-15");
        assert_eq!(range_display("2026-06-10", "2026-06-10"), "2026-06-10");
    }

    #[test]
    fn is_picking_after_reanchored_start() {
        let d = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let mut state = DateRangeFieldState::from_range("", "");
        state.tentative_start = Some(d);
        state.phase = PickPhase::PickEnd;
        assert!(is_picking_in_progress(&state));
    }

    #[test]
    fn secondary_clear_after_reanchored_start() {
        let early = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let mut state = DateRangeFieldState::from_range("2026-06-10", "2026-06-15");
        state.tentative_start = Some(early);
        state.phase = PickPhase::PickEnd;
        let mut start = "2026-06-10".to_string();
        let mut end = "2026-06-15".to_string();
        handle_day_secondary(&mut state, &mut start, &mut end);
        assert!(state.tentative_start.is_none());
        assert_eq!(state.phase, PickPhase::PickStart);
        assert_eq!(start, "2026-06-10");
        assert_eq!(end, "2026-06-15");
    }

    #[test]
    fn secondary_clear_committed_when_idle() {
        let mut state = DateRangeFieldState::from_range("2026-06-10", "2026-06-15");
        let mut start = "2026-06-10".to_string();
        let mut end = "2026-06-15".to_string();
        handle_day_secondary(&mut state, &mut start, &mut end);
        assert!(start.is_empty());
        assert!(end.is_empty());
    }
}
