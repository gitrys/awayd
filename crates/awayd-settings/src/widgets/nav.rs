//! SPDX-License-Identifier: GPL-3.0-or-later
//! Entry of the sidebar navigation.

use eframe::egui;
use egui::{Sense, Stroke, TextWrapMode, WidgetInfo, WidgetText, WidgetType};

use crate::theme;

const HEIGHT: f32 = 36.0;

/// Full-width sidebar entry; the selected one gets a background and an accent bar. Screen
/// readers announce it as a selectable item with its selected state.
pub fn nav_item(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, HEIGHT), Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::SelectableLabel, enabled, selected, label)
    });

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 4.0, p.nav_selected);
            let bar = egui::Rect::from_center_size(
                egui::pos2(rect.left() + 1.5, rect.center().y),
                egui::vec2(3.0, 16.0),
            );
            painter.rect_filled(bar, 1.5, p.accent);
        } else if response.hovered() {
            painter.rect_filled(rect, 4.0, p.nav_hover);
        }
        if response.has_focus() {
            painter.rect_stroke(rect.shrink(1.0), 4.0, Stroke::new(2.0_f32, p.text));
        }
        let text = if selected {
            egui::RichText::new(label).strong()
        } else {
            egui::RichText::new(label)
        };
        let galley = WidgetText::from(text).into_galley(
            ui,
            Some(TextWrapMode::Truncate),
            width - 24.0,
            egui::TextStyle::Body,
        );
        let pos = egui::pos2(rect.left() + 14.0, rect.center().y - galley.size().y / 2.0);
        painter.galley(pos, galley, p.text);
    }
    response
}
