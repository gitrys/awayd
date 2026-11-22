//! SPDX-License-Identifier: GPL-3.0-or-later
//! Flyout listing public holidays for the selected region and year.

use awayd_ipc::{HolidayRow, ListHolidaysResponse};
use eframe::egui;

/// Target number of holiday rows visible without scrolling.
const LIST_VISIBLE_ROWS: f32 = 12.0;

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
    last_query_key: String,
    popup_was_open: bool,
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

    /// Button + popup; returns an action for the host to fetch or refresh holiday data.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        button_width: f32,
        country: &str,
        subdiv: &str,
        subdiv_label: &str,
        daemon_ok: bool,
    ) -> HolidayFlyoutAction {
        let popup_id = ui.id().with("holiday_list_popup");
        let response = ui.add_sized(
            egui::vec2(button_width, ui.spacing().interact_size.y),
            egui::Button::new("View public holidays…"),
        );
        if response.clicked() {
            if ui.memory(|m| m.is_popup_open(popup_id)) {
                ui.memory_mut(|m| m.close_popup());
                self.popup_was_open = false;
            } else {
                ui.memory_mut(|m| m.open_popup(popup_id));
                self.popup_was_open = false;
            }
        }

        let mut action = HolidayFlyoutAction::None;
        let popup_open = egui::popup::popup_below_widget(
            ui,
            popup_id,
            &response,
            egui::popup::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(button_width.max(320.0));
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {} · {}",
                        country.to_uppercase(),
                        subdiv_label,
                        self.view_year
                    ))
                    .strong(),
                );

                ui.horizontal(|ui| {
                    let prev =
                        ui.add_enabled(self.view_year > self.year_min, egui::Button::new("◀"));
                    if prev.clicked() {
                        self.view_year -= 1;
                    }
                    ui.label(format!("{}", self.view_year));
                    let next =
                        ui.add_enabled(self.view_year < self.year_max, egui::Button::new("▶"));
                    if next.clicked() {
                        self.view_year += 1;
                    }
                });

                if !daemon_ok {
                    ui.colored_label(egui::Color32::YELLOW, "Daemon offline — start awayd first.");
                } else if let Some(err) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, err);
                } else if self.holidays.is_empty() {
                    ui.label("No public holidays for this year/region.");
                } else {
                    let row_h = ui.spacing().interact_size.y;
                    let viewport_h = row_h * LIST_VISIBLE_ROWS;
                    egui::ScrollArea::vertical()
                        .max_height(viewport_h)
                        .min_scrolled_height(viewport_h)
                        .show(ui, |ui| {
                            ui.set_row_height(row_h);
                            for h in &self.holidays {
                                let label = h.display_name();
                                if label == h.date {
                                    ui.label(label);
                                } else {
                                    ui.label(format!("{} — {label}", h.date));
                                }
                            }
                        });
                }

                if daemon_ok && self.stale_names() {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Holiday names appear after refreshing cached data.")
                            .weak()
                            .small(),
                    );
                    if ui.button("Refresh holiday data").clicked() {
                        action = HolidayFlyoutAction::RefreshCountry;
                    }
                }
            },
        )
        .is_some();

        if popup_open && !self.popup_was_open {
            self.popup_was_open = true;
        }
        if !popup_open {
            self.popup_was_open = false;
        }

        if action != HolidayFlyoutAction::None {
            return action;
        }
        if popup_open && daemon_ok && self.needs_reload(country, subdiv) {
            return HolidayFlyoutAction::Reload;
        }
        HolidayFlyoutAction::None
    }
}
