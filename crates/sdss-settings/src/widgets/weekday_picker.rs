//! SPDX-License-Identifier: GPL-3.0-or-later
//! Seven toggle buttons for ISO weekdays (1=Mon … 7=Sun).

use eframe::egui;
use sdss_core::schedule_util::parse_weekdays;

const LABELS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// Toggle state for each weekday (index 0 = Monday).
#[derive(Clone)]
pub struct WeekdayPickerState {
    pub days: [bool; 7],
}

impl WeekdayPickerState {
    pub fn from_spec(spec: &str) -> Self {
        let mut days = [false; 7];
        if let Ok(set) = parse_weekdays(spec) {
            for d in 1u8..=7 {
                days[(d - 1) as usize] = set.contains(&d);
            }
        } else {
            days[..5].fill(true);
        }
        Self { days }
    }

    pub fn sync_from_spec(&mut self, spec: &str) {
        *self = Self::from_spec(spec);
    }

    pub fn to_spec(&self) -> String {
        let days: Vec<u8> = (1u8..=7).filter(|&d| self.days[(d - 1) as usize]).collect();
        if days.is_empty() {
            return "1".into();
        }
        if days.len() == 7 {
            return "1-7".into();
        }
        days.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Draw weekday toggles; returns true if any day changed.
pub fn show_weekday_picker(
    ui: &mut egui::Ui,
    id_source: &str,
    state: &mut WeekdayPickerState,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for (i, label) in LABELS.iter().enumerate() {
            let selected = state.days[i];
            let btn = ui.add(
                egui::Button::new(*label)
                    .fill(if selected {
                        ui.visuals().selection.bg_fill
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .min_size(egui::vec2(34.0, 0.0)),
            );
            if btn.clicked() {
                state.days[i] = !state.days[i];
                changed = true;
            }
            let _ = ui.id().with(id_source).with(i);
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_mon_fri() {
        let s = WeekdayPickerState::from_spec("1-5");
        assert_eq!(s.to_spec(), "1,2,3,4,5");
    }
}
