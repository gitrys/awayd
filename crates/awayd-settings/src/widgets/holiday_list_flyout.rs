//! SPDX-License-Identifier: GPL-3.0-or-later
//! Public holidays of the selected region and year, shown on the "Days off" page (was a
//! flyout before; the type names stay).

use awayd_ipc::{HolidayRow, ListHolidaysResponse};
use chrono::NaiveDate;
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HolidayFlyoutAction {
    None,
    Reload,
    RefreshCountry,
}

#[derive(Default)]
pub struct HolidayListFlyout {
    pub view_year: i32,
    pub year_min: i32,
    pub year_max: i32,
    pub holidays: Vec<HolidayRow>,
    pub error: Option<String>,
    /// A refresh of the holiday data is running (set by the host each frame).
    pub refreshing: bool,
    last_query_key: String,
}

impl HolidayListFlyout {
    pub fn new(initial_year: i32) -> Self {
        Self {
            view_year: initial_year,
            ..Default::default()
        }
    }

    pub fn apply_response(&mut self, country: &str, subdiv: &str, resp: ListHolidaysResponse) {
        self.year_min = resp.year_min;
        self.year_max = resp.year_max;
        self.holidays = resp.holidays;
        self.error = resp.error;
        self.last_query_key = Self::query_key(country, subdiv, self.view_year);
    }

    /// A failed query counts as done for this region and year, so the page (always visible)
    /// does not ask the daemon again in every frame.
    pub fn apply_error(&mut self, country: &str, subdiv: &str, error: String) {
        self.holidays.clear();
        self.error = Some(error);
        self.last_query_key = Self::query_key(country, subdiv, self.view_year);
    }

    pub fn clear_loaded(&mut self) {
        self.holidays.clear();
        self.error = None;
        self.last_query_key.clear();
    }

    fn query_key(country: &str, subdiv: &str, year: i32) -> String {
        format!(
            "{}|{}|{year}",
            country.to_uppercase(),
            subdiv.to_uppercase()
        )
    }

    fn needs_reload(&self, country: &str, subdiv: &str) -> bool {
        Self::query_key(country, subdiv, self.view_year) != self.last_query_key
    }

    fn stale_names(&self) -> bool {
        !self.holidays.is_empty() && self.holidays.iter().any(|h| !h.has_proper_name())
    }

    /// List with year navigation and "Refresh holiday data"; returns an action for the host
    /// to fetch or refresh holiday data.
    pub fn show_inline(
        &mut self,
        ui: &mut egui::Ui,
        country: &str,
        subdiv: &str,
        daemon_ok: bool,
    ) -> HolidayFlyoutAction {
        let p = crate::theme::palette(ui.ctx());
        let mut action = HolidayFlyoutAction::None;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Holidays in {}", self.view_year)).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let next = super::accessible_name(
                    ui.add_enabled(self.view_year < self.year_max, egui::Button::new("▶")),
                    "Next year",
                );
                if next.clicked() {
                    self.view_year += 1;
                }
                ui.label(format!("{}", self.view_year));
                let prev = super::accessible_name(
                    ui.add_enabled(self.view_year > self.year_min, egui::Button::new("◀")),
                    "Previous year",
                );
                if prev.clicked() {
                    self.view_year -= 1;
                }
            });
        });

        if !daemon_ok {
            ui.colored_label(
                p.warn_fg,
                "Daemon offline — start awayd to see the holidays.",
            );
        } else if let Some(err) = &self.error {
            ui.colored_label(p.error_fg, err);
        } else if self.holidays.is_empty() {
            ui.label("No public holidays for this year and region.");
        } else {
            let half = self.holidays.len().div_ceil(2);
            ui.columns(2, |cols| {
                for (col, chunk) in cols.iter_mut().zip(self.holidays.chunks(half)) {
                    for h in chunk {
                        col.horizontal(|ui| {
                            let (date, name) = row_text(h);
                            ui.add_sized(
                                egui::vec2(86.0, ui.spacing().interact_size.y * 0.8),
                                egui::Label::new(egui::RichText::new(date).color(p.text_secondary)),
                            );
                            ui.label(name);
                        });
                    }
                }
            });
        }

        if daemon_ok {
            super::card::divider(ui);
            ui.horizontal(|ui| {
                let data = if self.year_max > 0 {
                    format!(
                        "Holiday data from Nager.Date · {}–{}",
                        self.year_min, self.year_max
                    )
                } else {
                    "Holiday data from Nager.Date".to_string()
                };
                super::card::secondary_label(ui, data);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if self.refreshing {
                        "Refreshing holiday data…"
                    } else {
                        "Refresh holiday data"
                    };
                    if ui
                        .add_enabled(!self.refreshing, egui::Button::new(label))
                        .clicked()
                    {
                        action = HolidayFlyoutAction::RefreshCountry;
                    }
                    if self.refreshing {
                        ui.spinner();
                    }
                });
            });
            if self.stale_names() {
                super::card::secondary_label(
                    ui,
                    "Holiday names appear after refreshing the cached data.",
                );
            }
        }

        if action != HolidayFlyoutAction::None {
            return action;
        }
        if daemon_ok && self.needs_reload(country, subdiv) {
            return HolidayFlyoutAction::Reload;
        }
        HolidayFlyoutAction::None
    }
}

/// ("Thu 1 Jan", "New Year's Day"); the raw date if it cannot be parsed.
fn row_text(h: &HolidayRow) -> (String, String) {
    let date = NaiveDate::parse_from_str(&h.date, "%Y-%m-%d")
        .map(|d| d.format("%a %-d %b").to_string())
        .unwrap_or_else(|_| h.date.clone());
    let name = h.display_name();
    let name = if name == h.date {
        String::new()
    } else {
        name.to_string()
    };
    (date, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_query_is_not_repeated_for_the_same_year() {
        let mut list = HolidayListFlyout::new(2026);
        assert!(list.needs_reload("DE", "BW"));
        list.apply_error("DE", "BW", "offline".into());
        assert!(!list.needs_reload("DE", "BW"));
        list.view_year = 2027;
        assert!(list.needs_reload("DE", "BW"));
    }
}
