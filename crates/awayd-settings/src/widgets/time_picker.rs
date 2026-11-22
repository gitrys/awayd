//! SPDX-License-Identifier: GPL-3.0-or-later
//! HH:MM text field with persistent edit buffer and slider clock popup.

use chrono::{NaiveTime, Timelike};
use eframe::egui;

/// Persistent edit state for a single time field.
#[derive(Clone)]
pub struct TimeFieldState {
    pub edit: String,
    focused: bool,
    popup_hour: i32,
    popup_minute: i32,
}

impl TimeFieldState {
    pub fn from_time(time: NaiveTime) -> Self {
        Self {
            edit: format_time(time),
            focused: false,
            popup_hour: time.hour() as i32,
            popup_minute: time.minute() as i32,
        }
    }

    pub fn sync_from_time(&mut self, time: NaiveTime) {
        if !self.focused {
            self.edit = format_time(time);
            self.popup_hour = time.hour() as i32;
            self.popup_minute = time.minute() as i32;
        }
    }
}

fn format_time(time: NaiveTime) -> String {
    format!("{:02}:{:02}", time.hour(), time.minute())
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    let t = s.trim();
    NaiveTime::parse_from_str(t, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(t, "%H:%M:%S"))
        .ok()
}

/// Label above time edit + clock button. Returns true when time changed (blur or OK).
pub fn show_time_field(
    ui: &mut egui::Ui,
    id_source: &str,
    label: &str,
    time: &mut NaiveTime,
    state: &mut TimeFieldState,
) -> bool {
    ui.vertical(|ui| {
        if !label.is_empty() {
            ui.label(label);
        }
        show_time_field_inline(ui, id_source, time, state)
    })
    .inner
}

/// Single-line time edit + clock button (for grids and toolbar rows).
pub fn show_time_field_inline(
    ui: &mut egui::Ui,
    id_source: &str,
    time: &mut NaiveTime,
    state: &mut TimeFieldState,
) -> bool {
    let mut changed = false;
    let before = *time;
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.edit)
                .id(ui.id().with(id_source).with("edit"))
                .desired_width(56.0),
        );

        if response.gained_focus() {
            state.focused = true;
        }
        // Typing only edits the buffer: intermediate values such as "16:3" (= 16:03) while
        // typing "16:30" must not be saved and applied. The value is taken on Enter / leaving
        // the field, or with "OK" in the clock popup.
        if response.lost_focus() {
            state.focused = false;
            if let Some(t) = parse_time(&state.edit) {
                *time = t;
                state.edit = format_time(t);
            } else {
                state.edit = format_time(*time);
            }
        }

        let clock_id = ui.id().with(id_source).with("clock");
        let clock_response = ui.button("🕐").on_hover_text("Pick time (drag sliders)");
        if clock_response.clicked() {
            if let Some(t) = parse_time(&state.edit) {
                state.popup_hour = t.hour() as i32;
                state.popup_minute = t.minute() as i32;
            }
            ui.memory_mut(|m| m.open_popup(clock_id));
        }

        let mut ok_clicked = false;
        egui::popup::popup_below_widget(
            ui,
            clock_id,
            &clock_response,
            egui::popup::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(260.0);
                ui.spacing_mut().slider_width = 140.0;
                ui.label("Time");
                ui.add(egui::Slider::new(&mut state.popup_hour, 0..=23).text("Hour"));
                ui.add(egui::Slider::new(&mut state.popup_minute, 0..=59).text("Min"));
                ui.label("Drag sliders to adjust");
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        state.popup_hour = state.popup_hour.clamp(0, 23);
                        state.popup_minute = state.popup_minute.clamp(0, 59);
                        if let Some(t) = NaiveTime::from_hms_opt(
                            state.popup_hour as u32,
                            state.popup_minute as u32,
                            0,
                        ) {
                            *time = t;
                            state.edit = format_time(t);
                        }
                        ok_clicked = true;
                        ui.memory_mut(|m| m.close_popup());
                    }
                    if ui.button("Cancel").clicked() {
                        ui.memory_mut(|m| m.close_popup());
                    }
                });
            },
        );
        if ok_clicked {
            changed = true;
        }
    });
    if !changed && *time != before {
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hhmm() {
        assert!(parse_time("09:30").is_some());
        assert!(parse_time("invalid").is_none());
    }
}
