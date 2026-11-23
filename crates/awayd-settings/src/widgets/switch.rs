//! SPDX-License-Identifier: GPL-3.0-or-later
//! On/off switch in the style of the Windows settings app.

use eframe::egui;
use egui::{Sense, Stroke, TextWrapMode, WidgetInfo, WidgetText, WidgetType};

use crate::theme;

const TRACK: egui::Vec2 = egui::vec2(40.0, 20.0);
const GAP: f32 = 10.0;

/// Switch with its state as text ("On" / "Off") beside it. `name` is what screen readers
/// announce (the visible label sits elsewhere in the row). Toggles on click, Space and Enter;
/// `changed()` is true in the frame it toggled.
pub fn switch(ui: &mut egui::Ui, on: &mut bool, name: &str) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let state_text = if *on { "On" } else { "Off" };
    let galley = WidgetText::from(state_text).into_galley(
        ui,
        Some(TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    let height = ui.spacing().interact_size.y.max(TRACK.y);
    let size = egui::vec2(TRACK.x + GAP + galley.size().x, height);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let now_on = *on;
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, now_on, name));

    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool(response.id, now_on);
        let track = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.center().y - TRACK.y / 2.0),
            TRACK,
        );
        let dim = |c: egui::Color32| if enabled { c } else { c.gamma_multiply(0.45) };
        let (fill, stroke, knob) = if now_on {
            (dim(p.accent), Stroke::NONE, dim(p.on_accent))
        } else {
            (
                egui::Color32::TRANSPARENT,
                Stroke::new(1.0_f32, dim(p.text_secondary)),
                dim(p.text_secondary),
            )
        };
        let painter = ui.painter();
        painter.rect(track, TRACK.y / 2.0, fill, stroke);
        let radius = if response.hovered() && enabled {
            6.0
        } else {
            5.0
        };
        let x = egui::lerp(
            (track.left() + TRACK.y / 2.0)..=(track.right() - TRACK.y / 2.0),
            t,
        );
        painter.circle_filled(egui::pos2(x, track.center().y), radius, knob);
        if response.has_focus() {
            painter.rect_stroke(
                track.expand(3.0),
                TRACK.y / 2.0 + 3.0,
                Stroke::new(2.0_f32, p.text),
            );
        }
        let text_pos = egui::pos2(track.right() + GAP, rect.center().y - galley.size().y / 2.0);
        painter.galley(text_pos, galley, dim(p.text));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame with `events`; returns the switch's rect and whether it changed.
    fn frame(ctx: &egui::Context, on: &mut bool, events: Vec<egui::Event>) -> (egui::Rect, bool) {
        let mut out = (egui::Rect::NOTHING, false);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 200.0),
            )),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = switch(ui, on, "Steam");
                out = (r.rect, r.changed());
            });
        });
        out
    }

    #[test]
    fn click_toggles_and_reports_change() {
        let ctx = egui::Context::default();
        let mut on = false;
        let (rect, changed) = frame(&ctx, &mut on, vec![]);
        assert!(!changed);
        let pos = rect.center();
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut on,
            vec![egui::Event::PointerMoved(pos), press(true)],
        );
        let (_, changed) = frame(&ctx, &mut on, vec![press(false)]);
        assert!(changed);
        assert!(on);
    }
}
