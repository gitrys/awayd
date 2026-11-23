//! SPDX-License-Identifier: GPL-3.0-or-later
//! Sidebar: app tile and navigation entries with line icons.

use eframe::egui;
use egui::{
    pos2, vec2, Color32, Painter, Pos2, Sense, Stroke, TextWrapMode, WidgetInfo, WidgetText,
    WidgetType,
};

use crate::theme;

const HEIGHT: f32 = 32.0;
const ICON: f32 = 16.0;

/// Line icons of the sidebar, drawn in a 16 px box.
#[derive(Clone, Copy)]
pub enum NavIcon {
    Clock,
    Calendar,
    Link,
    Sliders,
    List,
}

/// Full-width sidebar entry; the selected one gets a background and an accent bar. Screen
/// readers announce it as a selectable item with its selected state.
pub fn nav_item(ui: &mut egui::Ui, icon: NavIcon, label: &str, selected: bool) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, HEIGHT), Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::SelectableLabel, enabled, selected, label)
    });

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 4.0, p.nav_selected);
            let bar = egui::Rect::from_center_size(
                pos2(rect.left() + 1.5, rect.center().y),
                vec2(3.0, 16.0),
            );
            painter.rect_filled(bar, 1.5, p.accent);
        } else if response.hovered() {
            painter.rect_filled(rect, 4.0, p.nav_hover);
        }
        if response.has_focus() {
            painter.rect_stroke(rect.shrink(1.0), 4.0, Stroke::new(2.0_f32, p.text));
        }
        let icon_center = pos2(rect.left() + 12.0 + ICON / 2.0, rect.center().y);
        let bg = if selected { p.nav_selected } else { p.sidebar };
        draw_icon(painter, icon, icon_center, p.text, bg);

        let text = if selected {
            egui::RichText::new(label).strong()
        } else {
            egui::RichText::new(label)
        };
        let text_left = rect.left() + 12.0 + ICON + 12.0;
        let galley = WidgetText::from(text).into_galley(
            ui,
            Some(TextWrapMode::Truncate),
            rect.right() - text_left - 4.0,
            egui::TextStyle::Body,
        );
        let pos = pos2(text_left, rect.center().y - galley.size().y / 2.0);
        painter.galley(pos, galley, p.text);
    }
    response
}

/// Rounded accent tile with a half-filled circle, the app's mark in the sidebar.
pub fn app_tile(ui: &mut egui::Ui, size: f32) {
    let p = theme::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, size * 0.25, p.accent);
    let c = rect.center();
    let r = size * 0.3;
    painter.circle_stroke(c, r, Stroke::new(1.5_f32, p.on_accent));
    let half: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 12.0;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    painter.add(egui::Shape::convex_polygon(half, p.on_accent, Stroke::NONE));
}

fn draw_icon(painter: &Painter, icon: NavIcon, c: Pos2, color: Color32, bg: Color32) {
    let s = Stroke::new(1.3_f32, color);
    let at = |x: f32, y: f32| pos2(c.x + x, c.y + y);
    match icon {
        NavIcon::Clock => {
            painter.circle_stroke(c, 5.5, s);
            painter.line_segment([c, at(0.0, -3.0)], s);
            painter.line_segment([c, at(2.0, 1.5)], s);
        }
        NavIcon::Calendar => {
            let r = egui::Rect::from_min_max(at(-5.5, -4.5), at(5.5, 5.5));
            painter.rect_stroke(r, 1.5, s);
            painter.line_segment([at(-5.5, -1.5), at(5.5, -1.5)], s);
            painter.line_segment([at(-2.5, -6.0), at(-2.5, -3.0)], s);
            painter.line_segment([at(2.5, -6.0), at(2.5, -3.0)], s);
        }
        NavIcon::Link => {
            painter.circle_stroke(at(-2.5, 2.5), 3.0, s);
            painter.circle_stroke(at(2.5, -2.5), 3.0, s);
            painter.line_segment([at(-1.0, 1.0), at(1.0, -1.0)], s);
        }
        NavIcon::Sliders => {
            for (y, x) in [(-4.0, -2.5), (0.0, 2.5), (4.0, -1.0)] {
                painter.line_segment([at(-5.5, y), at(5.5, y)], s);
                painter.circle(at(x, y), 1.7, bg, s);
            }
        }
        NavIcon::List => {
            for y in [-4.0, 0.0, 4.0] {
                painter.line_segment([at(-2.0, y), at(5.5, y)], s);
                painter.circle_filled(at(-5.0, y), 0.9, color);
            }
        }
    }
}
