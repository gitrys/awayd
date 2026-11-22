//! SPDX-License-Identifier: GPL-3.0-or-later

pub mod date_range_picker;
pub mod holiday_list_flyout;
pub mod searchable_picker;
pub mod subdiv_labels;
pub mod time_picker;
pub mod weekday_schedule_grid;

use eframe::egui;

/// Name an icon-only button for screen readers: AccessKit reads the button text, which is
/// only a symbol (🗑, 📅, 🕐, <, >) here.
pub fn accessible_name(response: egui::Response, name: &'static str) -> egui::Response {
    let enabled = response.enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    response
}
